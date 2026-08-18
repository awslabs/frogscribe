// SPDX-License-Identifier: Apache-2.0
use std::process::Command;

pub fn notify_transcription(text: &str) {
    // Truncate on a UTF-8 char boundary (byte slicing panics mid-character,
    // e.g. for CJK/Cyrillic/accented text — see 99+ supported languages).
    let body = if text.chars().count() > 100 {
        let truncated: String = text.chars().take(100).collect();
        format!("{}…", truncated)
    } else {
        text.to_string()
    };
    let _ = Command::new("notify-send")
        .args(["FrogScribe", &body])
        .spawn();
}

pub fn notify_error(message: &str) {
    let _ = Command::new("notify-send")
        .args(["--urgency=critical", "FrogScribe Error", message])
        .spawn();
}
