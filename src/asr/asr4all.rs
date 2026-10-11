use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ndarray::{Array1, Array2, Array3};
use ort::environment::GlobalThreadPoolOptions;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::{DynValue, Tensor};
use serde_json::Value;

use super::engine::{AsrEngine, AsrPartial, AsrSession};
use super::pcec::{render_slots, PcecConfig, PcecState};

type BoxErr = Box<dyn std::error::Error>;

/// Files the streaming pipeline needs, with minimum sizes used as a cheap integrity check.
pub const MODEL_FILES: &[(&str, u64)] = &[
    ("metadata.json", 1_000),
    ("asr4all.onnx.data", 100_000_000),
    ("stream_c16r4.onnx", 100_000),
    ("pcec_c16.onnx", 50_000),
    ("pcec_c16_flush.onnx", 50_000),
];

const SPF: usize = 640; // samples per encoder frame (40 ms)
const WINDOW: usize = 22_400; // samples per streaming window
const COMMIT_FRAMES: usize = 16; // frames committed per step
const FILTER_CACHE: usize = 1_006_080;
const ATTN_CACHE: usize = 329_090;
const PCEC_STATE: usize = 166_976;

/// Load ONNX Runtime from next to the exe and configure a small, shared, non-spinning thread pool.
/// Spinning threads are ORT's default and would burn CPU while the user is merely talking.
pub fn init_runtime(threads: usize) -> Result<(), String> {
    let dll = crate::paths::find_resource("onnxruntime.dll")
        .or_else(|| crate::paths::find_resource("models/onnxruntime.dll"))
        .ok_or("onnxruntime.dll was not found next to echo.exe. Please reinstall Echo.")?;
    let pool = GlobalThreadPoolOptions::default()
        .with_intra_threads(threads.max(1))
        .and_then(|p| p.with_inter_threads(1))
        .and_then(|p| p.with_spin_control(false))
        .and_then(|p| p.with_flush_to_zero())
        .map_err(|e| e.to_string())?;
    ort::init_from(&dll)
        .map_err(|e| format!("Could not load {}: {e}", dll.display()))?
        .with_name("echo")
        .with_telemetry(false)
        .with_global_thread_pool(pool)
        .commit();
    Ok(())
}

pub struct Asr4AllEngine {
    model_dir: PathBuf,
    vocab: Vec<String>,
    blank_id: usize,
    pcec_config: PcecConfig,
}

impl Asr4AllEngine {
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Result<Self, BoxErr> {
        let dir = model_dir.as_ref().to_path_buf();
        for (name, min) in MODEL_FILES {
            let p = dir.join(name);
            match fs::metadata(&p) {
                Ok(m) if m.len() >= *min => {}
                Ok(_) => return Err(format!("Model file is incomplete or corrupt: {}", p.display()).into()),
                Err(_) => return Err(format!("Model file is missing: {}", p.display()).into()),
            }
        }

        let metadata: Value = serde_json::from_str(&fs::read_to_string(dir.join("metadata.json"))?)?;
        let vocab: Vec<String> = metadata["vocab"]
            .as_array()
            .ok_or("metadata.json: missing vocab")?
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();
        let blank_id = metadata["blank_id"].as_u64().unwrap_or(256) as usize;

        let pm = &metadata["pcec"];
        let ctrl = &pm["ctrl"];
        let id = |v: &Value, d: u64| v.as_u64().unwrap_or(d) as usize;
        let pcec_config = PcecConfig {
            commit: 16,
            lookahead: 16,
            pad_in: pm["pad_in"].as_i64().unwrap_or(256),
            pause_blackout: pm["pause_blackout"].as_i64().unwrap_or(9),
            real_pause: pm["real_pause"].as_bool().unwrap_or(true),
            hidden_dim: 384,
            vocab: pm["vocab"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect())
                .unwrap_or_else(|| vocab.clone()),
            blank_id: id(&pm["blank_id"], 6149),
            cap_id: id(&ctrl["cap"], 6144),
            allcaps_id: id(&ctrl["allcaps"], 6145),
            period_id: id(&ctrl["period"], 6146),
            comma_id: id(&ctrl["comma"], 6147),
            question_id: id(&ctrl["question"], 6148),
        };

        Ok(Self { model_dir: dir, vocab, blank_id, pcec_config })
    }
}

