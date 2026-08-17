// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Local summarization using Phi-3 Mini (GGUF) via llama.cpp.
//! Generates structured meeting notes from transcriptions.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::settings::Settings;

const MODEL_URL: &str = "https://huggingface.co/QuantFactory/Phi-3-mini-128k-instruct-GGUF/resolve/main/Phi-3-mini-128k-instruct.Q4_K_M.gguf";
const MODEL_FILENAME: &str = "Phi-3-mini-128k-instruct.Q4_K_M.gguf";

/// Minimum free space required (~2.5GB for the Q4 model)
pub const REQUIRED_SPACE_BYTES: u64 = 2_500_000_000;

/// Directory where summarization models are stored
pub fn models_dir() -> PathBuf {
    Settings::data_dir().join("summarization")
}

/// Check if the model is downloaded
pub fn is_model_downloaded(model_name: &str) -> bool {
    let path = models_dir().join(model_name).join(MODEL_FILENAME);
    path.exists() && std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 1_000_000
}

/// Check if there's enough disk space
pub fn has_enough_space() -> bool {
    let dir = models_dir();
    let _ = std::fs::create_dir_all(&dir);
    check_disk_space(&dir, REQUIRED_SPACE_BYTES).is_ok()
}

/// Returns available disk space in bytes
pub fn available_space_bytes() -> u64 {
    get_available_space(&models_dir()).unwrap_or(0)
}

fn get_available_space(path: &std::path::Path) -> Result<u64> {
    use std::process::Command;
    let output = Command::new("df")
        .args(["--output=avail", "-B1", path.to_str().unwrap_or("/")])
        .output()
        .context("Failed to run df")?;
    let text = String::from_utf8_lossy(&output.stdout);
    let avail = text.lines()
        .nth(1)
        .and_then(|l| l.trim().parse::<u64>().ok())
        .unwrap_or(0);
    Ok(avail)
}

fn check_disk_space(path: &std::path::Path, required: u64) -> Result<()> {
    let available = get_available_space(path)?;
    if available < required {
        anyhow::bail!(
            "Not enough disk space. Need {:.1} GB, but only {:.1} GB available.",
            required as f64 / 1_073_741_824.0,
            available as f64 / 1_073_741_824.0,
        );
    }
    Ok(())
}

/// Download the Phi-3 Mini GGUF model
pub async fn download_model(model_name: &str) -> Result<()> {
    let dir = models_dir().join(model_name);
    std::fs::create_dir_all(&dir)?;

    check_disk_space(&dir, REQUIRED_SPACE_BYTES)?;

    let path = dir.join(MODEL_FILENAME);
    if path.exists() && std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > 1_000_000 {
        eprintln!("  ✓ Model already downloaded");
        return Ok(());
    }
    let _ = std::fs::remove_file(&path);

    eprintln!("Downloading Phi-3 Mini Q4 GGUF (~2.3 GB)...");

    let client = reqwest::Client::new();
    let response = client.get(MODEL_URL).send().await
        .context("Failed to download model")?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed: HTTP {}", response.status());
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut file = std::fs::File::create(&path)?;
    let mut stream = response.bytes_stream();

    use futures::StreamExt;
    use std::io::Write;

    if total_size > 0 {
        eprint!("  0%");
    }

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Download interrupted")?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if total_size > 0 {
            let pct = (downloaded * 100) / total_size;
            eprint!("\r  {}% ({:.0}/{:.0} MB)", pct, downloaded as f64 / 1_048_576.0, total_size as f64 / 1_048_576.0);
        }
    }
    eprintln!("\n  ✓ Download complete");

    // Verify size
    let actual_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    if actual_size < 1_000_000_000 {
        let _ = std::fs::remove_file(&path);
        anyhow::bail!("Downloaded file too small ({} bytes). Removed.", actual_size);
    }

    Ok(())
}

/// The system prompt for meeting notes generation
const MEETING_NOTES_PROMPT: &str = r#"You are a meeting notes assistant. Given a transcript of a meeting, generate detailed structured meeting notes. Include:
- A brief overview of what the meeting was about
- Key topics discussed with important details
- Decisions made
- Action items with owners if mentioned
- Any deadlines mentioned

Be thorough and detailed. Use headers and bullet points."#;

