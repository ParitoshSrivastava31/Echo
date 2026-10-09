use std::collections::HashMap;
use std::fs;
use std::path::Path;
use ndarray::{Array1, Array2, Array3};
use ort::session::Session;
use ort::value::Tensor;
use serde_json::Value;

use super::engine::{AsrEngine, AsrPartial, AsrSession};
use super::pcec::{render_slots, PcecConfig, PcecState};

#[allow(dead_code)]
pub struct Asr4AllEngine {
    model_dir: String,
    metadata: Value,
    vocab: Vec<String>,
    blank_id: usize,
    pcec_config: PcecConfig,
}

impl Asr4AllEngine {
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Result<Self, Box<dyn std::error::Error>> {
        let dir_str = model_dir.as_ref().to_string_lossy().to_string();
        let meta_path = model_dir.as_ref().join("metadata.json");
        let meta_content = fs::read_to_string(&meta_path)?;
        let metadata: Value = serde_json::from_str(&meta_content)?;

        let vocab: Vec<String> = metadata["vocab"]
            .as_array()
            .ok_or("missing vocab")?
            .iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect();

        let blank_id = metadata["blank_id"].as_u64().unwrap_or(256) as usize;

        let pcec_meta = &metadata["pcec"];
        let pcec_vocab: Vec<String> = pcec_meta["vocab"]
            .as_array()
            .map(|arr| arr.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect())
            .unwrap_or_else(|| vocab.clone());

        let ctrl = &pcec_meta["ctrl"];
        let cap_id = ctrl["cap"].as_u64().unwrap_or(6144) as usize;
        let allcaps_id = ctrl["allcaps"].as_u64().unwrap_or(6145) as usize;
        let period_id = ctrl["period"].as_u64().unwrap_or(6146) as usize;
        let comma_id = ctrl["comma"].as_u64().unwrap_or(6147) as usize;
        let question_id = ctrl["question"].as_u64().unwrap_or(6148) as usize;
        let pcec_blank = pcec_meta["blank_id"].as_u64().unwrap_or(6149) as usize;

        let pcec_config = PcecConfig {
            commit: 16,
            lookahead: 16,
            pad_in: pcec_meta["pad_in"].as_i64().unwrap_or(256),
            pause_blackout: pcec_meta["pause_blackout"].as_i64().unwrap_or(9),
            real_pause: pcec_meta["real_pause"].as_bool().unwrap_or(true),
            hidden_dim: 384,
            vocab: pcec_vocab,
            blank_id: pcec_blank,
            cap_id,
            allcaps_id,
            period_id,
            comma_id,
            question_id,
        };

        Ok(Self {
            model_dir: dir_str,
            metadata,
            vocab,
            blank_id,
            pcec_config,
        })
    }
}

impl AsrEngine for Asr4AllEngine {
    fn create_session(&self) -> Result<Box<dyn AsrSession>, Box<dyn std::error::Error>> {
        let stream_path = Path::new(&self.model_dir).join("stream_c16r4.onnx");
        let pcec_path = Path::new(&self.model_dir).join("pcec_c16.onnx");
        let flush_path = Path::new(&self.model_dir).join("pcec_c16_flush.onnx");

        let stream_sess = Session::builder()?.commit_from_file(stream_path)?;
        let pcec_sess = Session::builder()?.commit_from_file(pcec_path)?;
        let flush_sess = Session::builder()?.commit_from_file(flush_path)?;

        let pcec_state = PcecState::new(self.pcec_config.clone());

        let mut ctrl_map = HashMap::new();
        ctrl_map.insert("cap".to_string(), self.pcec_config.cap_id);
        ctrl_map.insert("allcaps".to_string(), self.pcec_config.allcaps_id);
        ctrl_map.insert("period".to_string(), self.pcec_config.period_id);
        ctrl_map.insert("comma".to_string(), self.pcec_config.comma_id);
        ctrl_map.insert("question".to_string(), self.pcec_config.question_id);

        Ok(Box::new(Asr4AllSession {
            stream_sess,
            pcec_sess,
            flush_sess,
            vocab: self.vocab.clone(),
            blank_id: self.blank_id,
            pcec_config: self.pcec_config.clone(),
            ctrl_map,
            pcec_state,
            filter_cache: vec![0.0f32; 1006080],
            attn_cache: vec![0.0f32; 329090],
            pcm_buffer: Vec::new(),
            next_frame: 0,
            ids: Vec::new(),
            spf: 640,
            window_samples: 22400,
            commit_samples: 10240, // 16 frames * 640
            commit_frames: 16,
        }))
    }

