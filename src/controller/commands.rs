#[derive(Debug, PartialEq, Clone)]
pub enum Action {
    Insert(String),
    UndoLast,
    DeleteSentence,
}

pub struct CommandParser;

impl CommandParser {
    /// Inspect incoming speech for top-level commands or in-line spoken punctuation/corrections.
    pub fn parse(text: &str) -> Action {
        let trimmed = text.trim();
        let lower = trimmed.to_lowercase();

        // 1. Standalone command actions
        if lower == "scratch that"
            || lower == "undo that"
            || lower == "scratch that."
            || lower == "undo that."
        {
            return Action::UndoLast;
        }

        if lower == "delete the last sentence"
            || lower == "delete last sentence"
            || lower == "delete the last sentence."
        {
            return Action::DeleteSentence;
        }

        // 2. Process in-line commands and spoken punctuation
        let processed = Self::process_inline(&trimmed);
        Action::Insert(processed)
    }

    /// Convert spoken punctuation, newlines, and self-corrections inside the text.
    pub fn process_inline(text: &str) -> String {
        let mut s = text.to_string();

        // 1. Spoken newlines
        s = replace_phrase(&s, "new paragraph", "\n\n");
        s = replace_phrase(&s, "new line", "\n");
        s = s.replace(" \n", "\n").replace("\n ", "\n");

        // 2. Spoken punctuation phrases
        s = replace_phrase(&s, "comma", ",");
        s = replace_phrase(&s, "period", ".");
        s = replace_phrase(&s, "full stop", ".");
        s = replace_phrase(&s, "question mark", "?");
        s = replace_phrase(&s, "exclamation mark", "!");
        s = replace_phrase(&s, "exclamation point", "!");
        s = replace_phrase(&s, "open quote", "\"");
        s = replace_phrase(&s, "close quote", "\"");
        s = replace_phrase(&s, "colon", ":");
        s = replace_phrase(&s, "semicolon", ";");

        // 3. Spoken self-correction: "... word, actually replacement" or "... word, I mean replacement"
        s = handle_self_corrections(&s);

        // 4. Punctuation spacing cleanup
        cleanup_punctuation_spacing(&s)
    }
}

/// Helper to replace phrases case-insensitively with word boundary awareness
fn replace_phrase(text: &str, phrase: &str, replacement: &str) -> String {
    let lower_text = text.to_lowercase();
    let lower_phrase = phrase.to_lowercase();

    let mut result = String::new();
    let mut last_idx = 0;
    let phrase_len = lower_phrase.len();

    while let Some(found) = lower_text[last_idx..].find(&lower_phrase) {
        let start = last_idx + found;
        let end = start + phrase_len;

        let left_ok = if start == 0 {
            true
        } else {
            let prev = text[..start].chars().last().unwrap();
            !prev.is_alphanumeric()
        };

        let right_ok = if end >= text.len() {
            true
        } else {
            let next = text[end..].chars().next().unwrap();
            !next.is_alphanumeric()
        };

        if left_ok && right_ok {
            result.push_str(&text[last_idx..start]);
            result.push_str(replacement);
            last_idx = end;
        } else {
            result.push_str(&text[last_idx..start + 1]);
            last_idx = start + 1;
        }
    }

    result.push_str(&text[last_idx..]);
    result
}

/// Handle self-corrections like "meet on Tuesday, actually Wednesday" -> "meet on Wednesday"
fn handle_self_corrections(text: &str) -> String {
    let patterns = [", actually ", " actually ", ", I mean ", " I mean "];
    let mut res = text.to_string();

    for &pat in &patterns {
        while let Some(idx) = res.to_lowercase().find(pat) {
            let before = &res[..idx];
            let after = &res[idx + pat.len()..];

            // Find last word before `pat`
            let mut before_words: Vec<&str> = before.split_whitespace().collect();
            if !before_words.is_empty() && !after.trim().is_empty() {
                before_words.pop(); // Remove the mistaken word
                let new_before = before_words.join(" ");
                res = if new_before.is_empty() {
                    after.to_string()
                } else {
                    format!("{} {}", new_before, after)
                };
            } else {
                break;
            }
        }
    }

    res
}

/// Clean up spaces around punctuation (e.g., "Hello , world ." -> "Hello, world.")
fn cleanup_punctuation_spacing(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == ' ' {
            // If next char is punctuation, don't emit the space
            if let Some(&next) = chars.peek() {
                if next == '.' || next == ',' || next == '?' || next == '!' || next == ':' || next == ';' {
                    continue;
                }
            }
        }
        out.push(c);
        // If char is punctuation, ensure next character is space or newline (unless already whitespace or end)
        if c == '.' || c == ',' || c == '?' || c == '!' || c == ':' || c == ';' {
            if let Some(&next) = chars.peek() {
                if !next.is_whitespace() && next != '"' && next != '\'' {
                    out.push(' ');
                }
            }
        }
    }

    out.replace(" \n", "\n").replace("\n ", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spoken_punctuation() {
        let input = "Hello comma this is a test period New line Are you ready question mark";
        let parsed = CommandParser::parse(input);
        match parsed {
            Action::Insert(text) => {
                assert_eq!(text, "Hello, this is a test.\nAre you ready?");
            }
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn test_self_correction() {
        let input = "I will meet you on Tuesday, actually Wednesday.";
        let parsed = CommandParser::parse(input);
        match parsed {
            Action::Insert(text) => {
                assert_eq!(text, "I will meet you on Wednesday.");
            }
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn test_undo_command() {
        let input = "scratch that";
        assert_eq!(CommandParser::parse(input), Action::UndoLast);

        let input2 = "Undo that.";
        assert_eq!(CommandParser::parse(input2), Action::UndoLast);
    }
}
