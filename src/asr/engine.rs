pub trait AsrEngine: Send + Sync {
    fn create_session(&self) -> Result<Box<dyn AsrSession>, Box<dyn std::error::Error>>;
    fn name(&self) -> &str;
}

pub struct AsrPartial {
    /// Raw CTC transcript (lowercase, no punctuation).
    pub raw_ctc: String,
    /// Punctuated and capitalised transcript.
    pub formatted: String,
}

pub trait AsrSession: Send {
    /// Feed 16 kHz mono audio. Inference runs incrementally as soon as enough audio has arrived,
    /// so `finish` only has to process the tail.
    fn accept_audio(&mut self, samples: &[f32]) -> Result<(), Box<dyn std::error::Error>>;

    /// Current provisional transcript (cheap; does not run inference).
    fn partial_text(&self) -> String;

    /// Flush buffered audio and return the final transcript.
    fn finish(&mut self) -> Result<AsrPartial, Box<dyn std::error::Error>>;

    /// Reset state for a new utterance.
    fn reset(&mut self);
}