    fn name(&self) -> &str {
        "FUTO asr4all-s (streaming c16r4)"
    }
}

#[allow(dead_code)]
pub struct Asr4AllSession {
    stream_sess: Session,
    pcec_sess: Session,
    flush_sess: Session,
    vocab: Vec<String>,
    blank_id: usize,
    pcec_config: PcecConfig,
    ctrl_map: HashMap<String, usize>,
    pcec_state: PcecState,
    filter_cache: Vec<f32>,
    attn_cache: Vec<f32>,
    pcm_buffer: Vec<f32>,
    next_frame: usize,
    ids: Vec<i64>,
    spf: usize,
    window_samples: usize,
    commit_samples: usize,
    commit_frames: usize,
}

impl Asr4AllSession {
    fn step(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let start = self.next_frame * self.spf;
        let mut win = vec![0.0f32; self.window_samples];
        let hi = (start + self.window_samples).min(self.pcm_buffer.len());
        if hi > start {
            win[..hi - start].copy_from_slice(&self.pcm_buffer[start..hi]);
        }

        let pcm_array = Array2::from_shape_vec((1, self.window_samples), win)?;
        let f_array = Array1::from_shape_vec(self.filter_cache.len(), self.filter_cache.clone())?;
        let a_array = Array1::from_shape_vec(self.attn_cache.len(), self.attn_cache.clone())?;

        let outputs = self.stream_sess.run(ort::inputs![
            "stream_c16r4_pcm" => Tensor::from_array(pcm_array)?,
            "filter_cache" => Tensor::from_array(f_array)?,
            "attn_cache" => Tensor::from_array(a_array)?,
        ])?;

        // 1. Update filter & attn caches
        let (_, new_f_cache) = outputs["stream_c16r4_filter_cache_out"].try_extract_tensor::<f32>()?;
        self.filter_cache.copy_from_slice(new_f_cache);

        let (_, new_a_cache) = outputs["stream_c16r4_attn_cache_out"].try_extract_tensor::<f32>()?;
        self.attn_cache.copy_from_slice(new_a_cache);

        // 2. Extract log probs
        let (lp_shape, lp_data) = outputs["stream_c16r4_log_probs"].try_extract_tensor::<f32>()?;
        // lp_shape is [1, 16, 257]
        let _frames_count = lp_shape[1] as usize;
        let vocab_dim = lp_shape[2] as usize;

        let valid_frames = self.commit_frames.min((self.pcm_buffer.len() / self.spf).saturating_sub(self.next_frame));
        let mut frame_ids = Vec::with_capacity(valid_frames);

        for t in 0..valid_frames {
            let offset = t * vocab_dim;
            let slice = &lp_data[offset..offset + vocab_dim];
            let mut best_id = 0;
            let mut best_val = f32::NEG_INFINITY;
            for (idx, &v) in slice.iter().enumerate() {
                if v > best_val {
                    best_val = v;
                    best_id = idx;
                }
            }
            frame_ids.push(best_id as i64);
        }

        // 3. Extract hidden states for PCEC
        let (_, hidden_data) = outputs["stream_c16r4_hidden"].try_extract_tensor::<f32>()?;
        let hidden_vec = hidden_data.to_vec();

        // Drop session outputs to release borrow on self.stream_sess
        drop(outputs);

        // 4. Commit frame ids & pool to PCEC
        self.ids.extend_from_slice(&frame_ids);
        if !frame_ids.is_empty() {
            self.pcec_state.pool(&frame_ids, &hidden_vec, self.pcec_config.hidden_dim, self.blank_id as i64);
            self.drain_pcec()?;
        }

        self.next_frame += self.commit_frames;
        Ok(())
    }

