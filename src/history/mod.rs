// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::settings::Settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: u64,
    pub text: String,
    pub timestamp: String,
    pub duration_secs: f32,
    pub model: String,
    pub language: String,
}

pub struct HistoryStore {
    path: PathBuf,
    entries: Vec<HistoryEntry>,
    next_id: u64,
}

impl HistoryStore {
    pub fn new() -> Result<Self> {
        let path = Settings::data_dir().join("history.json");
        let entries = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        let next_id = entries.iter().map(|e: &HistoryEntry| e.id).max().unwrap_or(0) + 1;
        Ok(Self { path, entries, next_id })
    }

    pub fn add(&mut self, text: &str, duration_secs: f32, model: &str, language: &str) -> Result<()> {
        let entry = HistoryEntry {
            id: self.next_id,
            text: text.to_string(),
            timestamp: chrono_now(),
            duration_secs,
            model: model.to_string(),
            language: language.to_string(),
        };
        self.next_id += 1;
        self.entries.push(entry);
        self.save()
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn clear(&mut self) -> Result<()> {
        self.entries.clear();
        self.save()
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            Settings::ensure_private_dir(parent)?;
        }
        let data = serde_json::to_string_pretty(&self.entries)?;
        Settings::write_private(&self.path, data.as_bytes())?;
        Ok(())
    }
}

fn chrono_now() -> String {
    // Simple ISO timestamp without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{}", secs)
}
