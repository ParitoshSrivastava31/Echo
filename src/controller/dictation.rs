use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::commands::{Action, CommandParser};
use crate::vocabulary::PersonalDictionary;

/// Where the text is going. `key_counter` is the number of physical key presses seen so far;
/// if it changed since our last insertion the user has typed or moved, so we must not
/// assume the cursor is still right after our text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InsertContext {
    pub target: isize,
    pub key_counter: u64,
}

impl InsertContext {
    pub fn current() -> Self {
        let hwnd = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
        Self {
            target: hwnd as isize,
            key_counter: crate::hotkey::KEY_COUNTER.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ControllerOutput {
    InsertText(String),
    /// Delete `backspaces` characters, then type `text`.
    ReplaceText { backspaces: usize, text: String },
    NoOp,
}

struct Entry {
    text: String,
    ctx: InsertContext,
    at: Instant,
}

const HISTORY: usize = 20;
const CONTINUE_WINDOW: Duration = Duration::from_secs(300);

pub struct DictationController {
    pub dictionary: PersonalDictionary,
    pub suppress_fillers: bool,
    history: VecDeque<Entry>,
}

impl DictationController {
    pub fn new(dictionary: PersonalDictionary, suppress_fillers: bool) -> Self {
        Self { dictionary, suppress_fillers, history: VecDeque::new() }
    }

    /// The last insertion, if the cursor is provably still right after it.
    fn continuing(&self, ctx: &InsertContext) -> Option<&Entry> {
        self.history
            .back()
            .filter(|e| e.ctx == *ctx && e.at.elapsed() < CONTINUE_WINDOW)
    }

    pub fn process_transcript(&mut self, raw_text: &str, ctx: &InsertContext) -> ControllerOutput {
        if raw_text.trim().is_empty() {
            return ControllerOutput::NoOp;
        }
        match CommandParser::parse_with(raw_text, self.suppress_fillers) {
            Action::UndoLast => match self.undo(ctx) {
                Some(n) => ControllerOutput::ReplaceText { backspaces: n, text: String::new() },
                None => ControllerOutput::NoOp,
            },
            Action::UndoAndInsert(text) => {
                let backspaces = self.undo(ctx).unwrap_or(0);
                match self.insert(&text, ctx) {
                    Some(text) => ControllerOutput::ReplaceText { backspaces, text },
                    None if backspaces > 0 => ControllerOutput::ReplaceText { backspaces, text: String::new() },
                    None => ControllerOutput::NoOp,
                }
            }
            Action::Insert(text) => match self.insert(&text, ctx) {
                Some(t) => ControllerOutput::InsertText(t),
                None => ControllerOutput::NoOp,
            },
        }
    }

    fn undo(&mut self, ctx: &InsertContext) -> Option<usize> {
        self.continuing(ctx)?;
        self.history.pop_back().map(|e| e.text.chars().count())
    }

    fn insert(&mut self, text: &str, ctx: &InsertContext) -> Option<String> {
        let text = self.dictionary.apply(text);
        if text.trim().is_empty() {
            return None;
        }
        let needs_space = self.continuing(ctx).is_some_and(|e| !e.text.ends_with(['\n', ' ']))
            && !text.starts_with(|c: char| c.is_whitespace() || ".,?!:;)".contains(c));
        let out = if needs_space { format!(" {text}") } else { text };
        self.history.push_back(Entry { text: out.clone(), ctx: *ctx, at: Instant::now() });
        if self.history.len() > HISTORY {
            self.history.pop_front();
        }
        Some(out)
    }

    /// Call when an insertion could not be typed (e.g. copied to clipboard instead).
    pub fn forget_last(&mut self) {
        self.history.pop_back();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTX: InsertContext = InsertContext { target: 1, key_counter: 0 };

    #[test]
    fn test_dictation_flow_with_vocab_and_history() {
        let mut c = DictationController::new(PersonalDictionary::default(), true);
        assert_eq!(
            c.process_transcript("I write typescript code", &CTX),
            ControllerOutput::InsertText("I write TypeScript code".into())
        );
        assert_eq!(
            c.process_transcript("with postgres", &CTX),
            ControllerOutput::InsertText(" with PostgreSQL".into())
        );
        assert_eq!(
            c.process_transcript("scratch that", &CTX),
            ControllerOutput::ReplaceText { backspaces: " with PostgreSQL".chars().count(), text: String::new() }
        );
    }

    #[test]
    fn test_no_space_or_undo_after_user_typed() {
        let mut c = DictationController::new(PersonalDictionary::default(), true);
        c.process_transcript("Hello there.", &CTX);
        let moved = InsertContext { key_counter: 3, ..CTX };
        assert_eq!(c.process_transcript("Next message.", &moved), ControllerOutput::InsertText("Next message.".into()));
        let moved_again = InsertContext { key_counter: 9, ..CTX };
        assert_eq!(c.process_transcript("scratch that", &moved_again), ControllerOutput::NoOp);
    }

    #[test]
    fn test_other_window_gets_no_leading_space() {
        let mut c = DictationController::new(PersonalDictionary::default(), true);
        c.process_transcript("First.", &CTX);
        let other = InsertContext { target: 2, ..CTX };
        assert_eq!(c.process_transcript("Second.", &other), ControllerOutput::InsertText("Second.".into()));
    }

    #[test]
    fn test_scratch_that_replace() {
        let mut c = DictationController::new(PersonalDictionary::default(), true);
        c.process_transcript("Send it Monday.", &CTX);
        assert_eq!(
            c.process_transcript("Scratch that, send it Friday.", &CTX),
            ControllerOutput::ReplaceText { backspaces: 15, text: "Send it Friday.".into() }
        );
    }

    #[test]
    fn test_filler_only_is_noop() {
        let mut c = DictationController::new(PersonalDictionary::default(), true);
        assert_eq!(c.process_transcript("Um.", &CTX), ControllerOutput::NoOp);
    }
}
