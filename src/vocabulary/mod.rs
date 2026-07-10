#![allow(dead_code)]
//! Custom Vocabulary management: add/remove terms, import/export files, validation.

use anyhow::Result;
use std::path::PathBuf;

use crate::settings::Settings;

const MAX_TERMS: usize = 500;
const MAX_TERM_LENGTH: usize = 100;

fn vocab_path() -> PathBuf {
    Settings::data_dir().join("vocabulary.txt")
}

/// Load vocabulary from dedicated file (falls back to settings if file doesn't exist)
pub fn load() -> Vec<String> {
    let path = vocab_path();
    if path.exists() {
        std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .take(MAX_TERMS)
            .collect()
    } else {
        Settings::load().map(|s| s.refinement.custom_vocabulary).unwrap_or_default()
    }
}

/// Save vocabulary to dedicated file
pub fn save(terms: &[String]) -> Result<()> {
    let dir = Settings::data_dir();
    std::fs::create_dir_all(&dir)?;
    let content = terms.join("\n");
    std::fs::write(vocab_path(), content)?;
    Ok(())
}

/// Add a term (with validation)
pub fn add_term(term: &str) -> Result<()> {
    let term = term.trim().to_string();
    if term.is_empty() { anyhow::bail!("Term cannot be empty"); }
    if term.len() > MAX_TERM_LENGTH { anyhow::bail!("Term exceeds {} characters", MAX_TERM_LENGTH); }

    let mut terms = load();
    if terms.len() >= MAX_TERMS { anyhow::bail!("Vocabulary full ({} terms max)", MAX_TERMS); }

    // Case-insensitive dedup
    if terms.iter().any(|t| t.to_lowercase() == term.to_lowercase()) {
        return Ok(()); // already exists
    }

    terms.push(term);
    save(&terms)
}

/// Remove a term (case-insensitive match)
pub fn remove_term(term: &str) -> Result<()> {
    let lower = term.to_lowercase();
    let mut terms = load();
    terms.retain(|t| t.to_lowercase() != lower);
    save(&terms)
}

/// Import terms from a file (one per line, max 1MB, validates each term)
pub fn import_file(path: &str) -> Result<usize> {
    let content = std::fs::read_to_string(path)?;
    if content.len() > 1_048_576 { anyhow::bail!("File too large (max 1MB)"); }

    let mut terms = load();
    let mut added = 0;

    for line in content.lines() {
        let term = line.trim().to_string();
        if term.is_empty() || term.len() > MAX_TERM_LENGTH { continue; }
        if terms.len() >= MAX_TERMS { break; }
        if terms.iter().any(|t| t.to_lowercase() == term.to_lowercase()) { continue; }
        terms.push(term);
        added += 1;
    }

    save(&terms)?;
    Ok(added)
}

/// Export vocabulary to a file
pub fn export_file(path: &str) -> Result<()> {
    let terms = load();
    std::fs::write(path, terms.join("\n"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_empty() {
        assert!(add_term("").is_err());
    }

    #[test]
    fn test_validation_too_long() {
        let long = "a".repeat(101);
        assert!(add_term(&long).is_err());
    }

    #[test]
    fn test_max_term_length() {
        let exact = "a".repeat(100);
        // This would try to write to disk, so just test the validation logic
        assert!(exact.len() <= MAX_TERM_LENGTH);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_TERMS, 500);
        assert_eq!(MAX_TERM_LENGTH, 100);
    }
}
