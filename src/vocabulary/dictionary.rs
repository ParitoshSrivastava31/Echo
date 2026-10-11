use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Local "spoken phrase -> written form" replacements, stored as JSON the user can edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalDictionary {
    pub words: BTreeMap<String, String>,
}

impl Default for PersonalDictionary {
    fn default() -> Self {
        let defaults = [
            ("typescript", "TypeScript"),
            ("javascript", "JavaScript"),
            ("postgresql", "PostgreSQL"),
            ("postgres", "PostgreSQL"),
            ("next js", "Next.js"),
            ("nextjs", "Next.js"),
            ("node js", "Node.js"),
            ("langgraph", "LangGraph"),
            ("lang graph", "LangGraph"),
            ("lang chain", "LangChain"),
            ("langchain", "LangChain"),
            ("vercel", "Vercel"),
            ("vscode", "VS Code"),
            ("vs code", "VS Code"),
            ("github", "GitHub"),
            ("git hub", "GitHub"),
            ("api", "API"),
            ("apis", "APIs"),
            ("json", "JSON"),
            ("html", "HTML"),
            ("css", "CSS"),
            ("sql", "SQL"),
            ("graphql", "GraphQL"),
            ("kubernetes", "Kubernetes"),
            ("aws", "AWS"),
            ("gcp", "GCP"),
            ("openai", "OpenAI"),
            ("chatgpt", "ChatGPT"),
            ("chat gpt", "ChatGPT"),
            ("llm", "LLM"),
            ("llms", "LLMs"),
            ("onnx", "ONNX"),
            ("ui", "UI"),
            ("url", "URL"),
        ];
        Self { words: defaults.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }
}

impl PersonalDictionary {
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Self {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(dict) = serde_json::from_str::<PersonalDictionary>(&data) {
                return dict;
            }
            // Never overwrite a file the user hand-edited into invalid JSON.
            return Self::default();
        }
        let dict = Self::default();
        let _ = dict.save(path);
        dict
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn add(&mut self, spoken: &str, replacement: &str) {
        self.words.insert(spoken.trim().to_lowercase(), replacement.trim().to_string());
    }

    pub fn remove(&mut self, spoken: &str) {
        self.words.remove(&spoken.trim().to_lowercase());
    }

    /// Whole-word, case-insensitive replacement. Longer phrases win ("next js" before "js").
    pub fn apply(&self, text: &str) -> String {
        let mut entries: Vec<(&String, &String)> = self.words.iter().filter(|(k, _)| !k.is_empty()).collect();
        entries.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
        let mut result = text.to_string();
        for (pattern, replacement) in entries {
            result = replace_word(&result, pattern, replacement);
        }
        result
    }
}

/// ASCII case folding keeps byte offsets identical to the original string,
/// so slicing stays on valid UTF-8 boundaries even with non-ASCII text.
fn replace_word(text: &str, target: &str, replacement: &str) -> String {
    let hay = text.to_ascii_lowercase();
    let needle = target.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut from = 0;
    while let Some(found) = hay[from..].find(&needle) {
        let start = from + found;
        let end = start + needle.len();
        let left_ok = text[..start].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let right_ok = text[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if left_ok && right_ok {
            out.push_str(&text[last..start]);
            out.push_str(replacement);
            last = end;
            from = end;
        } else {
            from = start + text[start..].chars().next().map_or(1, char::len_utf8);
        }
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dictionary_replacement() {
        let dict = PersonalDictionary::default();
        assert_eq!(
            dict.apply("I love writing typescript with postgres and next js for our api"),
            "I love writing TypeScript with PostgreSQL and Next.js for our API"
        );
    }

    #[test]
    fn test_no_substring_accidental_replace() {
        let dict = PersonalDictionary::default();
        assert_eq!(dict.apply("The rapid development environment"), "The rapid development environment");
    }

    #[test]
    fn test_unicode_neighbours() {
        let dict = PersonalDictionary::default();
        assert_eq!(dict.apply("naïve api café"), "naïve API café");
        assert_eq!(dict.apply("éapi"), "éapi");
    }
}
