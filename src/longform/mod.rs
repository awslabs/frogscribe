// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

use crate::audio::Recorder;
use crate::refinement;
use crate::settings::Settings;
use crate::transcription::Engine;

const CHUNK_DURATION_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSession {
    pub title: String,
    pub started_at: u64,
    pub duration_secs: u64,
    pub chunks: Vec<TranscriptChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptChunk {
    pub text: String,
    pub offset_secs: u64,
    pub duration_secs: f32,
}

impl TranscriptSession {
    pub fn full_text(&self) -> String {
        self.chunks.iter().map(|c| c.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// Events emitted by the long-form session for UI/caller consumption
#[derive(Debug, Clone)]
pub enum LongFormEvent {
    ChunkProcessing,
    ChunkTranscribed { text: String, elapsed_secs: u64 },
    SessionComplete { session: TranscriptSession },
    Error { message: String },
}

/// Run a long-form dictation session with chunked transcription.
/// Transcribes every 30 seconds, appending chunks to the session.
/// Returns a receiver for events and a stop handle.
pub async fn start_session(
    recorder: &mut Recorder,
    _engine: &Engine,
    settings: &Settings,
) -> Result<(mpsc::Receiver<LongFormEvent>, Arc<AtomicBool>)> {
    let (tx, rx) = mpsc::channel(32);
    let stop = Arc::new(AtomicBool::new(false));

    recorder.start()?;

    let stop_clone = stop.clone();
    let samples_ref = recorder.samples_ref();
    let settings_clone = settings.clone();

    // We need to run the chunked transcription loop
    // Since Engine isn't Send, we do transcription on the current task
    // and use a timer-based approach
    let tx_clone = tx.clone();
    let engine_model_path = Settings::models_dir().join(format!("ggml-{}.bin", settings.transcription.model));
    let language = settings.transcription.language.clone();
    let translate = settings.transcription.translate_to_english;

    tokio::spawn(async move {
        let start_time = Instant::now();
        let mut session = TranscriptSession {
            title: format!("Session {}", timestamp_now()),
            started_at: timestamp_now(),
            duration_secs: 0,
            chunks: Vec::new(),
        };

        // Load a dedicated whisper context for this session
        let ctx = match whisper_rs::WhisperContext::new_with_params(
            engine_model_path.to_str().unwrap_or(""),
            whisper_rs::WhisperContextParameters::default(),
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx_clone.send(LongFormEvent::Error {
                    message: format!("Failed to load model: {}", e),
                }).await;
                return;
            }
        };

        loop {
            // Wait for chunk duration, checking stop flag frequently
            let chunk_start = tokio::time::Instant::now();
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if stop_clone.load(Ordering::Relaxed) { break; }
                if chunk_start.elapsed() >= Duration::from_secs(CHUNK_DURATION_SECS) { break; }
            }

            if stop_clone.load(Ordering::Relaxed) {
                break;
            }

            let elapsed = start_time.elapsed().as_secs();

            // Extract accumulated samples
            let chunk_samples = {
                let mut s = samples_ref.lock().unwrap();
                if s.len() < 8000 { // need at least 0.5s
                    continue;
                }
                std::mem::take(&mut *s)
            };

            // Transcribe chunk
            let _ = tx_clone.send(LongFormEvent::ChunkProcessing).await;
            tokio::task::yield_now().await;
            let text = transcribe_chunk(&ctx, &chunk_samples, &language, translate);
            match text {
                Ok(text) if !text.trim().is_empty() => {
                    let refined = refinement::apply(&text, &settings_clone);
                    let chunk = TranscriptChunk {
                        text: refined.clone(),
                        offset_secs: elapsed.saturating_sub(CHUNK_DURATION_SECS),
                        duration_secs: chunk_samples.len() as f32 / 16000.0,
                    };
                    session.chunks.push(chunk);
                    session.duration_secs = elapsed;

                    let _ = tx_clone.send(LongFormEvent::ChunkTranscribed {
                        text: refined,
                        elapsed_secs: elapsed,
                    }).await;
                }
                Err(e) => {
                    let _ = tx_clone.send(LongFormEvent::Error {
                        message: format!("Chunk transcription error: {}", e),
                    }).await;
                    // Continue recording despite errors
                }
                _ => {} // empty transcription, skip
            }
        }

        // Final chunk: transcribe remaining audio
        let final_samples = {
            let mut s = samples_ref.lock().unwrap();
            std::mem::take(&mut *s)
        };

        if final_samples.len() >= 8000 {
            if let Ok(text) = transcribe_chunk(&ctx, &final_samples, &language, translate) {
                if !text.trim().is_empty() {
                    let refined = refinement::apply(&text, &settings_clone);
                    let elapsed = start_time.elapsed().as_secs();
                    session.chunks.push(TranscriptChunk {
                        text: refined,
                        offset_secs: elapsed.saturating_sub(5),
                        duration_secs: final_samples.len() as f32 / 16000.0,
                    });
                }
            }
        }

        session.duration_secs = start_time.elapsed().as_secs();

        // Persist session
        if let Err(e) = save_session(&session) {
            tracing::error!("Failed to save long-form session: {}", e);
        }

        let _ = tx_clone.send(LongFormEvent::SessionComplete { session }).await;
    });

    Ok((rx, stop))
}

fn transcribe_chunk(
    ctx: &whisper_rs::WhisperContext,
    samples: &[f32],
    language: &str,
    translate: bool,
) -> Result<String> {
    use whisper_rs::{FullParams, SamplingStrategy};

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(language));
    params.set_translate(translate);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_no_context(true);

    let mut state = ctx.create_state().map_err(|e| anyhow::anyhow!("{}", e))?;
    state.full(params, samples).map_err(|e| anyhow::anyhow!("{}", e))?;

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

/// Save a completed session to disk
fn save_session(session: &TranscriptSession) -> Result<()> {
    let dir = Settings::data_dir().join("sessions");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("session-{}.json", session.started_at));
    let data = serde_json::to_string_pretty(session)?;
    std::fs::write(path, data)?;
    Ok(())
}

/// List saved sessions
pub fn list_sessions() -> Vec<TranscriptSession> {
    let dir = Settings::data_dir().join("sessions");
    if !dir.exists() {
        return Vec::new();
    }
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let data = std::fs::read_to_string(entry.path()).ok()?;
            serde_json::from_str(&data).ok()
        })
        .collect()
}

fn timestamp_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
