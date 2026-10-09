/// Audio resampler from arbitrary input sample rate to 16,000 Hz mono.
pub struct LinearResampler {
    pub from_rate: u32,
    pub to_rate: u32,
    phase: f64,
}

impl LinearResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            from_rate,
            to_rate,
            phase: 0.0,
        }
    }

    /// Resample multi-channel or mono audio slice into 16,000 Hz mono f32.
    pub fn process_interleaved(&mut self, input: &[f32], channels: u16) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }

        // 1. Downmix to mono if multi-channel
        let mono: Vec<f32> = if channels <= 1 {
            input.to_vec()
        } else {
            let ch = channels as usize;
            input
                .chunks_exact(ch)
                .map(|frame| frame.iter().sum::<f32>() / ch as f32)
                .collect()
        };

        if self.from_rate == self.to_rate {
            return mono;
        }

        let ratio = self.from_rate as f64 / self.to_rate as f64;
        let mut output = Vec::with_capacity((mono.len() as f64 / ratio) as usize + 8);

        while (self.phase as usize) + 1 < mono.len() {
            let idx = self.phase as usize;
            let frac = (self.phase - idx as f64) as f32;
            let s0 = mono[idx];
            let s1 = mono[idx + 1];
            let interpolated = s0 + frac * (s1 - s0);
            output.push(interpolated);
            self.phase += ratio;
        }

        self.phase -= (mono.len().saturating_sub(1)) as f64;
        if self.phase < 0.0 {
            self.phase = 0.0;
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resampling_48k_to_16k() {
        let mut resampler = LinearResampler::new(48000, 16000);
        let input = vec![1.0f32; 4800]; // 0.1s
        let out = resampler.process_interleaved(&input, 1);
        // 4800 / 3 = 1600 samples
        assert!((out.len() as i32 - 1600).abs() <= 2);
    }
}
