#![allow(dead_code)]
use anyhow::{Context, Result};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tokio::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::audio::AudioData;
use crate::settings::Settings;

/// Streaming transcription events
#[derive(Debug, Clone)]
pub enum StreamingEvent {
    Partial { confirmed: String, unconfirmed: String },
    Finished { text: String },
    Error { message: String },
}

/// Run streaming transcription: periodically transcribe accumulated audio and emit partial results.
/// Returns a channel of StreamingEvents and a stop handle.
pub fn start_streaming(
    samples: Arc<Mutex<Vec<f32>>>,
    model_path: &str,
    language: &str,
    translate: bool,
) -> Result<(mpsc::Receiver<StreamingEvent>, Arc<AtomicBool>)> {
    let (tx, rx) = mpsc::channel(32);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();
    let model_path = model_path.to_string();
    let language = language.to_string();

    std::thread::spawn(move || {
        let params = WhisperContextParameters::default();
        let ctx = match WhisperContext::new_with_params(&model_path, params) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.blocking_send(StreamingEvent::Error {
                    message: format!("Failed to load model: {}", e),
                });
                return;
            }
        };

        let mut last_text = String::new();
        let mut all_confirmed = String::new();
        let mut last_sample_count: usize = 0;
        const WINDOW_SAMPLES: usize = 16000 * 15; // last 15 seconds

        while !stop_clone.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(500));

            let current_samples = {
                let s = samples.lock().unwrap();
                // Skip if no new audio since last transcription
                if s.len() == last_sample_count {
                    continue;
                }
                if s.len() < 16000 { // need at least 1 second
                    continue;
                }
                last_sample_count = s.len();
                // Only transcribe the last 15 seconds to avoid repetition
                if s.len() > WINDOW_SAMPLES {
                    s[s.len() - WINDOW_SAMPLES..].to_vec()
                } else {
                    s.clone()
                }
            };

            let mut fp = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            fp.set_language(Some(&language));
            fp.set_translate(translate);
            fp.set_print_special(false);
            fp.set_print_progress(false);
            fp.set_print_realtime(false);
            fp.set_print_timestamps(false);
            fp.set_suppress_blank(true);
            fp.set_no_context(true);
            fp.set_single_segment(true);

            let mut state = match ctx.create_state() {
                Ok(s) => s,
                Err(_) => continue,
            };

            if state.full(fp, &current_samples).is_err() {
                continue;
            }

            let n = state.full_n_segments();
            let mut text = String::new();
            for i in 0..n {
                if let Some(seg) = state.get_segment(i) {
                    if let Ok(s) = seg.to_str() {
                        text.push_str(s);
                    }
                }
            }

            let text = text.trim().to_string();
            if text != last_text && !text.is_empty() {
                // Detect when the window has moved: if new text doesn't start with
                // the beginning of last_text, the old text is confirmed
                if !last_text.is_empty() && !text.starts_with(&last_text[..last_text.len().min(20)]) {
                    if !all_confirmed.is_empty() { all_confirmed.push(' '); }
                    all_confirmed.push_str(&last_text);
                }
                let _ = tx.blocking_send(StreamingEvent::Partial {
                    confirmed: all_confirmed.clone(),
                    unconfirmed: text.clone(),
                });
                last_text = text;
            }
        }

        // Final transcription
        let final_samples = samples.lock().unwrap().clone();
        if !final_samples.is_empty() {
            let mut fp = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            fp.set_language(Some(&language));
            fp.set_translate(translate);
            fp.set_print_special(false);
            fp.set_print_progress(false);
            fp.set_suppress_blank(true);

            let mut state = match ctx.create_state() {
                Ok(s) => s,
                Err(_) => return,
            };

            if state.full(fp, &final_samples).is_ok() {
                let n = state.full_n_segments();
                let mut text = String::new();
                for i in 0..n {
                    if let Some(seg) = state.get_segment(i) {
                        if let Ok(s) = seg.to_str() {
                            text.push_str(s);
                        }
                    }
                }
                let _ = tx.blocking_send(StreamingEvent::Finished {
                    text: text.trim().to_string(),
                });
            }
        }
    });

    Ok((rx, stop))
}
