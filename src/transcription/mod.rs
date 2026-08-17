// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use anyhow::{Context, Result};
use std::path::PathBuf;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::AudioData;
use crate::settings::Settings;

pub struct Engine {
    ctx: Option<WhisperContext>,
    model_path: PathBuf,
    language: String,
    translate: bool,
}

#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

impl Engine {
    pub async fn new(settings: &Settings) -> Result<Self> {
        let model_path = Settings::models_dir().join(format!("ggml-{}.bin", settings.transcription.model));

        let mut engine = Self {
            ctx: None,
            model_path,
            language: settings.transcription.language.clone(),
            translate: settings.transcription.translate_to_english,
        };

        if engine.model_path.exists() {
            engine.load_model()?;
        }

        Ok(engine)
    }

    pub fn load_model(&mut self) -> Result<()> {
        let params = WhisperContextParameters::default();
        let ctx = WhisperContext::new_with_params(
            self.model_path.to_str().unwrap(),
            params,
        )
        .context("Failed to load whisper model")?;
        self.ctx = Some(ctx);
        tracing::info!("Model loaded: {:?}", self.model_path);
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.ctx.is_some()
    }

    pub async fn transcribe(&self, audio: &AudioData) -> Result<String> {
        let ctx = self.ctx.as_ref().context("Model not loaded")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&self.language));
        params.set_translate(self.translate);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);

        let mut state = ctx.create_state().context("Failed to create whisper state")?;
        state.full(params, &audio.samples).context("Transcription failed")?;

        let n = state.full_n_segments();
        let mut text = String::new();

        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                if let Ok(s) = seg.to_str() {
                    text.push_str(s);
                }
            }
        }

        Ok(text.trim().to_string())
    }

    pub async fn transcribe_with_timestamps(&self, audio: &AudioData) -> Result<TranscriptionResult> {
        let ctx = self.ctx.as_ref().context("Model not loaded")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(&self.language));
        params.set_translate(self.translate);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_token_timestamps(true);

        let mut state = ctx.create_state().context("Failed to create whisper state")?;
        state.full(params, &audio.samples).context("Transcription failed")?;

        let n = state.full_n_segments();
        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..n {
            if let Some(seg) = state.get_segment(i) {
                let text = seg.to_str_lossy().map(|c| c.to_string()).unwrap_or_default();
                let start = seg.start_timestamp() * 10;
                let end = seg.end_timestamp() * 10;
                full_text.push_str(&text);
                segments.push(Segment { start_ms: start, end_ms: end, text });
            }
        }

        Ok(TranscriptionResult {
            text: full_text.trim().to_string(),
            segments,
        })
    }
}

/// Download a whisper model from Hugging Face
pub async fn download_model(model_name: &str) -> Result<PathBuf> {
    let models_dir = Settings::models_dir();
    std::fs::create_dir_all(&models_dir)?;

    let filename = format!("ggml-{}.bin", model_name);
    let dest = models_dir.join(&filename);

    if dest.exists() {
        tracing::info!("Model already exists: {:?}", dest);
        return Ok(dest);
    }

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        filename
    );

    tracing::info!("Downloading model from: {}", url);

    let client = reqwest::Client::new();

    // Fetch the authoritative SHA256 from Hugging Face before downloading.
    let expected = crate::model_integrity::fetch_expected_for_url(&client, &url)
        .await
        .context("Failed to fetch expected model checksum from Hugging Face")?;

    let response = client.get(&url).send().await.context("Failed to download model")?;
    let bytes = response.bytes().await?;
    std::fs::write(&dest, &bytes)?;

    // Verify integrity against the checksum published by Hugging Face. Removes
    // the file and errors out on any mismatch (possible tampering/MITM).
    use sha2::Digest;
    let actual = format!("{:x}", sha2::Sha256::digest(&bytes));
    crate::model_integrity::verify_downloaded_sha256(&dest, &actual, &expected)?;

    tracing::info!("Model downloaded to: {:?}", dest);
    Ok(dest)
}

/// List available models
pub fn available_models() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("tiny", "~75 MB", "Fastest, lower accuracy"),
        ("base", "~142 MB", "Good balance of speed and accuracy"),
        ("small", "~466 MB", "Better accuracy, slower"),
        ("medium", "~1.5 GB", "High accuracy"),
        ("large-v3", "~3 GB", "Best accuracy, slowest"),
    ]
}

/// List downloaded models
pub fn downloaded_models() -> Vec<String> {
    let models_dir = Settings::models_dir();
    if !models_dir.exists() {
        return Vec::new();
    }
    std::fs::read_dir(models_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("ggml-") && name.ends_with(".bin") {
                Some(name.trim_start_matches("ggml-").trim_end_matches(".bin").to_string())
            } else {
                None
            }
        })
        .collect()
}
