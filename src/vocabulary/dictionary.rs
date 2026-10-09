use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalDictionary {
    pub words: HashMap<String, String>,
}

impl Default for PersonalDictionary {
    fn default() -> Self {
        let mut words = HashMap::new();
        // Common technical terms, programming languages, and acronyms
        let defaults = [
            ("typescript", "TypeScript"),
            ("javascript", "JavaScript"),
            ("rust", "Rust"),
            ("python", "Python"),
            ("postgresql", "PostgreSQL"),
            ("postgres", "PostgreSQL"),
            ("next js", "Next.js"),
            ("nextjs", "Next.js"),
            ("langgraph", "LangGraph"),
            ("lang graph", "LangGraph"),
            ("vscode", "VS Code"),
            ("vs code", "VS Code"),
            ("github", "GitHub"),
            ("git hub", "GitHub"),
            ("docker", "Docker"),
            ("antigravity", "Antigravity"),
            ("api", "API"),
            ("apis", "APIs"),
            ("json", "JSON"),
            ("html", "HTML"),
            ("css", "CSS"),
            ("sql", "SQL"),
            ("nosql", "NoSQL"),
            ("linux", "Linux"),
            ("windows", "Windows"),
            ("graphql", "GraphQL"),
            ("kubernetes", "Kubernetes"),
            ("k8s", "K8s"),
            ("aws", "AWS"),
            ("gcp", "GCP"),
            ("gemini", "Gemini"),
            ("openai", "OpenAI"),
            ("llm", "LLM"),
            ("llms", "LLMs"),
            ("asr", "ASR"),
            ("onnx", "ONNX"),
        ];

        for (k, v) in defaults {
            words.insert(k.to_string(), v.to_string());
        }

        Self { words }
    }
}

#[allow(dead_code)]
impl PersonalDictionary {
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Self {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(dict) = serde_json::from_str::<PersonalDictionary>(&data) {
                return dict;
            }
        }
        let dict = Self::default();
        let _ = dict.save(path);
        dict
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn add(&mut self, spoken: &str, replacement: &str) {
        self.words.insert(spoken.to_lowercase().trim().to_string(), replacement.trim().to_string());
    }

    pub fn remove(&mut self, spoken: &str) {
        self.words.remove(&spoken.to_lowercase().trim().to_string());
    }

    /// Apply dictionary replacements respecting word boundaries.
    pub fn apply(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (pattern, replacement) in &self.words {
            // Case-insensitive boundary match
            result = replace_word_boundary(&result, pattern, replacement);
        }
        result
    }
}

fn replace_word_boundary(text: &str, target: &str, replacement: &str) -> String {
    let lower_text = text.to_lowercase();
    let lower_target = target.to_lowercase();

    let mut result = String::new();
    let mut last_idx = 0;

    let target_len = lower_target.len();

    while let Some(found) = lower_text[last_idx..].find(&lower_target) {
        let start = last_idx + found;
        let end = start + target_len;

        // Check left boundary: start == 0 or char before is non-alphanumeric
        let left_ok = if start == 0 {
            true
        } else {
            let prev_char = text[..start].chars().last().unwrap();
            !prev_char.is_alphanumeric()
        };

        // Check right boundary: end == text.len() or char after is non-alphanumeric
        let right_ok = if end >= text.len() {
            true
        } else {
            let next_char = text[end..].chars().next().unwrap();
            !next_char.is_alphanumeric()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dictionary_replacement() {
        let dict = PersonalDictionary::default();
        let input = "I love writing typescript with postgres and next js for our api";
        let output = dict.apply(input);
        assert_eq!(
            output,
            "I love writing TypeScript with PostgreSQL and Next.js for our API"
        );
    }

    #[test]
    fn test_no_substring_accidental_replace() {
        let dict = PersonalDictionary::default();
        let input = "The rapid development environment";
        let output = dict.apply(input);
        // "api" should not match inside "rapid"
        assert_eq!(output, "The rapid development environment");
    }
}
