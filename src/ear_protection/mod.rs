// SPDX-License-Identifier: Apache-2.0
//! Bluetooth ear protection: lowers system volume during Bluetooth HFP profile switch
//! to reduce pop/click sounds on Bluetooth headsets (AirPods, etc.).
//! Uses pactl to control PulseAudio/PipeWire volume.

use std::process::Command;
use std::sync::Mutex;

use crate::settings::EarProtection;

static SAVED_VOLUME: Mutex<Option<u32>> = Mutex::new(None);

/// Capture current volume and lower it before recording starts.
/// No-op if ear protection is disabled.
pub fn activate(mode: EarProtection) {
    if mode != EarProtection::On {
        return;
    }

    let current = get_sink_volume();
    let Some(vol) = current else {
        tracing::warn!("Ear protection: could not read volume");
        return;
    };

    let mut saved = SAVED_VOLUME.lock().unwrap();
    if saved.is_none() {
        *saved = Some(vol);
    }

    // Lower to 20% of current volume
    let target = (vol as f32 * 0.2) as u32;
    set_sink_volume(target);
    tracing::info!("Ear protection: volume {} → {}", vol, target);
}

/// Restore volume after recording stops.
pub fn deactivate() {
    let mut saved = SAVED_VOLUME.lock().unwrap();
    if let Some(vol) = saved.take() {
        set_sink_volume(vol);
        tracing::info!("Ear protection: volume restored to {}", vol);
    }
}

/// Get default sink volume as percentage (0-100)
fn get_sink_volume() -> Option<u32> {
    let output = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse "Volume: front-left: 42000 /  64% / ..."
    for part in stdout.split('/') {
        let trimmed = part.trim();
        if trimmed.ends_with('%') {
            return trimmed.trim_end_matches('%').trim().parse().ok();
        }
    }
    None
}

/// Set default sink volume as percentage
fn set_sink_volume(percent: u32) {
    let _ = Command::new("pactl")
        .args(["set-sink-volume", "@DEFAULT_SINK@", &format!("{}%", percent)])
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activate_off_is_noop() {
        // Should not panic or change state when disabled
        activate(EarProtection::Off);
        let saved = SAVED_VOLUME.lock().unwrap();
        assert!(saved.is_none());
    }
}
