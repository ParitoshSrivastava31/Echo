use std::path::Path;
use std::time::Instant;
use crate::asr::{Asr4AllEngine, AsrEngine};
use crate::controller::{DictationController, ControllerOutput};
use crate::vocabulary::PersonalDictionary;

#[derive(Debug)]
#[allow(dead_code)]
pub struct BenchmarkResult {
    pub audio_duration_sec: f64,
    pub inference_duration_ms: f64,
    pub rtfx: f64,
    pub raw_transcript: String,
    pub formatted_transcript: String,
    pub reference: String,
    pub wer: f64,
    pub cer: f64,
}

pub struct BenchmarkRunner;

impl BenchmarkRunner {
    pub fn run_wav<P: AsRef<Path>>(
        wav_path: P,
        reference_text: &str,
    ) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
        let _ = crate::asr::init_runtime(2);

        let model_dir = crate::paths::find_resource("models/asr4all-s")
            .unwrap_or_else(|| std::path::PathBuf::from("models/asr4all-s"));
        let engine = Asr4AllEngine::new(&model_dir)?;
        let mut session = engine.create_session()?;

        let mut reader = hound::WavReader::open(wav_path)?;
        let spec = reader.spec();
        let samples: Vec<f32> = reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect();

        let duration_sec = samples.len() as f64 / spec.sample_rate as f64;

        let start_time = Instant::now();

        // Feed in 0.5s chunks
        let chunk_size = 8000;
        let dict = PersonalDictionary::default();
        let mut controller = DictationController::new(dict, true);

        for chunk in samples.chunks(chunk_size) {
            let _ = session.accept_audio(chunk)?;
        }

        let asr_final = session.finish()?;
        let elapsed = start_time.elapsed();

        let ctx = crate::controller::InsertContext::default();
        let final_out = controller.process_transcript(&asr_final.formatted, &ctx);
        let final_text = match final_out {
            ControllerOutput::InsertText(t) => t.trim().to_string(),
            ControllerOutput::ReplaceText { text, .. } => text.trim().to_string(),
            _ => asr_final.formatted.trim().to_string(),
        };

        let wer = calculate_wer(&final_text, reference_text);
        let cer = calculate_cer(&final_text, reference_text);

        Ok(BenchmarkResult {
            audio_duration_sec: duration_sec,
            inference_duration_ms: elapsed.as_secs_f64() * 1000.0,
            rtfx: duration_sec / elapsed.as_secs_f64(),
            raw_transcript: asr_final.raw_ctc,
            formatted_transcript: final_text,
            reference: reference_text.to_string(),
            wer,
            cer,
        })
    }
}

pub fn calculate_wer(hyp: &str, ref_text: &str) -> f64 {
    let clean_hyp = normalize_text(hyp);
    let clean_ref = normalize_text(ref_text);

    let hyp_words: Vec<&str> = clean_hyp.split_whitespace().collect();
    let ref_words: Vec<&str> = clean_ref.split_whitespace().collect();

    if ref_words.is_empty() {
        return if hyp_words.is_empty() { 0.0 } else { 1.0 };
    }

    let dist = levenshtein(&hyp_words, &ref_words);
    dist as f64 / ref_words.len() as f64
}

pub fn calculate_cer(hyp: &str, ref_text: &str) -> f64 {
    let clean_hyp = normalize_text(hyp);
    let clean_ref = normalize_text(ref_text);

    let hyp_chars: Vec<char> = clean_hyp.chars().filter(|c| !c.is_whitespace()).collect();
    let ref_chars: Vec<char> = clean_ref.chars().filter(|c| !c.is_whitespace()).collect();

    if ref_chars.is_empty() {
        return if hyp_chars.is_empty() { 0.0 } else { 1.0 };
    }

    let dist = levenshtein(&hyp_chars, &ref_chars);
    dist as f64 / ref_chars.len() as f64
}

fn normalize_text(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect()
}

fn levenshtein<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut d = vec![vec![0; b.len() + 1]; a.len() + 1];

    for i in 0..=a.len() {
        d[i][0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
        }
    }

    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wer() {
        let hyp = "And so my fellow Americans ask not what your country can do for you";
        let ref_text = "And so my fellow Americans ask not what your country can do for you";
        assert_eq!(calculate_wer(hyp, ref_text), 0.0);

        let hyp2 = "And so my fellow friends";
        let ref2 = "And so my fellow Americans";
        // 1 error out of 5 words = 0.20
        assert!((calculate_wer(hyp2, ref2) - 0.20).abs() < 1e-4);
    }
}
