// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result};
use std::process::Command;

use crate::settings::Settings;
use crate::transcription;

/// Transcribe and return the text (for use with --output)
pub async fn transcribe_file_to_string(
    path: &str,
    model_name: &str,
    language: &str,
    translate: bool,
) -> Result<String> {
    let model_path = Settings::models_dir().join(format!("ggml-{}.bin", model_name));
    if !model_path.exists() {
        eprintln!("Model '{}' not found. Downloading...", model_name);
        transcription::download_model(model_name).await?;
    }

    let samples = decode_audio_file(path)?;

    let settings = Settings {
        transcription: crate::settings::TranscriptionConfig {
            model: model_name.to_string(),
            language: language.to_string(),
            translate_to_english: translate,
            streaming: false,
        },
        ..Settings::default()
    };

    let engine = transcription::Engine::new(&settings).await?;
    let audio = crate::audio::AudioData {
        samples,
        sample_rate: 16000,
        duration_secs: 0.0,
    };

    engine.transcribe(&audio).await
}

/// Decode audio file to 16kHz mono f32 PCM using ffmpeg
fn decode_audio_file(path: &str) -> Result<Vec<f32>> {
    let output = Command::new("ffmpeg")
        .args([
            "-i", path,
            "-f", "f32le",
            "-acodec", "pcm_f32le",
            "-ar", "16000",
            "-ac", "1",
            "-",
        ])
        .output()
        .context("ffmpeg not found. Install ffmpeg for audio file transcription.")?;

    if !output.status.success() {
        anyhow::bail!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let samples: Vec<f32> = output
        .stdout
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Ok(samples)
}
