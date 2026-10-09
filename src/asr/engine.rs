#[allow(dead_code)]
pub trait AsrEngine: Send + Sync {
    fn create_session(&self) -> Result<Box<dyn AsrSession>, Box<dyn std::error::Error>>;
    fn name(&self) -> &str;
}

pub struct AsrPartial {
    pub raw_ctc: String,
    pub formatted: String,
    #[allow(dead_code)]
    pub is_final: bool,
}

pub trait AsrSession: Send {
    /// Feed an incremental slice of 16kHz mono float audio.
    /// Returns current partial transcript (provisional tail + stable committed text).
    fn accept_audio(&mut self, samples: &[f32]) -> Result<AsrPartial, Box<dyn std::error::Error>>;

    /// Flush the remaining buffered audio and settle final tokens.
    fn finish(&mut self) -> Result<AsrPartial, Box<dyn std::error::Error>>;

    /// Reset internal state for a new utterance.
    fn reset(&mut self);
}