fn build_session(path: &Path) -> Result<Session, BoxErr> {
    let mut builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?;
    Ok(builder.commit_from_file(path)?)
}

fn zeros(n: usize) -> Result<DynValue, BoxErr> {
    Ok(Tensor::from_array(Array1::<f32>::zeros(n))?.into_dyn())
}

impl AsrEngine for Asr4AllEngine {
    fn create_session(&self) -> Result<Box<dyn AsrSession>, BoxErr> {
        let c = &self.pcec_config;
        let ctrl_map: HashMap<String, usize> = [
            ("cap", c.cap_id),
            ("allcaps", c.allcaps_id),
            ("period", c.period_id),
            ("comma", c.comma_id),
            ("question", c.question_id),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();

        Ok(Box::new(Asr4AllSession {
            stream: build_session(&self.model_dir.join("stream_c16r4.onnx"))?,
            pcec: build_session(&self.model_dir.join("pcec_c16.onnx"))?,
            flush: build_session(&self.model_dir.join("pcec_c16_flush.onnx"))?,
            vocab: self.vocab.clone(),
            blank_id: self.blank_id,
            pcec_config: c.clone(),
            ctrl_map,
            pcec_state: PcecState::new(c.clone(), PCEC_STATE),
            filter_cache: zeros(FILTER_CACHE)?,
            attn_cache: zeros(ATTN_CACHE)?,
            pcm: Vec::with_capacity(WINDOW * 2),
            pcm_offset: 0,
            next_frame: 0,
            ids: Vec::new(),
        }))
    }

    fn name(&self) -> &str {
        "FUTO asr4all-s (streaming c16r4)"
    }
}

pub struct Asr4AllSession {
    stream: Session,
    pcec: Session,
    flush: Session,
    vocab: Vec<String>,
    blank_id: usize,
    pcec_config: PcecConfig,
    ctrl_map: HashMap<String, usize>,
    pcec_state: PcecState,
    // Encoder caches stay as ORT values and are fed straight back in (no 5 MB copies per step).
    filter_cache: DynValue,
    attn_cache: DynValue,
    // Only the not-yet-encoded audio is kept, so memory stays bounded in long sessions.
    pcm: Vec<f32>,
    pcm_offset: usize, // absolute sample index of pcm[0]
    next_frame: usize,
    ids: Vec<i64>,
}

impl Asr4AllSession {
    fn total_samples(&self) -> usize {
        self.pcm_offset + self.pcm.len()
    }

