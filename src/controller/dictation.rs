use crate::vocabulary::PersonalDictionary;
use super::commands::{Action, CommandParser};

pub struct DictationController {
    dictionary: PersonalDictionary,
    suppress_fillers: bool,
    committed_text: String,
    history_stack: Vec<HistoryEntry>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct HistoryEntry {
    pub inserted: String,
    pub backspaces_needed: usize,
}

#[derive(Debug, PartialEq)]
pub enum ControllerOutput {
    /// Text to type/inject at cursor
    InsertText(String),
    /// Delete count characters back, then optionally insert new text
    ReplaceText { backspaces: usize, text: String },
    /// No change needed
    NoOp,
}

impl DictationController {
    pub fn new(dictionary: PersonalDictionary, suppress_fillers: bool) -> Self {
        Self {
            dictionary,
            suppress_fillers,
            committed_text: String::new(),
            history_stack: Vec::new(),
        }
    }

    /// Process a new speech hypothesis.
    /// If `is_final` is true, commits the segment to permanent history.
    pub fn process_transcript(&mut self, raw_text: &str, is_final: bool) -> ControllerOutput {
        if raw_text.trim().is_empty() {
            return ControllerOutput::NoOp;
        }

        // 1. Filter filler words if enabled
        let cleaned = if self.suppress_fillers {
            suppress_fillers(raw_text)
        } else {
            raw_text.to_string()
        };

        // 2. Parse commands
        let action = CommandParser::parse(&cleaned);

        match action {
            Action::UndoLast => {
                if let Some(last) = self.history_stack.pop() {
                    let backspaces = last.inserted.chars().count();
                    return ControllerOutput::ReplaceText {
                        backspaces,
                        text: String::new(),
                    };
                }
                ControllerOutput::NoOp
            }
            Action::DeleteSentence => {
                // Find last sentence in history
                if let Some(last) = self.history_stack.pop() {
                    let backspaces = last.inserted.chars().count();
                    return ControllerOutput::ReplaceText {
                        backspaces,
                        text: String::new(),
                    };
                }
                ControllerOutput::NoOp
            }
            Action::Insert(text) => {
                // 3. Apply personal vocabulary
                let enriched = self.dictionary.apply(&text);

                if is_final {
                    // Compute text to insert: prepend a space if needed
                    let to_insert = if !self.committed_text.is_empty()
                        && !self.committed_text.ends_with('\n')
                        && !self.committed_text.ends_with(' ')
                        && !enriched.starts_with('\n')
                        && !enriched.starts_with('.')
                        && !enriched.starts_with(',')
                        && !enriched.starts_with('?')
                        && !enriched.starts_with('!')
                    {
                        format!(" {}", enriched)
                    } else {
                        enriched
                    };

                    self.committed_text.push_str(&to_insert);
                    self.history_stack.push(HistoryEntry {
                        inserted: to_insert.clone(),
                        backspaces_needed: to_insert.chars().count(),
                    });

                    ControllerOutput::InsertText(to_insert)
                } else {
                    // During streaming partial updates, conservative preview
                    ControllerOutput::NoOp
                }
            }
        }
    }

    pub fn reset_session(&mut self) {
        self.committed_text.clear();
        self.history_stack.clear();
    }
}

fn suppress_fillers(text: &str) -> String {
    let fillers = [" um ", " uh ", " er ", " ah "];
    let mut s = format!(" {} ", text);
    for &f in &fillers {
        while let Some(pos) = s.to_lowercase().find(f) {
            s.replace_range(pos..pos + f.len() - 1, " ");
        }
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dictation_flow_with_vocab_and_history() {
        let dict = PersonalDictionary::default();
        let mut controller = DictationController::new(dict, true);

        let out1 = controller.process_transcript("I write typescript code", true);
        assert_eq!(
            out1,
            ControllerOutput::InsertText("I write TypeScript code".to_string())
        );

        let out2 = controller.process_transcript("with postgres", true);
        assert_eq!(
            out2,
            ControllerOutput::InsertText(" with PostgreSQL".to_string())
        );

        // Undo
        let out3 = controller.process_transcript("scratch that", true);
        match out3 {
            ControllerOutput::ReplaceText { backspaces, text } => {
                assert_eq!(backspaces, " with PostgreSQL".chars().count());
                assert_eq!(text, "");
            }
            _ => panic!("Expected ReplaceText"),
        }
    }
}