    fn drain_pcec(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let k = self.pcec_config.commit;
        while self.pcec_state.ids.len() >= k {
            let (hyp_ids, pause, acoustic, keep) = self.build_pcec_feats(k, k);
            let state_arr = Array2::from_shape_vec((1, self.pcec_state.state_cache.len()), self.pcec_state.state_cache.clone())?;
            let seen_arr = Array1::from_shape_vec(1, vec![self.pcec_state.seen_cache])?;

            let outs = self.pcec_sess.run(ort::inputs![
                "pcec_c16_hyp_ids" => Tensor::from_array(hyp_ids)?,
                "pcec_c16_pause" => Tensor::from_array(pause)?,
                "pcec_c16_acoustic" => Tensor::from_array(acoustic)?,
                "pcec_c16_keep" => Tensor::from_array(keep)?,
                "pcec_c16_state" => Tensor::from_array(state_arr)?,
                "pcec_c16_seen" => Tensor::from_array(seen_arr)?,
            ])?;

            let (_, slots_out) = outs["pcec_c16_slots"].try_extract_tensor::<i64>()?;
            let slots_vec = slots_out.to_vec();
            let (_, state_out) = outs["pcec_c16_state_out"].try_extract_tensor::<f32>()?;
            let state_vec = state_out.to_vec();
            let (_, seen_out) = outs["pcec_c16_seen_out"].try_extract_tensor::<i64>()?;
            let seen_val = seen_out[0];
            drop(outs);

            self.pcec_state.state_cache.copy_from_slice(&state_vec);
            self.pcec_state.seen_cache = seen_val;

            let lookahead = self.pcec_config.lookahead;
            let skip = (2 * k).min((2 * lookahead).saturating_sub(2 * self.pcec_state.consumed));
            let end_slot = (2 * k).min(slots_vec.len());
            if skip < end_slot {
                self.pcec_state.slots.extend_from_slice(&slots_vec[skip..end_slot]);
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
        let mut ids = vec![self.pcec_config.pad_in; k];
        let mut pause = vec![self.pcec_config.pause_blackout; k];
        let mut acoustic = vec![0.0f32; k * self.pcec_config.hidden_dim];
        let mut keep = vec![0.0f32; k];

        for i in 0..n {
            ids[i] = self.pcec_state.ids[i];
            if self.pcec_config.real_pause {
                pause[i] = self.pcec_state.pause[i];
            }
            let src_ac = &self.pcec_state.acoustic[i];
            acoustic[i * self.pcec_config.hidden_dim..(i + 1) * self.pcec_config.hidden_dim].copy_from_slice(src_ac);
            keep[i] = self.pcec_state.keep[i];
        }

        let hyp_ids = Array2::from_shape_vec((1, k), ids).unwrap();
        let pause_arr = Array2::from_shape_vec((1, k), pause).unwrap();
        let ac_arr = Array3::from_shape_vec((1, k, self.pcec_config.hidden_dim), acoustic).unwrap();
        let keep_arr = Array2::from_shape_vec((1, k), keep).unwrap();

        (hyp_ids, pause_arr, ac_arr, keep_arr)
    }

    fn raw_ctc_text(&self) -> String {
        let mut out = Vec::new();
        let mut prev = -1i64;
        for &id in &self.ids {
            if id != prev && id != self.blank_id as i64 && (id as usize) < self.vocab.len() {
                out.push(self.vocab[id as usize].as_str());
            }
            prev = id;
        }
        out.concat().split_whitespace().collect::<Vec<&str>>().join(" ")
    }

    fn pcec_text(&self) -> String {
        render_slots(
            &self.pcec_config.vocab,
            self.pcec_config.blank_id,
            &self.ctrl_map,
            &self.pcec_state.slots,
        )
    }
}

impl AsrSession for Asr4AllSession {
    fn accept_audio(&mut self, samples: &[f32]) -> Result<AsrPartial, Box<dyn std::error::Error>> {
        self.pcm_buffer.extend_from_slice(samples);
        let window_frames = self.window_samples / self.spf;
        while (self.next_frame + window_frames) * self.spf <= self.pcm_buffer.len() {
            self.step()?;
        }

        let raw = self.raw_ctc_text();
        let formatted = self.pcec_text();
        let final_text = if formatted.is_empty() { raw.clone() } else { formatted };

        Ok(AsrPartial {
            raw_ctc: raw,
            formatted: final_text,
            is_final: false,
        })
    }

    fn finish(&mut self) -> Result<AsrPartial, Box<dyn std::error::Error>> {
        while self.next_frame * self.spf < self.pcm_buffer.len() {
            self.step()?;
        }
        self.pcec_state.flush_open();
        self.drain_pcec()?;

        // Run flush session for final tail slots
        let n_real = self.pcec_state.ids.len();
        let k = self.pcec_config.commit;
        let (hyp_ids, pause, acoustic, keep) = self.build_pcec_feats(k, n_real);
        let state_arr = Array2::from_shape_vec((1, self.pcec_state.state_cache.len()), self.pcec_state.state_cache.clone())?;
        let seen_arr = Array1::from_shape_vec(1, vec![self.pcec_state.seen_cache])?;

        let n_real_scalar = Array1::from_shape_vec(1, vec![n_real as i64])?;

        let flush_outs = self.flush_sess.run(ort::inputs![
            "pcec_c16_hyp_ids" => Tensor::from_array(hyp_ids)?,
            "pcec_c16_pause" => Tensor::from_array(pause)?,
            "pcec_c16_acoustic" => Tensor::from_array(acoustic)?,
            "pcec_c16_keep" => Tensor::from_array(keep)?,
            "pcec_c16_n_real" => Tensor::from_array(n_real_scalar)?,
            "pcec_c16_state" => Tensor::from_array(state_arr)?,
            "pcec_c16_seen" => Tensor::from_array(seen_arr)?,
        ])?;

        let (_, tail_slots) = flush_outs["pcec_c16_tail_slots"].try_extract_tensor::<i64>()?;
        let tail_slots_vec = tail_slots.to_vec();
        drop(flush_outs);

        let lookahead = self.pcec_config.lookahead;
        let skip = (2 * lookahead).saturating_sub(2 * self.pcec_state.consumed);
        let valid = 2 * lookahead + 2 * n_real + 1;
        let valid_clamped = valid.min(tail_slots_vec.len());

        let mut all_slots = self.pcec_state.slots.clone();
        if skip < valid_clamped {
            all_slots.extend_from_slice(&tail_slots_vec[skip..valid_clamped]);
        }

        let raw = self.raw_ctc_text();
        let mut formatted = render_slots(
            &self.pcec_config.vocab,
            self.pcec_config.blank_id,
            &self.ctrl_map,
            &all_slots,
        );

        if formatted.is_empty() {
            formatted = raw.clone();
        }

        Ok(AsrPartial {
            raw_ctc: raw,
            formatted,
            is_final: true,
        })
    }

    fn reset(&mut self) {
        self.filter_cache.fill(0.0);
        self.attn_cache.fill(0.0);
        self.pcm_buffer.clear();
        self.next_frame = 0;
        self.ids.clear();
        self.pcec_state.reset();
    }
}
