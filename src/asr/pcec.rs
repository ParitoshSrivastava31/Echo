use std::collections::HashMap;

pub const PAUSE_THRESHOLDS: &[i64] = &[1, 2, 4, 8, 16, 32, 64, 128];

pub fn pause_bucket(blanks: i64) -> i64 {
    PAUSE_THRESHOLDS.iter().filter(|&&t| blanks >= t).count() as i64
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PcecConfig {
    pub commit: usize,
    pub lookahead: usize,
    pub pad_in: i64,
    pub pause_blackout: i64,
    pub real_pause: bool,
    pub hidden_dim: usize,
    pub vocab: Vec<String>,
    pub blank_id: usize,
    pub cap_id: usize,
    pub allcaps_id: usize,
    pub period_id: usize,
    pub comma_id: usize,
    pub question_id: usize,
}

#[derive(Debug, Clone)]
pub struct OpenToken {
    pub id: i64,
    pub hidden_sum: Vec<f32>,
    pub frames: usize,
    pub leading_blanks: i64,
    pub keep: f32,
}

#[allow(dead_code)]
pub struct PcecState {
    pub config: PcecConfig,
    pub ids: Vec<i64>,
    pub pause: Vec<i64>,
    pub acoustic: Vec<Vec<f32>>,
    pub keep: Vec<f32>,
    pub slots: Vec<i64>,
    pub consumed: usize,
    pub open_token: Option<OpenToken>,
    pub blanks: i64,
    pub prev_frame_id: i64,
    pub state_cache: Vec<f32>,
    pub seen_cache: i64,
}

impl PcecState {
    pub fn new(config: PcecConfig, state_size: usize) -> Self {
        Self {
            config,
            ids: Vec::new(),
            pause: Vec::new(),
            acoustic: Vec::new(),
            keep: Vec::new(),
            slots: Vec::new(),
            consumed: 0,
            open_token: None,
            blanks: 0,
            prev_frame_id: -1,
            state_cache: vec![0.0f32; state_size],
            seen_cache: 0,
        }
    }

    pub fn reset(&mut self) {
        self.ids.clear();
        self.pause.clear();
        self.acoustic.clear();
        self.keep.clear();
        self.slots.clear();
        self.consumed = 0;
        self.open_token = None;
        self.blanks = 0;
        self.prev_frame_id = -1;
        self.state_cache.fill(0.0);
        self.seen_cache = 0;
    }

    pub fn pool(&mut self, frame_ids: &[i64], hidden: &[f32], hidden_dim: usize, blank: i64) {
        for (t, &fid) in frame_ids.iter().enumerate() {
            let frame_hidden = &hidden[t * hidden_dim..(t + 1) * hidden_dim];
            if fid != self.prev_frame_id && fid != blank {
                self.emit();
                self.open_token = Some(OpenToken {
                    id: fid,
                    hidden_sum: frame_hidden.to_vec(),
                    frames: 1,
                    leading_blanks: self.blanks,
                    keep: 0.0,
                });
                self.blanks = 0;
            } else if let Some(ref mut open) = self.open_token {
                for (sum, val) in open.hidden_sum.iter_mut().zip(frame_hidden.iter()) {
                    *sum += *val;
                }
                open.frames += 1;
            }
            self.prev_frame_id = fid;
            if fid == blank {
                self.blanks += 1;
            }
        }
    }

    pub fn flush_open(&mut self) {
        self.emit();
    }

    fn emit(&mut self) {
        if let Some(open) = self.open_token.take() {
            self.ids.push(open.id);
            let n = open.frames as f32;
            let avg_hidden: Vec<f32> = open.hidden_sum.iter().map(|v| v / n).collect();
            self.acoustic.push(avg_hidden);
            self.pause.push(pause_bucket(open.leading_blanks));
            self.keep.push(open.keep);
        }
    }
}

pub fn render_slots(
    vocab: &[String],
    blank_id: usize,
    ctrl: &HashMap<String, usize>,
    slots: &[i64],
) -> String {
    let cap_id = ctrl.get("cap").copied().unwrap_or(usize::MAX) as i64;
    let allcaps_id = ctrl.get("allcaps").copied().unwrap_or(usize::MAX) as i64;
    let period_id = ctrl.get("period").copied().unwrap_or(usize::MAX) as i64;
    let comma_id = ctrl.get("comma").copied().unwrap_or(usize::MAX) as i64;
    let question_id = ctrl.get("question").copied().unwrap_or(usize::MAX) as i64;

    struct WordInfo {
        pieces: Vec<String>,
        case: String,
        punct: String,
    }

    let mut words: Vec<WordInfo> = Vec::new();
    let mut pending_case = String::new();
    let mut prev: i64 = -1;

    for &slot in slots {
        if slot == prev || slot == blank_id as i64 {
            prev = slot;
            continue;
        }
        prev = slot;

        if slot == cap_id {
            pending_case = "cap".to_string();
        } else if slot == allcaps_id {
            pending_case = "upper".to_string();
        } else if slot == period_id {
            if let Some(last) = words.last_mut() {
                last.punct = ".".to_string();
            }
        } else if slot == comma_id {
            if let Some(last) = words.last_mut() {
                last.punct = ",".to_string();
            }
        } else if slot == question_id {
            if let Some(last) = words.last_mut() {
                last.punct = "?".to_string();
            }
        } else if (slot as usize) < vocab.len() {
            let piece = &vocab[slot as usize];
            if !piece.is_empty() {
                if piece.starts_with(' ') || words.is_empty() {
                    words.push(WordInfo {
                        pieces: vec![piece.clone()],
                        case: pending_case.clone(),
                        punct: String::new(),
                    });
                    pending_case.clear();
                } else if let Some(last) = words.last_mut() {
                    last.pieces.push(piece.clone());
                }
            }
        }
    }

    let mut out: Vec<String> = Vec::new();
    let mut sentence_start = true;

    for word in words {
        let joined = word.pieces.concat();
        let w = joined.trim();
        if w.is_empty() {
            continue;
        }
        let formatted = if word.case == "upper" {
            w.to_uppercase()
        } else if word.case == "cap" || sentence_start {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
            }
        } else {
            w.to_string()
        };

        sentence_start = word.punct == "." || word.punct == "?";
        out.push(format!("{}{}", formatted, word.punct));
    }

    out.join(" ")
}