    fn step(&mut self) -> Result<(), BoxErr> {
        let start = self.next_frame * SPF - self.pcm_offset;
        let mut win = vec![0.0f32; WINDOW];
        let n = self.pcm.len().saturating_sub(start).min(WINDOW);
        win[..n].copy_from_slice(&self.pcm[start..start + n]);

        let valid = COMMIT_FRAMES.min((self.total_samples() / SPF).saturating_sub(self.next_frame));

        let pcm = Tensor::from_array(Array2::from_shape_vec((1, WINDOW), win)?)?;
        let mut outputs = self.stream.run(ort::inputs![
            "stream_c16r4_pcm" => pcm,
            "filter_cache" => &self.filter_cache,
            "attn_cache" => &self.attn_cache,
        ])?;

        let new_filter = outputs.remove("stream_c16r4_filter_cache_out").ok_or("missing filter cache output")?;
        let new_attn = outputs.remove("stream_c16r4_attn_cache_out").ok_or("missing attn cache output")?;

        let (lp_shape, lp) = outputs["stream_c16r4_log_probs"].try_extract_tensor::<f32>()?;
        let vocab_dim = lp_shape[2] as usize;
        let frame_ids: Vec<i64> = (0..valid)
            .map(|t| {
                let row = &lp[t * vocab_dim..(t + 1) * vocab_dim];
                let mut best = 0;
                for (i, &v) in row.iter().enumerate() {
                    if v > row[best] {
                        best = i;
                    }
                }
                best as i64
            })
            .collect();

        let hidden = if frame_ids.is_empty() {
            Vec::new()
        } else {
            outputs["stream_c16r4_hidden"].try_extract_tensor::<f32>()?.1.to_vec()
        };
        drop(outputs);
        self.filter_cache = new_filter;
        self.attn_cache = new_attn;

        if !frame_ids.is_empty() {
            self.ids.extend_from_slice(&frame_ids);
            self.pcec_state.pool(&frame_ids, &hidden, self.pcec_config.hidden_dim, self.blank_id as i64);
            self.drain_pcec()?;
        }

        self.next_frame += COMMIT_FRAMES;
        let consumed = (self.next_frame * SPF).saturating_sub(self.pcm_offset).min(self.pcm.len());
        self.pcm.drain(..consumed);
        self.pcm_offset += consumed;
        Ok(())
    }

    fn drain_pcec(&mut self) -> Result<(), BoxErr> {
        let k = self.pcec_config.commit;
        while self.pcec_state.ids.len() >= k {
            let (hyp, pause, acoustic, keep) = self.build_pcec_feats(k, k);
            let state = Array2::from_shape_vec((1, self.pcec_state.state_cache.len()), self.pcec_state.state_cache.clone())?;
            let seen = Array1::from_vec(vec![self.pcec_state.seen_cache]);

            let outs = self.pcec.run(ort::inputs![
                "pcec_c16_hyp_ids" => Tensor::from_array(hyp)?,
                "pcec_c16_pause" => Tensor::from_array(pause)?,
                "pcec_c16_acoustic" => Tensor::from_array(acoustic)?,
                "pcec_c16_keep" => Tensor::from_array(keep)?,
                "pcec_c16_state" => Tensor::from_array(state)?,
                "pcec_c16_seen" => Tensor::from_array(seen)?,
            ])?;
            let slots = outs["pcec_c16_slots"].try_extract_tensor::<i64>()?.1.to_vec();
            let (_, state_out) = outs["pcec_c16_state_out"].try_extract_tensor::<f32>()?;
            self.pcec_state.state_cache.copy_from_slice(state_out);
            self.pcec_state.seen_cache = outs["pcec_c16_seen_out"].try_extract_tensor::<i64>()?.1[0];
            drop(outs);

            let skip = (2 * k).min((2 * self.pcec_config.lookahead).saturating_sub(2 * self.pcec_state.consumed));
            let end = (2 * k).min(slots.len());
            if skip < end {
                self.pcec_state.slots.extend_from_slice(&slots[skip..end]);
            }
            self.pcec_state.ids.drain(0..k);
            self.pcec_state.pause.drain(0..k);
            self.pcec_state.acoustic.drain(0..k);
            self.pcec_state.keep.drain(0..k);
            self.pcec_state.consumed += k;
        }
        Ok(())
    }

    fn build_pcec_feats(&self, k: usize, n: usize) -> (Array2<i64>, Array2<i64>, Array3<f32>, Array2<f32>) {
        let c = &self.pcec_config;
        let s = &self.pcec_state;
        let mut ids = vec![c.pad_in; k];
        let mut pause = vec![c.pause_blackout; k];
        let mut acoustic = vec![0.0f32; k * c.hidden_dim];
        let mut keep = vec![0.0f32; k];
        for i in 0..n {
            ids[i] = s.ids[i];
            if c.real_pause {
                pause[i] = s.pause[i];
            }
            acoustic[i * c.hidden_dim..(i + 1) * c.hidden_dim].copy_from_slice(&s.acoustic[i]);
            keep[i] = s.keep[i];
        }
        (
            Array2::from_shape_vec((1, k), ids).unwrap(),
            Array2::from_shape_vec((1, k), pause).unwrap(),
            Array3::from_shape_vec((1, k, c.hidden_dim), acoustic).unwrap(),
            Array2::from_shape_vec((1, k), keep).unwrap(),
        )
    }

