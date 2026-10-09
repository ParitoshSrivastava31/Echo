use std::time::Duration;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct SessionMetrics {
    pub utterances_count: u64,
    pub total_audio_duration_sec: f64,
    pub total_inference_time: Duration,
    pub last_first_partial_latency: Option<Duration>,
    pub last_finalization_latency: Option<Duration>,
    pub last_rtfx: f64,
}

#[allow(dead_code)]
impl SessionMetrics {
    pub fn record_utterance(
        &mut self,
        audio_duration_sec: f64,
        inference_time: Duration,
        first_partial_latency: Option<Duration>,
        finalization_latency: Option<Duration>,
    ) {
        self.utterances_count += 1;
        self.total_audio_duration_sec += audio_duration_sec;
        self.total_inference_time += inference_time;
        self.last_first_partial_latency = first_partial_latency;
        self.last_finalization_latency = finalization_latency;
        self.last_rtfx = if inference_time.as_secs_f64() > 0.0 {
            audio_duration_sec / inference_time.as_secs_f64()
        } else {
            0.0
        };
    }

    pub fn average_rtfx(&self) -> f64 {
        if self.total_inference_time.as_secs_f64() > 0.0 {
            self.total_audio_duration_sec / self.total_inference_time.as_secs_f64()
        } else {
            0.0
        }
    }
}
