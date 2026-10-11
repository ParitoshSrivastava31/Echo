//! Deterministic post-processing of recognised speech: spoken punctuation, line breaks,
//! filler removal, inline "scratch that", and conservative self-corrections.
//! Everything here is rule-based and unit-tested; nothing rewrites meaning on a guess.

#[derive(Debug, PartialEq, Clone)]
pub enum Action {
    Insert(String),
    /// Remove the previous insertion.
    UndoLast,
    /// Remove the previous insertion, then insert this text ("scratch that, ...").
    UndoAndInsert(String),
}

pub struct CommandParser;

#[derive(Clone, Debug)]
struct Tok {
    lead: String,
    core: String,
    trail: String,
}

impl Tok {
    fn new(raw: &str) -> Self {
        let start = raw.char_indices().find(|(_, c)| c.is_alphanumeric()).map(|(i, _)| i).unwrap_or(raw.len());
        let end = raw
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_alphanumeric())
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(start)
            .max(start);
        Self { lead: raw[..start].into(), core: raw[start..end].into(), trail: raw[end..].into() }
    }
    fn key(&self) -> String {
        self.core.to_lowercase()
    }
    fn ends_sentence(&self) -> bool {
        self.trail.contains(['.', '?', '!'])
    }
    fn is_capitalised(&self) -> bool {
        self.core.chars().next().is_some_and(char::is_uppercase)
    }
}

const STANDALONE_UNDO: &[&str] = &[
    "scratch that",
    "undo that",
    "delete that",
    "delete the last sentence",
    "delete last sentence",
];

const FILLERS: &[&str] = &["um", "umm", "uh", "uhh", "uhm", "erm", "hmm", "mm", "mmm"];

// Words commonly stuttered ("the the"). Content words are left alone ("very very", "had had").
const STUTTER: &[&str] = &[
    "i", "the", "a", "an", "to", "and", "but", "or", "of", "in", "on", "at", "it", "we", "you", "he", "she",
    "they", "my", "this", "for", "with", "if", "so",
];

// "period" is kept as a word after these ("the period", "a trial period").
const PERIOD_AS_WORD: &[&str] = &["a", "the", "this", "that", "each", "every", "any", "trial", "grace", "waiting", "same", "one"];

enum Cmd {
    Punct(char),
    Break(&'static str),
    Open(char),
    Close(char),
}

impl CommandParser {
    pub fn parse(text: &str) -> Action {
        Self::parse_with(text, false)
    }

    pub fn parse_with(text: &str, suppress_fillers: bool) -> Action {
        let mut toks: Vec<Tok> = text.split_whitespace().map(Tok::new).filter(|t| !t.core.is_empty() || !t.trail.is_empty() || !t.lead.is_empty()).collect();
        if suppress_fillers {
            remove_fillers(&mut toks);
        }
        let joined: String = toks.iter().map(Tok::key).filter(|k| !k.is_empty()).collect::<Vec<_>>().join(" ");
        if STANDALONE_UNDO.contains(&joined.as_str()) {
            return Action::UndoLast;
        }

        // Leading "scratch that, <new text>" replaces the previous insertion.
        let mut undo_first = false;
        if toks.len() > 2 && toks[0].key() == "scratch" && toks[1].key() == "that" {
            toks.drain(..2);
            capitalise(&mut toks[0]);
            undo_first = true;
        }

        inline_scratch(&mut toks);
        self_corrections(&mut toks);
        remove_stutters(&mut toks);
        let text = render(&toks);
        if undo_first {
            Action::UndoAndInsert(text)
        } else {
            Action::Insert(text)
        }
    }

