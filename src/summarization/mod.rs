// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Local summarization using BART models via ONNX Runtime.
//! Supports distilbart-cnn-12-6 (fast) and bart-large-cnn (best quality).

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::settings::Settings;

const HF_BASE: &str = "https://huggingface.co";

/// Model info for download/loading
struct ModelInfo {
    encoder_url: &'static str,
    decoder_url: &'static str,
    tokenizer_url: &'static str,
}

fn model_info(model_name: &str) -> ModelInfo {
    match model_name {
        "bart-large-cnn" => ModelInfo {
            encoder_url: "https://huggingface.co/facebook/bart-large-cnn/resolve/main/onnx/encoder_model.onnx",
            decoder_url: "https://huggingface.co/facebook/bart-large-cnn/resolve/main/onnx/decoder_model.onnx",
            tokenizer_url: "https://huggingface.co/facebook/bart-large-cnn/resolve/main/tokenizer.json",
        },
        _ => ModelInfo { // distilbart-cnn-12-6
            encoder_url: "https://huggingface.co/sshleifer/distilbart-cnn-12-6/resolve/main/onnx/encoder_model.onnx",
            decoder_url: "https://huggingface.co/sshleifer/distilbart-cnn-12-6/resolve/main/onnx/decoder_model.onnx",
            tokenizer_url: "https://huggingface.co/sshleifer/distilbart-cnn-12-6/resolve/main/tokenizer.json",
        },
    }
}

/// Directory where summarization models are stored
pub fn models_dir() -> PathBuf {
    Settings::data_dir().join("summarization")
}

/// Check if a model is downloaded
pub fn is_model_downloaded(model_name: &str) -> bool {
    let dir = models_dir().join(model_name);
    dir.join("encoder_model.onnx").exists()
        && dir.join("decoder_model.onnx").exists()
        && dir.join("tokenizer.json").exists()
}

/// Download a summarization model
pub async fn download_model(model_name: &str) -> Result<()> {
    let info = model_info(model_name);
    let dir = models_dir().join(model_name);
    std::fs::create_dir_all(&dir)?;

    tracing::info!("Downloading summarization model: {}", model_name);

    let client = reqwest::Client::new();

    for (url, filename) in [
        (info.encoder_url, "encoder_model.onnx"),
        (info.decoder_url, "decoder_model.onnx"),
        (info.tokenizer_url, "tokenizer.json"),
    ] {
        let path = dir.join(filename);
        if path.exists() {
            tracing::info!("  {} already exists, skipping", filename);
            continue;
        }
        tracing::info!("  Downloading {}...", filename);
        let response = client.get(url).send().await
            .context(format!("Failed to download {}", filename))?;
        let bytes = response.bytes().await?;
        std::fs::write(&path, &bytes)?;
        tracing::info!("  {} downloaded ({} bytes)", filename, bytes.len());
    }

    tracing::info!("Model {} ready", model_name);
    Ok(())
}

/// Summarize text using the specified local model.
/// Returns the summary string, or an error.
pub fn summarize(text: &str, model_name: &str) -> Result<String> {
    if text.trim().is_empty() {
        return Ok(String::new());
    }

    // Don't summarize very short texts
    let word_count = text.split_whitespace().count();
    if word_count < 30 {
        return Ok(text.to_string());
    }

    let dir = models_dir().join(model_name);
    if !is_model_downloaded(model_name) {
        anyhow::bail!("Summarization model '{}' not downloaded. Download it in Settings.", model_name);
    }

    let tokenizer_path = dir.join("tokenizer.json");
    let encoder_path = dir.join("encoder_model.onnx");
    let decoder_path = dir.join("decoder_model.onnx");

    // Load tokenizer
    let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    // Tokenize input
    let encoding = tokenizer.encode(text, true)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

    let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
    let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&m| m as i64).collect();

    // Truncate to max 1024 tokens (BART's limit)
    let max_len = 1024.min(input_ids.len());
    let input_ids = &input_ids[..max_len];
    let attention_mask = &attention_mask[..max_len];

    // Run encoder
    let mut session = ort::session::Session::builder()?
        .commit_from_file(&encoder_path)?;

    let input_ids_tensor = ort::value::Tensor::<i64>::from_array(
        ([1, input_ids.len()], input_ids.to_vec())
    )?;
    let attention_mask_tensor = ort::value::Tensor::<i64>::from_array(
        ([1, attention_mask.len()], attention_mask.to_vec())
    )?;

    let encoder_outputs = session.run(ort::inputs![
        input_ids_tensor,
        attention_mask_tensor,
    ])?;

    let encoder_hidden_tensor = &encoder_outputs[0];

    // Run decoder autoregressively
    let mut decoder_session = ort::session::Session::builder()?
        .commit_from_file(&decoder_path)?;

    // Start with BOS token (2 for BART)
    let eos_token_id: i64 = 2;
    let max_summary_len = 150;

    let mut generated_ids: Vec<i64> = vec![2]; // </s> as BOS for BART

    for _ in 0..max_summary_len {
        let decoder_input = ort::value::Tensor::<i64>::from_array(
            ([1, generated_ids.len()], generated_ids.clone())
        )?;

        let decoder_outputs = decoder_session.run(ort::inputs![
            decoder_input,
        ])?;

        let logits = decoder_outputs[0].try_extract_tensor::<f32>()?;
        let (shape, data) = logits;
        // shape is [1, seq_len, vocab_size]
        let vocab_size = shape[2] as usize;
        let seq_len = shape[1] as usize;
        let last_start = (seq_len - 1) * vocab_size;
        let last_logits = &data[last_start..last_start + vocab_size];

        // Greedy: argmax of last position
        let next_token = last_logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(idx, _)| idx as i64)
            .unwrap_or(eos_token_id);

        if next_token == eos_token_id {
            break;
        }
        generated_ids.push(next_token);
    }

    // Decode tokens back to text
    let output_ids: Vec<u32> = generated_ids.iter().map(|&id| id as u32).collect();
    let summary = tokenizer.decode(&output_ids, true)
        .map_err(|e| anyhow::anyhow!("Decoding failed: {}", e))?;

    Ok(summary.trim().to_string())
}
