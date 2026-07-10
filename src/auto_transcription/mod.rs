// SPDX-License-Identifier: Apache-2.0
//! Automatic Transcription: detects when another app activates the microphone,
//! starts recording, and uses energy-based VAD to auto-stop on silence.

use anyhow::Result;
use std::process::{Command, Stdio};
use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::AppEvent;
use crate::settings::AutoTranscriptionConfig;

/// Monitor microphone activity via PulseAudio/PipeWire.
/// When another app (not frogscribe/parec) starts using the mic, sends StartRecording.
/// When that app stops using the mic, sends StopRecording.
pub async fn monitor(
    tx: mpsc::Sender<AppEvent>,
    config: AutoTranscriptionConfig,
) -> Result<()> {
    if !config.enabled {
        return Ok(());
    }

    tracing::info!("Auto-transcription monitor started (vad={}, silence={}s)", config.vad_enabled, config.silence_seconds);

    let (mic_tx, mut mic_rx) = mpsc::channel::<bool>(16);

    // Spawn pactl subscribe listener
    let mic_tx_clone = mic_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = watch_source_outputs(mic_tx_clone).await {
            tracing::error!("pactl subscribe error: {}", e);
        }
    });

    let mut is_recording = false;

    while let Some(mic_active) = mic_rx.recv().await {
        if mic_active && !is_recording {
            tracing::info!("Auto-transcription: mic activated by another app, starting recording");
            is_recording = true;
            let app = get_triggering_app().unwrap_or_else(|| "unknown".to_string());
            let _ = tx.send(AppEvent::StartAutoTranscription(app)).await;
        } else if !mic_active && is_recording {
            tracing::info!("Auto-transcription: mic deactivated, stopping recording");
            is_recording = false;
            let _ = tx.send(AppEvent::StopAutoTranscription).await;
        }
    }

    Ok(())
}

/// Watch `pactl subscribe` for source-output (mic client) events.
/// Sends true when a non-frogscribe app starts using the mic, false when it stops.
async fn watch_source_outputs(tx: mpsc::Sender<bool>) -> Result<()> {
    let mut child = Command::new("pactl")
        .args(["subscribe"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child.stdout.take().unwrap();
    let reader = std::io::BufReader::new(stdout);

    // Process subscribe events in a blocking thread
    let tx_clone = tx.clone();
    tokio::task::spawn_blocking(move || {
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            // Lines look like: Event 'new' on source-output #42
            // or: Event 'remove' on source-output #42
            if line.contains("source-output") {
                if line.contains("'new'") {
                    if let Some(app) = get_triggering_app() {
                        tracing::info!("Auto-transcription: mic activated by '{}'", app);
                        let _ = tx_clone.blocking_send(true);
                    }
                } else if line.contains("'remove'") {
                    if !has_non_own_source_outputs() {
                        let _ = tx_clone.blocking_send(false);
                    }
                }
            }
        }
    });

    Ok(())
}

/// Get the name of the most recent non-frogscribe app using the mic
fn get_triggering_app() -> Option<String> {
    let output = Command::new("pactl")
        .args(["list", "source-outputs"])
        .output().ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<&str> = text.split("Source Output #").collect();

    for entry in entries.iter().rev().skip(0) {
        if !is_own_entry(entry) {
            // Extract application.name
            for line in entry.lines() {
                if line.contains("application.name") {
                    return line.split('=').nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string());
                }
            }
            // Fallback to application.process.binary
            for line in entry.lines() {
                if line.contains("application.process.binary") {
                    return line.split('=').nth(1)
                        .map(|s| s.trim().trim_matches('"').to_string());
                }
            }
            return Some("unknown".to_string());
        }
    }
    None
}

/// Public check: are non-frogscribe apps currently using the microphone?
pub fn has_external_mic_usage() -> bool {
    has_non_own_source_outputs()
}

/// Check if there are any non-frogscribe source outputs currently active
fn has_non_own_source_outputs() -> bool {
    let output = Command::new("pactl")
        .args(["list", "source-outputs"])
        .output();

    match output {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            let entries: Vec<&str> = text.split("Source Output #").collect();
            for entry in entries.iter().skip(1) {
                if !is_own_entry(entry) {
                    return true;
                }
            }
            false
        }
        Err(_) => false,
    }
}

fn is_own_entry(entry: &str) -> bool {
    let lower = entry.to_lowercase();
    lower.contains("frogscribe") || lower.contains("parec") || lower.contains("speech-dispatcher")
}

/// Energy-based Voice Activity Detection.
/// Returns true if the audio samples contain speech above the energy threshold.
pub fn detect_voice(samples: &[f32], threshold: f32) -> bool {
    if samples.is_empty() {
        return false;
    }
    let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    energy > threshold
}

/// Default energy threshold for VAD (tuned for typical microphone input)
pub const VAD_ENERGY_THRESHOLD: f32 = 0.001;
