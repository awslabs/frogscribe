// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
pub mod devices;

use anyhow::{Context, Result};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use crate::settings::Settings;

/// Raw audio data in whisper.cpp's expected format: 16kHz mono f32 PCM
#[derive(Debug, Clone)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration_secs: f32,
}

pub struct Recorder {
    recording: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    office_mode: bool,
    device: Option<String>,
    capture_desktop_audio: bool,
}

impl Recorder {
    pub fn new(settings: &Settings) -> Result<Self> {
        Ok(Self {
            recording: Arc::new(AtomicBool::new(false)),
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: settings.audio.sample_rate,
            office_mode: settings.audio.office_mode,
            device: settings.audio.device.clone(),
            capture_desktop_audio: settings.audio.capture_desktop_audio,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::Relaxed)
    }

    pub fn samples_ref(&self) -> Arc<Mutex<Vec<f32>>> {
        self.samples.clone()
    }

    pub fn start(&mut self) -> Result<()> {
        self.samples.lock().unwrap().clear();
        self.recording.store(true, Ordering::Relaxed);

        let samples = self.samples.clone();
        let recording = self.recording.clone();
        let device = self.device.clone();

        // Capture from microphone
        std::thread::spawn(move || {
            if let Err(e) = capture_audio(samples, recording, device) {
                tracing::error!("Audio capture error: {}", e);
            }
        });

        // Capture desktop audio (monitor source) if enabled
        if self.capture_desktop_audio {
            let samples_desktop = self.samples.clone();
            let recording_desktop = self.recording.clone();
            std::thread::spawn(move || {
                if let Some(monitor) = detect_monitor_source() {
                    tracing::info!("Desktop audio capture from: {}", monitor);
                    if let Err(e) = capture_audio(samples_desktop, recording_desktop, Some(monitor)) {
                        tracing::error!("Desktop audio capture error: {}", e);
                    }
                } else {
                    tracing::warn!("No monitor source found for desktop audio capture");
                }
            });
        }

        Ok(())
    }

    pub fn stop(&mut self) -> Result<Option<AudioData>> {
        if !self.recording.load(Ordering::Relaxed) {
            return Ok(None);
        }
        self.recording.store(false, Ordering::Relaxed);

        // Give capture thread time to flush
        std::thread::sleep(std::time::Duration::from_millis(100));

        let mut samples = self.samples.lock().unwrap();
        if samples.is_empty() {
            return Ok(None);
        }

        let mut audio_samples = std::mem::take(&mut *samples);

        if self.office_mode {
            normalize_audio(&mut audio_samples);
        }

        let duration_secs = audio_samples.len() as f32 / self.sample_rate as f32;

        Ok(Some(AudioData {
            samples: audio_samples,
            sample_rate: self.sample_rate,
            duration_secs,
        }))
    }

    /// Extract accumulated samples without stopping (for long-form/streaming)
    pub fn extract_samples(&self) -> Option<AudioData> {
        let mut samples = self.samples.lock().unwrap();
        if samples.is_empty() {
            return None;
        }
        let audio_samples = std::mem::take(&mut *samples);
        let duration_secs = audio_samples.len() as f32 / self.sample_rate as f32;
        Some(AudioData {
            samples: audio_samples,
            sample_rate: self.sample_rate,
            duration_secs,
        })
    }
}

/// Capture audio via parec (PulseAudio/PipeWire compatible)
fn capture_audio(
    samples: Arc<Mutex<Vec<f32>>>,
    recording: Arc<AtomicBool>,
    device: Option<String>,
) -> Result<()> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut args = vec![
        "--format=float32le".to_string(),
        "--rate=16000".to_string(),
        "--channels=1".to_string(),
        "--latency-msec=50".to_string(),
    ];

    if let Some(dev) = &device {
        args.push(format!("--device={}", dev));
    }

    let mut child = Command::new("parec")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start audio capture (parec). Is PulseAudio/PipeWire installed?")?;

    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 6400]; // 100ms of f32 samples at 16kHz

    while recording.load(Ordering::Relaxed) {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let float_samples: Vec<f32> = buf[..n]
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                samples.lock().unwrap().extend(float_samples);
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    Ok(())
}

/// Detect the monitor source for the default audio output (speakers/headphones).
/// This captures what's being played through the system's default sink.
fn detect_monitor_source() -> Option<String> {
    use std::process::Command;

    // Get the default sink name
    let output = Command::new("pactl")
        .args(["get-default-sink"])
        .output()
        .ok()?;
    let sink = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sink.is_empty() { return None; }

    // The monitor source is typically "<sink_name>.monitor"
    let monitor = format!("{}.monitor", sink);

    // Verify it exists by listing sources
    let sources_output = Command::new("pactl")
        .args(["list", "sources", "short"])
        .output()
        .ok()?;
    let sources = String::from_utf8_lossy(&sources_output.stdout);
    if sources.contains(&monitor) {
        Some(monitor)
    } else {
        // Fallback: find any source containing ".monitor"
        for line in sources.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 && parts[1].ends_with(".monitor") {
                return Some(parts[1].to_string());
            }
        }
        None
    }
}

/// Peak normalization for office mode (boost quiet audio)
fn normalize_audio(samples: &mut [f32]) {
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak > 0.0 && peak < 0.5 {
        let gain = 0.9 / peak;
        for sample in samples.iter_mut() {
            *sample *= gain;
        }
    }
}
