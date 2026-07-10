// SPDX-License-Identifier: Apache-2.0
use regex::Regex;

use crate::settings::Settings;

/// Apply text refinement rules (equivalent to macOS RuleEngine).
/// Pipeline: filler removal → capitalization → punctuation cleanup.
pub fn apply(text: &str, settings: &Settings) -> String {
    if !settings.refinement.enabled || text.is_empty() {
        return text.to_string();
    }

    let mut result = text.to_string();

    if settings.refinement.remove_fillers {
        result = remove_fillers(&result);
    }

    if settings.refinement.fix_capitalization {
        result = fix_capitalization(&result, &settings.refinement.custom_vocabulary);
    }

    result = cleanup_whitespace(&result);
    result
}

/// Remove filler words: um, uh, like, you know, I mean, basically, actually, etc.
fn remove_fillers(text: &str) -> String {
    let filler_pattern = Regex::new(
        r"(?i)\b(um+|uh+|er+|ah+|like|you know|i mean|basically|actually|literally|so+|well)\b[,.]?\s*"
    ).unwrap();

    let result = filler_pattern.replace_all(text, " ");

    // Collapse multiple spaces
    let spaces = Regex::new(r" {2,}").unwrap();
    spaces.replace_all(&result, " ").trim().to_string()
}

/// Fix capitalization: sentence starts + custom vocabulary terms
fn fix_capitalization(text: &str, vocabulary: &[String]) -> String {
    let mut result = String::with_capacity(text.len());
    let mut capitalize_next = true;

    for ch in text.chars() {
        if capitalize_next && ch.is_alphabetic() {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
            if ch == '.' || ch == '!' || ch == '?' || ch == '\n' {
                capitalize_next = true;
            }
        }
    }

    // Apply custom vocabulary (case-sensitive replacements)
    for term in vocabulary {
        let pattern = Regex::new(&format!(r"(?i)\b{}\b", regex::escape(term))).unwrap();
        result = pattern.replace_all(&result, term.as_str()).to_string();
    }

    result
}

/// Clean up whitespace artifacts from filler removal
fn cleanup_whitespace(text: &str) -> String {
    let result = text.trim().to_string();
    // Remove space before punctuation
    let space_punct = Regex::new(r" ([.,!?;:])").unwrap();
    let result = space_punct.replace_all(&result, "$1");
    // Collapse multiple spaces
    let multi_space = Regex::new(r" {2,}").unwrap();
    multi_space.replace_all(&result, " ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_fillers() {
        assert_eq!(remove_fillers("um hello world"), "hello world");
        assert_eq!(remove_fillers("I, like, went there"), "I, went there");
        assert_eq!(remove_fillers("uh you know it works"), "it works");
    }

    #[test]
    fn test_fix_capitalization() {
        assert_eq!(fix_capitalization("hello. world", &[]), "Hello. World");
        assert_eq!(
            fix_capitalization("i use rust", &["Rust".to_string()]),
            "I use Rust"
        );
    }

    #[test]
    fn test_cleanup_whitespace() {
        assert_eq!(cleanup_whitespace("hello  world"), "hello world");
        assert_eq!(cleanup_whitespace("hello ."), "hello.");
    }
}
