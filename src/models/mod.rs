// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::settings::Settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperModel {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub description: String,
    pub sha256: Option<String>,
}

impl WhisperModel {
    pub fn all() -> Vec<Self> {
        vec![
            Self { id: "tiny".into(), name: "Tiny".into(), size_bytes: 75_000_000, size_label: "~75 MB".into(), description: "Fastest, good for quick notes".into(), sha256: None },
            Self { id: "base".into(), name: "Base".into(), size_bytes: 142_000_000, size_label: "~142 MB".into(), description: "Good balance of speed and accuracy".into(), sha256: None },
            Self { id: "small".into(), name: "Small".into(), size_bytes: 466_000_000, size_label: "~466 MB".into(), description: "Better accuracy, moderate speed".into(), sha256: None },
            Self { id: "medium".into(), name: "Medium".into(), size_bytes: 1_500_000_000, size_label: "~1.5 GB".into(), description: "High accuracy".into(), sha256: None },
            Self { id: "large-v3".into(), name: "Large v3".into(), size_bytes: 3_000_000_000, size_label: "~3 GB".into(), description: "Best accuracy, requires more RAM".into(), sha256: None },
        ]
    }

    pub fn path(&self) -> PathBuf {
        Settings::models_dir().join(format!("ggml-{}.bin", self.id))
    }

    pub fn is_downloaded(&self) -> bool {
        self.path().exists()
    }
}

/// Download a model with progress reporting
pub async fn download_model(model: &WhisperModel, on_progress: impl Fn(f64)) -> Result<PathBuf> {
    let models_dir = Settings::models_dir();
    std::fs::create_dir_all(&models_dir)?;

    let dest = model.path();
    if dest.exists() {
        return Ok(dest);
    }

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        model.id
    );

    let client = reqwest::Client::new();

    // Fetch the authoritative SHA256 from Hugging Face before downloading, so we
    // fail fast if the model is unavailable and can verify what we receive.
    let expected = crate::model_integrity::fetch_expected_for_url(&client, &url)
        .await
        .context("Failed to fetch expected model checksum from Hugging Face")?;

    let response = client.get(&url).send().await.context("Failed to start download")?;
    let total = response.content_length().unwrap_or(expected.size);

    let mut file = std::fs::File::create(&dest)?;
    let mut downloaded: u64 = 0;
    let mut hasher = Sha256::new();

    use futures::StreamExt;
    use std::io::Write;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Download interrupted")?;
        file.write_all(&chunk)?;
        hasher.update(&chunk);
        downloaded += chunk.len() as u64;
        on_progress(downloaded as f64 / total as f64);
    }
    file.flush()?;
    drop(file);

    // Verify integrity against the checksum published by Hugging Face. On any
    // mismatch the partial/tampered file is removed and an error is returned.
    let actual = format!("{:x}", hasher.finalize());
    crate::model_integrity::verify_downloaded_sha256(&dest, &actual, &expected)?;

    on_progress(1.0);
    Ok(dest)
}

/// Delete a downloaded model
pub fn delete_model(model: &WhisperModel) -> Result<()> {
    let path = model.path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Health check: verify model file integrity via size check
pub fn check_model_health(model: &WhisperModel) -> ModelHealth {
    let path = model.path();
    if !path.exists() {
        return ModelHealth::NotDownloaded;
    }

    match std::fs::metadata(&path) {
        Ok(meta) => {
            // Basic size sanity check (at least 50% of expected)
            if meta.len() < model.size_bytes / 2 {
                ModelHealth::Corrupted("File is too small, likely incomplete download".into())
            } else {
                ModelHealth::Healthy
            }
        }
        Err(e) => ModelHealth::Corrupted(format!("Cannot read file: {}", e)),
    }
}

/// Compute SHA256 of a model file (for full integrity verification)
pub fn compute_sha256(model: &WhisperModel) -> Result<String> {
    let path = model.path();
    let data = std::fs::read(&path)?;
    let hash = Sha256::digest(&data);
    Ok(format!("{:x}", hash))
}

#[derive(Debug, Clone)]
pub enum ModelHealth {
    Healthy,
    NotDownloaded,
    Corrupted(String),
}
