// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Window picker: queries the GNOME extension for open windows, shows a
//! compositor-native picker for the user to choose a target, then activates
//! that window and pastes.
//!
//! All calls go over the daemon's own D-Bus connection — the one that owns
//! `com.frogscribe.Daemon` — rather than `gdbus` subprocesses, so the extension
//! can authorize us by well-known name owner (see H2 in docs/THREAT_MODEL.md).

use std::collections::HashMap;
use zbus::Connection;

const WIN_DEST: &str = "org.frogscribe.Windows";
const WIN_PATH: &str = "/org/frogscribe/Windows";
const WIN_IFACE: &str = "org.frogscribe.Windows";

/// Window entry from the extension
#[derive(Debug, Clone)]
pub struct WindowEntry {
    pub id: String,
    pub title: String,
    pub wm_class: String,
}

/// Call a no-argument `org.frogscribe.Windows` method that returns a single string.
async fn call_string(conn: &Connection, method: &str) -> Option<String> {
    match conn
        .call_method(Some(WIN_DEST), WIN_PATH, Some(WIN_IFACE), method, &())
        .await
    {
        Ok(msg) => msg.body().deserialize::<String>().ok(),
        Err(e) => {
            tracing::warn!("org.frogscribe.Windows.{} failed: {}", method, e);
            None
        }
    }
}

/// Query the extension for the list of open windows.
pub async fn get_windows(conn: &Connection) -> Vec<WindowEntry> {
    match call_string(conn, "List").await {
        Some(json) => parse_window_list(&json),
        None => Vec::new(),
    }
}

/// Request thumbnails from the extension, returns map of window_id -> filepath.
pub async fn get_thumbnails(conn: &Connection) -> HashMap<String, String> {
    match call_string(conn, "GetThumbnails").await {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => HashMap::new(),
    }
}

/// Activate a window by ID.
pub async fn activate_window(conn: &Connection, id: &str) {
    if let Err(e) = conn
        .call_method(Some(WIN_DEST), WIN_PATH, Some(WIN_IFACE), "Activate", &(id,))
        .await
    {
        tracing::warn!("org.frogscribe.Windows.Activate failed: {}", e);
    }
}

fn parse_window_list(json: &str) -> Vec<WindowEntry> {
    let mut entries = Vec::new();
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

/// Show the compositor-native window picker (rendered by the extension via
/// Clutter.Clone) and return the selected window ID, or None if cancelled.
pub async fn show_picker(conn: &Connection) -> Option<String> {
    match call_string(conn, "GetThumbnails").await {
        Some(id) if !id.is_empty() => Some(id),
        _ => None,
    }
}