/// Summarize text using Phi-3 Mini via llama.cpp.
pub fn summarize(text: &str, model_name: &str) -> Result<String> {
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::LlamaModel;

    if text.trim().is_empty() {
        return Ok(String::new());
    }

    let word_count = text.split_whitespace().count();
    if word_count < 30 {
        return Ok(text.to_string());
    }

    let model_path = models_dir().join(model_name).join(MODEL_FILENAME);
    if !model_path.exists() {
        anyhow::bail!("Summarization model not downloaded. Run: frogscribe --summarize <file> to auto-download.");
    }

    let prompt = format!(
        "<|system|>\n{}<|end|>\n<|user|>\nPlease generate detailed meeting notes for this transcript:\n\n{}<|end|>\n<|assistant|>\n",
        MEETING_NOTES_PROMPT, text
    );

    eprintln!("Generating meeting notes...");

    let backend = LlamaBackend::init()?;
    let model_params = LlamaModelParams::default().with_n_gpu_layers(99);
    let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
        .map_err(|e| anyhow::anyhow!("Failed to load model: {:?}", e))?;

    // Tokenize with BOS (model requires it)
    let tokens = model.str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
        .map_err(|e| anyhow::anyhow!("Tokenization failed: {:?}", e))?;

    let n_tokens = tokens.len();
    let max_new_tokens = 1024;
    let ctx_size = (n_tokens + max_new_tokens + 64) as u32;

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(std::num::NonZeroU32::new(ctx_size));
    let mut ctx = model.new_context(&backend, ctx_params)
        .map_err(|e| anyhow::anyhow!("Failed to create context: {:?}", e))?;

    eprintln!("  Input: {} tokens, generating...", n_tokens);

    // Process prompt in batches of 512
    let batch_size = 512;
    let mut n_cur = 0;
    for chunk_start in (0..n_tokens).step_by(batch_size) {
        let chunk_end = (chunk_start + batch_size).min(n_tokens);
        let is_last_chunk = chunk_end == n_tokens;

        let mut batch = LlamaBatch::new(batch_size, 1);
        for (i, &token) in tokens[chunk_start..chunk_end].iter().enumerate() {
            let is_last = is_last_chunk && (i == chunk_end - chunk_start - 1);
            batch.add(token, (chunk_start + i) as i32, &[0], is_last).unwrap();
        }
        ctx.decode(&mut batch)
            .map_err(|e| anyhow::anyhow!("Decode failed: {:?}", e))?;
        n_cur = chunk_end;
    }

    // Generate using direct logits access (greedy argmax) - proven working approach
    let mut output_tokens = Vec::new();
    let gen_start = std::time::Instant::now();
    let mut batch = LlamaBatch::new(1, 1);

    // First logits index: last token position in the final prompt batch
    let last_chunk_len = n_tokens % batch_size;
    let first_idx = if last_chunk_len == 0 { batch_size as i32 - 1 } else { last_chunk_len as i32 - 1 };
    let mut logit_idx = first_idx;

    for i in 0..max_new_tokens {
        let logits = ctx.get_logits_ith(logit_idx);

        // Greedy argmax
        let next_token_id = logits.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(idx, _)| idx)
            .unwrap() as i32;

        let token = llama_cpp_2::token::LlamaToken(next_token_id);

        if model.is_eog_token(token) {
            break;
        }

        output_tokens.push(token);

        // Progress every 20 tokens
        if (i + 1) % 20 == 0 {
            let elapsed = gen_start.elapsed().as_secs_f64();
            let tps = (i + 1) as f64 / elapsed;
            let eta = (max_new_tokens - (i + 1)) as f64 / tps;
            eprint!("\r  Generating: {} tokens ({:.1} tok/s, ~{:.0}s remaining)   ", i + 1, tps, eta);
        }

        // Decode next token
        batch.clear();
        batch.add(token, n_cur as i32, &[0], true).unwrap();
        n_cur += 1;
        ctx.decode(&mut batch)
            .map_err(|e| anyhow::anyhow!("Decode failed: {:?}", e))?;

        logit_idx = 0; // after first, always index 0
    }

    let total = gen_start.elapsed().as_secs_f64();
    eprintln!("\r  Done: {} tokens in {:.1}s ({:.1} tok/s)                  ",
        output_tokens.len(), total, output_tokens.len() as f64 / total);

    // Decode tokens to text. A single shared decoder correctly handles
    // multi-byte UTF-8 sequences that span more than one token.
    let mut output = String::new();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    for token in &output_tokens {
        let piece = model
            .token_to_piece(*token, &mut decoder, false, None)
            .map_err(|e| anyhow::anyhow!("Token decode failed: {:?}", e))?;
        output.push_str(&piece);
    }

    Ok(output.trim().to_string())
}