    pub fn process_inline(text: &str) -> String {
        match Self::parse(text) {
            Action::Insert(t) | Action::UndoAndInsert(t) => t,
            Action::UndoLast => String::new(),
        }
    }
}

fn capitalise(t: &mut Tok) {
    let mut c = t.core.chars();
    if let Some(f) = c.next() {
        t.core = f.to_uppercase().collect::<String>() + c.as_str();
    }
}

fn lowercase_first(t: &mut Tok) {
    if t.core == "I" || t.core.starts_with("I'") {
        return;
    }
    let mut c = t.core.chars();
    if let Some(f) = c.next() {
        t.core = f.to_lowercase().collect::<String>() + c.as_str();
    }
}

fn remove_fillers(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i < toks.len() {
        if FILLERS.contains(&toks[i].key().as_str()) {
            let removed = toks.remove(i);
            let at_sentence_start = i == 0 || toks[i - 1].ends_sentence();
            if let Some(next) = toks.get_mut(i) {
                if removed.is_capitalised() && at_sentence_start {
                    capitalise(next);
                }
            }
            // Keep sentence-ending punctuation the filler was carrying.
            if removed.ends_sentence() && i > 0 && toks[i - 1].trail.is_empty() {
                toks[i - 1].trail = removed.trail.clone();
            }
        } else {
            i += 1;
        }
    }
}

/// "Meet at five. Scratch that. Meet at six." -> "Meet at six."
fn inline_scratch(toks: &mut Vec<Tok>) {
    let sentence_start = |toks: &[Tok], before: usize| -> usize {
        (0..before).rev().find(|&k| toks[k].ends_sentence()).map(|k| k + 1).unwrap_or(0)
    };
    loop {
        let Some(i) = (1..toks.len().saturating_sub(1)).find(|&i| toks[i].key() == "scratch" && toks[i + 1].key() == "that") else {
            break;
        };
        let mut s = sentence_start(toks, i);
        if s == i {
            // "scratch that" begins a sentence: drop the previous sentence.
            s = sentence_start(toks, i - 1);
        }
        toks.drain(s..i + 2);
        if s < toks.len() && (s == 0 || toks[s - 1].ends_sentence()) {
            capitalise(&mut toks[s]);
        }
    }
}

/// "on Tuesday, actually Wednesday." -> "on Wednesday."
/// Only fires for a single-word replacement that ends the sentence, after a comma.
fn self_corrections(toks: &mut Vec<Tok>) {
    let mut i = 1;
    while i < toks.len() {
        let (marker_len, ok) = match toks[i].key().as_str() {
            "actually" => (1, true),
            "i" if toks.get(i + 1).is_some_and(|t| t.key() == "mean") => (2, true),
            _ => (0, false),
        };
        let y = i + marker_len;
        if ok
            && y < toks.len()
            && toks[i - 1].trail == ","
            && toks[i + marker_len - 1].trail.is_empty()
            && (y + 1 == toks.len() || toks[y].ends_sentence())
        {
            let mut replacement = toks[y].clone();
            if toks[i - 1].is_capitalised() {
                capitalise(&mut replacement);
            }
            replacement.lead = toks[i - 1].lead.clone();
            toks.splice(i - 1..=y, [replacement]);
        } else {
            i += 1;
        }
    }
}

fn remove_stutters(toks: &mut Vec<Tok>) {
    let mut i = 0;
    while i + 1 < toks.len() {
        let k = toks[i].key();
        if toks[i].trail.is_empty() && STUTTER.contains(&k.as_str()) && toks[i + 1].key() == k {
            let cap = toks[i].is_capitalised();
            toks.remove(i);
            if cap {
                capitalise(&mut toks[i]);
            }
        } else {
            i += 1;
        }
    }
}

fn match_command(toks: &[Tok], i: usize) -> Option<(Cmd, usize)> {
    let k0 = toks[i].key();
    let k1 = toks.get(i + 1).map(Tok::key).unwrap_or_default();
    let prev = if i > 0 { toks[i - 1].key() } else { String::new() };
    Some(match (k0.as_str(), k1.as_str()) {
        ("new", "paragraph") => (Cmd::Break("\n\n"), 2),
        ("new", "line") => (Cmd::Break("\n"), 2),
        ("comma", _) => (Cmd::Punct(','), 1),
        ("period", _) if i > 0 && !PERIOD_AS_WORD.contains(&prev.as_str()) => (Cmd::Punct('.'), 1),
        ("full", "stop") => (Cmd::Punct('.'), 2),
        ("question", "mark") => (Cmd::Punct('?'), 2),
        ("exclamation", "mark" | "point") => (Cmd::Punct('!'), 2),
        ("colon", _) => (Cmd::Punct(':'), 1),
        ("semicolon", _) => (Cmd::Punct(';'), 1),
        ("open", "quote") => (Cmd::Open('"'), 2),
        ("close" | "end", "quote") | ("unquote", _) => (Cmd::Close('"'), if k0 == "unquote" { 1 } else { 2 }),
        ("open", "paren" | "parenthesis" | "bracket") => (Cmd::Open('('), 2),
        ("close", "paren" | "parenthesis" | "bracket") => (Cmd::Close(')'), 2),
        _ => return None,
    })
}

fn strip_trailing_punct(out: &mut String) -> Option<char> {
    let mut last = None;
    while let Some(c) = out.chars().last() {
        if ".,?!:;".contains(c) {
            last = Some(c);
            out.pop();
        } else {
            break;
        }
    }
    last
}

fn render(toks: &[Tok]) -> String {
    let mut out = String::new();
    let (mut cap_next, mut lower_next, mut glue_next) = (false, false, false);
    let mut i = 0;
    while i < toks.len() {
        if let Some((cmd, len)) = match_command(toks, i) {
            match cmd {
                Cmd::Punct(p) => {
                    let had = strip_trailing_punct(&mut out);
                    out.push(p);
                    cap_next = ".?!".contains(p);
                    lower_next = !cap_next && (had.is_some_and(|c| ".?!".contains(c)) || toks[i + len - 1].ends_sentence());
                }
                Cmd::Break(s) => {
                    out.truncate(out.trim_end_matches(' ').len());
                    out.push_str(s);
                    cap_next = true;
                    glue_next = true;
                }
                Cmd::Open(c) => {
                    if !out.is_empty() && !out.ends_with(['\n', ' ']) {
                        out.push(' ');
                    }
                    out.push(c);
                    glue_next = true;
                }
                Cmd::Close(c) => {
                    out.truncate(out.trim_end_matches(' ').len());
                    out.push(c);
                }
            }
            i += len;
            continue;
        }
        let mut t = toks[i].clone();
        if !out.is_empty() && !glue_next && !out.ends_with('\n') {
            out.push(' ');
        }
        if cap_next {
            capitalise(&mut t);
        } else if lower_next {
            lowercase_first(&mut t);
        }
        cap_next = false;
        lower_next = false;
        glue_next = false;
        out.push_str(&t.lead);
        out.push_str(&t.core);
        out.push_str(&t.trail);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(s: &str) -> String {
        match CommandParser::parse_with(s, true) {
            Action::Insert(t) => t,
            a => panic!("expected insert, got {a:?}"),
        }
    }

    #[test]
    fn test_spoken_punctuation() {
        assert_eq!(
            ins("Hello comma this is a test period New line Are you ready question mark"),
            "Hello, this is a test.\nAre you ready?"
        );
    }

    #[test]
    fn test_spoken_punctuation_after_model_punctuation() {
        // The punctuation model may already have added marks around the spoken command.
        assert_eq!(ins("Hello, comma. This is it."), "Hello, this is it.");
    }

    #[test]
    fn test_period_as_word() {
        assert_eq!(ins("It took a period of time."), "It took a period of time.");
    }

    #[test]
    fn test_self_correction() {
        assert_eq!(ins("I will meet you on Tuesday, actually Wednesday."), "I will meet you on Wednesday.");
        assert_eq!(ins("Send it to John, I mean Peter."), "Send it to Peter.");
    }

    #[test]
    fn test_actually_in_prose_is_preserved() {
        assert_eq!(ins("I actually like it."), "I actually like it.");
        assert_eq!(ins("It's fine, actually I love it."), "It's fine, actually I love it.");
        assert_eq!(ins("I mean it."), "I mean it.");
    }

    #[test]
    fn test_undo_command() {
        assert_eq!(CommandParser::parse("scratch that"), Action::UndoLast);
        assert_eq!(CommandParser::parse("Undo that."), Action::UndoLast);
        assert_eq!(CommandParser::parse("Delete the last sentence."), Action::UndoLast);
    }

    #[test]
    fn test_inline_scratch() {
        assert_eq!(ins("Let's meet at five. Scratch that. Let's meet at six."), "Let's meet at six.");
        assert_eq!(ins("Call him tomorrow scratch that call him today."), "Call him today.");
    }

    #[test]
    fn test_leading_scratch_replaces_previous() {
        assert_eq!(
            CommandParser::parse("Scratch that, send it Friday."),
            Action::UndoAndInsert("Send it Friday.".into())
        );
    }

    #[test]
    fn test_fillers_and_stutters() {
        assert_eq!(ins("Um, so I think the the plan works."), "So I think the plan works.");
        assert_eq!(ins("It was very very good."), "It was very very good.");
    }

    #[test]
    fn test_command_words_inside_prose() {
        assert_eq!(ins("The newline character is backslash n"), "The newline character is backslash n");
    }

    #[test]
    fn test_unicode_safe() {
        assert_eq!(ins("Café comma naïve résumé period"), "Café, naïve résumé.");
    }

    #[test]
    fn test_quotes() {
        assert_eq!(ins("He said open quote hello close quote"), "He said \"hello\"");
    }
}
