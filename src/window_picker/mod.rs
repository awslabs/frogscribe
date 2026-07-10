// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Window picker: queries the GNOME extension for open windows,
//! shows a GTK dialog for the user to pick a target, then activates
//! that window and pastes.

use std::process::Command;
use std::sync::{Arc, Mutex};

/// Window entry from the extension
#[derive(Debug, Clone)]
pub struct WindowEntry {
    pub id: String,
    pub title: String,
    pub wm_class: String,
}

/// Query the extension for the list of open windows
pub fn get_windows() -> Vec<WindowEntry> {
    let output = Command::new("gdbus")
        .args(["call", "--session", "--dest", "org.frogscribe.Windows",
               "--object-path", "/org/frogscribe/Windows",
               "--method", "org.frogscribe.Windows.List"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout);
            // gdbus wraps in ('...',) — extract the JSON string
            let json_str = raw.trim()
                .trim_start_matches("('")
                .trim_end_matches("',)")
                .trim_end_matches("')");
            parse_window_list(json_str)
        }
        _ => Vec::new(),
    }
}

/// Request thumbnails from the extension, returns map of window_id -> filepath
pub fn get_thumbnails() -> std::collections::HashMap<String, String> {
    let output = Command::new("gdbus")
        .args(["call", "--session", "--dest", "org.frogscribe.Windows",
               "--object-path", "/org/frogscribe/Windows",
               "--method", "org.frogscribe.Windows.GetThumbnails"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout);
            let json_str = raw.trim()
                .trim_start_matches("('")
                .trim_end_matches("',)")
                .trim_end_matches("')");
            serde_json::from_str(json_str).unwrap_or_default()
        }
        _ => std::collections::HashMap::new(),
    }
}

/// Activate a window by ID
pub fn activate_window(id: &str) {
    let _ = Command::new("gdbus")
        .args(["call", "--session", "--dest", "org.frogscribe.Windows",
               "--object-path", "/org/frogscribe/Windows",
               "--method", "org.frogscribe.Windows.Activate", id])
        .output();
}

fn parse_window_list(json: &str) -> Vec<WindowEntry> {
    // Simple JSON parsing without serde_json for the window list
    let mut entries = Vec::new();
    // The JSON is an array of {id, title, wm_class}
    if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(json) {
        for item in parsed {
            if let (Some(id), Some(title), Some(wm_class)) = (
                item.get("id").and_then(|v| v.as_str()),
                item.get("title").and_then(|v| v.as_str()),
                item.get("wm_class").and_then(|v| v.as_str()),
            ) {
                entries.push(WindowEntry {
                    id: id.to_string(),
                    title: title.to_string(),
                    wm_class: wm_class.to_string(),
                });
            }
        }
    }
    entries
}

/// Show a GTK window picker dialog. Returns the selected window ID, or None if cancelled.
/// Runs on the GTK main thread via invoke and uses a channel to return the result.
pub fn show_picker(_text: &str) -> Option<String> {
    // The extension shows a compositor-native window picker with live Clutter.Clone previews
    // and returns the selected window ID via D-Bus
    let output = Command::new("gdbus")
        .args(["call", "--session", "--dest", "org.frogscribe.Windows",
               "--object-path", "/org/frogscribe/Windows",
               "--method", "org.frogscribe.Windows.GetThumbnails"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout);
            let id = raw.trim()
                .trim_start_matches("('")
                .trim_end_matches("',)")
                .trim_end_matches("')")
                .to_string();
            if id.is_empty() { None } else { Some(id) }
        }
        _ => None,
    }
}
