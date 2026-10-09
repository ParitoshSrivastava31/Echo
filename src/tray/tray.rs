use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum DictationState {
    Idle,
    Listening,
    Processing,
    Error,
}

pub struct TrayManager {
    pub state: DictationState,
    pub is_running: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn new() -> Self {
        Self {
            state: DictationState::Idle,
            is_running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn set_state(&mut self, state: DictationState) {
        self.state = state;
        let state_name = match state {
            DictationState::Idle => "Idle (Waiting for shortcut)",
            DictationState::Listening => "Listening... (Speak naturally)",
            DictationState::Processing => "Processing speech...",
            DictationState::Error => "Error",
        };
        println!("[Echo Status] -> {}", state_name);
    }
}