    fn raw_ctc_text(&self) -> String {
        let mut out = String::new();
        let mut prev = -1i64;
        for &id in &self.ids {
            if id != prev && id != self.blank_id as i64 && (id as usize) < self.vocab.len() {
                out.push_str(&self.vocab[id as usize]);
            }
            prev = id;
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

impl AsrSession for Asr4AllSession {
    fn accept_audio(&mut self, samples: &[f32]) -> Result<(), BoxErr> {
        self.pcm.extend_from_slice(samples);
        while self.next_frame * SPF + WINDOW <= self.total_samples() {
            self.step()?;
        }
        Ok(())
    }

    fn partial_text(&self) -> String {
        let f = render_slots(&self.pcec_config.vocab, self.pcec_config.blank_id, &self.ctrl_map, &self.pcec_state.slots);
        if f.is_empty() {
            self.raw_ctc_text()
        } else {
            f
        }
    }

    fn finish(&mut self) -> Result<AsrPartial, BoxErr> {
        while self.next_frame * SPF < self.total_samples() {
            self.step()?;
        }
        self.pcec_state.flush_open();
        self.drain_pcec()?;

        let n_real = self.pcec_state.ids.len();
        let k = self.pcec_config.commit;
        let (hyp, pause, acoustic, keep) = self.build_pcec_feats(k, n_real);
        let state = Array2::from_shape_vec((1, self.pcec_state.state_cache.len()), self.pcec_state.state_cache.clone())?;
        let seen = Array1::from_vec(vec![self.pcec_state.seen_cache]);
        let n_real_arr = Array1::from_vec(vec![n_real as i64]);

        let outs = self.flush.run(ort::inputs![
            "pcec_c16_hyp_ids" => Tensor::from_array(hyp)?,
            "pcec_c16_pause" => Tensor::from_array(pause)?,
            "pcec_c16_acoustic" => Tensor::from_array(acoustic)?,
            "pcec_c16_keep" => Tensor::from_array(keep)?,
            "pcec_c16_n_real" => Tensor::from_array(n_real_arr)?,
            "pcec_c16_state" => Tensor::from_array(state)?,
            "pcec_c16_seen" => Tensor::from_array(seen)?,
        ])?;
        let tail = outs["pcec_c16_tail_slots"].try_extract_tensor::<i64>()?.1.to_vec();
        drop(outs);

        let lookahead = self.pcec_config.lookahead;
        let skip = (2 * lookahead).saturating_sub(2 * self.pcec_state.consumed);
        let valid = (2 * lookahead + 2 * n_real + 1).min(tail.len());
        let mut slots = self.pcec_state.slots.clone();
        if skip < valid {
            slots.extend_from_slice(&tail[skip..valid]);
        }

        let raw = self.raw_ctc_text();
        let mut formatted = render_slots(&self.pcec_config.vocab, self.pcec_config.blank_id, &self.ctrl_map, &slots);
        if formatted.is_empty() {
            formatted = raw.clone();
        }
        Ok(AsrPartial { raw_ctc: raw, formatted })
    }

    fn reset(&mut self) {
        if let (Ok(f), Ok(a)) = (zeros(FILTER_CACHE), zeros(ATTN_CACHE)) {
            self.filter_cache = f;
            self.attn_cache = a;
        }
        self.pcm.clear();
        self.pcm_offset = 0;
        self.next_frame = 0;
        self.ids.clear();
        self.pcec_state.reset();
    }
}
