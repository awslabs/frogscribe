// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result};
use arboard::Clipboard;
use std::process::Command;
use std::time::Duration;

/// Resolve the ydotoold socket path.
///
/// Only user-private locations are trusted: an explicit `$YDOTOOL_SOCKET`, or
/// the per-user socket in `$XDG_RUNTIME_DIR` (a 0700 user-owned directory). We
/// deliberately do NOT fall back to a world-accessible path such as
/// `/tmp/.ydotool_socket`: a world-writable input-injection socket can be
/// squatted or hijacked by any local process (see T3 in docs/THREAT_MODEL.md).
fn ydotool_socket_path() -> Option<String> {
    if let Ok(path) = std::env::var("YDOTOOL_SOCKET") {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
    }
    // Per-user socket in the user runtime dir (the only path we create).
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let user_path = format!("{}/.ydotool_socket", runtime_dir);
        if std::path::Path::new(&user_path).exists() {
            return Some(user_path);
        }
    }
    None
}

/// Check if ydotool is usable. Returns Ok(()) if ready, or an error with
/// user-facing guidance on how to fix it.
pub fn check_ydotool() -> Result<(), String> {
    let has_ydotool = Command::new("which")
        .arg("ydotool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !has_ydotool {
        let has_xdotool = Command::new("which")
            .arg("xdotool")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if has_xdotool {
            return Ok(()); // xdotool fallback available
        }
        return Err("ydotool not installed. Install with: sudo dnf install ydotool (or apt install ydotool)".into());
    }

    // ydotool is installed — check if the socket is accessible
    let socket = match ydotool_socket_path() {
        Some(s) => s,
        None => {
            return Err(
                "ydotoold is not running. Fix: systemctl --user enable --now ydotoold".into()
            );
        }
    };

    // Try connecting — run a no-op key event to test
    let result = Command::new("ydotool")
        .env("YDOTOOL_SOCKET", &socket)
        .args(["key", ""])
        .output();

    match result {
        Ok(output) => {
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if combined.contains("Permission denied") {
                return Err(format!(
                    "ydotool socket ({}) not accessible. It should be your \
                     per-user socket in $XDG_RUNTIME_DIR. Fix: \
                     systemctl --user restart ydotoold",
                    socket
                ));
            }
            // Any other error (including "invalid key") means the socket works
            Ok(())
        }
        Err(_) => Ok(()), // couldn't run ydotool at all — will fail later with context
    }
}

/// Insert text at the cursor position in the active application.
/// Strategy depends on settings: type character-by-character or clipboard paste.
pub async fn insert_text(text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }

    // Sanitize: strip control characters that could drive input injection.
    // Newline (\n) is deliberately preserved to support multi-line / long-form
    // dictation; tab and every other control character are removed. A preserved
    // newline still acts as Enter in a focused terminal (see T1 in
    // docs/THREAT_MODEL.md), which is why the window picker defaults to on.
    let text: String = text.chars().filter(|c| {
        !c.is_control() || *c == '\n'
    }).collect();
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }

    let method = crate::settings::Settings::load()
        .map(|s| s.general.insertion_method)
        .unwrap_or_default();

    // Note: Settings are cached by the caller (main.rs reloads per-recording).
    // This load is a fallback for direct callers; consider passing method as param.

    match method {
        crate::settings::InsertionMethod::Off => {
            // Just copy to clipboard, don't type anything
            let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
            clipboard.set_text(text).context("Failed to set clipboard")?;
            return Ok(());
        }
        crate::settings::InsertionMethod::TypeEveryCharacter => {
            let ydotool_ok = if let Some(socket) = ydotool_socket_path() {
                Command::new("ydotool")
                    .env("YDOTOOL_SOCKET", &socket)
                    .args(["type", "--", text])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            } else {
                false
            };
            if !ydotool_ok {
                tracing::debug!("ydotool type failed, trying xdotool clipboard paste");
                let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
                clipboard.set_text(text).context("Failed to set clipboard")?;
                tokio::time::sleep(Duration::from_millis(50)).await;
                Command::new("xdotool")
                    .args(["key", "ctrl+v"])
                    .status()
                    .context("Text insertion failed. Install ydotool or xdotool.")?;
            }
        }
        crate::settings::InsertionMethod::PasteFullTranscript => {
            let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
            let saved = clipboard.get_text().ok();
            clipboard.set_text(text).context("Failed to set clipboard")?;
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Ctrl+V via ydotool key codes
            let ydotool_ok = if let Some(socket) = ydotool_socket_path() {
                Command::new("ydotool")
                    .env("YDOTOOL_SOCKET", &socket)
                    .args(["key", "29:1", "47:1", "47:0", "29:0"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            } else {
                false
            };
            if !ydotool_ok {
                Command::new("xdotool")
                    .args(["key", "ctrl+v"])
                    .status()
                    .context("Text insertion failed. Install ydotool or xdotool.")?;
            }
            // Restore clipboard
            tokio::time::sleep(Duration::from_millis(200)).await;
            if let Some(saved_text) = saved {
                let _ = clipboard.set_text(saved_text);
            }
        }
    }

    Ok(())
}

/// Simulate pressing Enter (for auto-submit feature)
pub async fn press_enter() -> Result<()> {
    let ydotool_ok = if let Some(socket) = ydotool_socket_path() {
        Command::new("ydotool")
            .env("YDOTOOL_SOCKET", &socket)
            .args(["key", "28:1", "28:0"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        false
    };

    if !ydotool_ok {
        Command::new("xdotool")
            .args(["key", "Return"])
            .status()
            .context("Failed to simulate Enter key")?;
    }

    Ok(())
}
