use std::path::Path;
use ndarray::Array2;
use ort::session::Session;
use ort::value::Tensor;

#[allow(dead_code)]
pub struct KiloVad {
    session: Option<Session>,
    threshold: f32,
}

#[allow(dead_code)]
impl KiloVad {
    pub fn new<P: AsRef<Path>>(vad_model_path: P, threshold: f32) -> Self {
        let session = if vad_model_path.as_ref().exists() {
            Session::builder()
                .and_then(|mut b| b.commit_from_file(vad_model_path))
                .ok()
        } else {
            None
        };

        Self { session, threshold }
    }

    /// Check if speech is present in a 1-second (16,000 samples) window of 16kHz mono audio.
    /// If session is not available, uses an adaptive RMS energy detector.
    pub fn is_speech(&mut self, samples: &[f32]) -> bool {
        if let Some(ref mut session) = self.session {
            let mut input = vec![0.0f32; 16000];
            let len = samples.len().min(16000);
            input[..len].copy_from_slice(&samples[..len]);

            if let Ok(input_arr) = Array2::from_shape_vec((1, 16000), input) {
                if let Ok(tensor) = Tensor::from_array(input_arr) {
                    if let Ok(outputs) = session.run(ort::inputs!["vad_pcm" => tensor]) {
                        if let Ok((_, probs)) = outputs["vad_speech_prob"].try_extract_tensor::<f32>() {
                            let max_prob = probs.iter().fold(0.0f32, |m, &p| m.max(p));
                            return max_prob >= self.threshold;
                        }
                    }
                }
            }
        }

        // Energy fallback: RMS > 0.015 (~ -36 dBFS)
        if samples.is_empty() {
            return false;
        }
        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        let rms = (sum_sq / samples.len() as f32).sqrt();
        rms > 0.012
    }
}
