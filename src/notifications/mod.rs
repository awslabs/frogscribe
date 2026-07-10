// SPDX-License-Identifier: Apache-2.0
use std::process::Command;

pub fn notify_transcription(text: &str) {
    let body = if text.len() > 100 {
        format!("{}…", &text[..100])
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
