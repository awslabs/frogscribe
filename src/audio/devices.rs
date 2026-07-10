// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use anyhow::Result;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// List available audio input devices via pactl
pub fn list_input_devices() -> Result<Vec<AudioDevice>> {
    let output = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let default_source = get_default_source().unwrap_or_default();

    let devices: Vec<AudioDevice> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let id = parts[1].to_string();
                // Skip monitor sources (output monitors)
                if id.contains(".monitor") {
                    return None;
                }
                let is_default = id == default_source;
                let name = id.replace("alsa_input.", "").replace('_', " ");
                Some(AudioDevice { id, name, is_default })
            } else {
                None
            }
        })
        .collect();

    Ok(devices)
}

/// Get the default PulseAudio/PipeWire source
fn get_default_source() -> Option<String> {
    let output = Command::new("pactl")
        .args(["get-default-source"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Set the default source for recording
pub fn set_default_source(device_id: &str) -> Result<()> {
    Command::new("pactl")
        .args(["set-default-source", device_id])
        .status()?;
    Ok(())
}
