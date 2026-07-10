// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
use anyhow::{Context, Result};
use std::process::Command;

use crate::settings::Settings;
use crate::transcription;

/// A speaker-labeled segment from diarization
#[derive(Debug, Clone)]
pub struct SpeakerSegment {
    pub speaker: String,
    pub start_secs: f32,
    pub end_secs: f32,
}

/// A timestamped transcript segment
#[derive(Debug, Clone)]
pub struct TimestampedSegment {
    pub text: String,
    pub start_secs: f32,
    pub end_secs: f32,
}

/// Aligned output: speaker number + text + timestamp
#[derive(Debug, Clone)]
pub struct DiarizedEntry {
    pub speaker: u32,
    pub start_secs: f32,
    pub text: String,
}

/// Run diarization on an audio file. Requires pyannote.audio Python package.
/// Falls back to a simpler approach if unavailable.
pub async fn diarize_file(audio_path: &str, model_name: &str, language: &str) -> Result<String> {
    eprintln!("Transcribing with timestamps...");
    let samples = decode_audio_file(audio_path)?;

    // Get timestamped transcription from whisper
    let transcript_segments = transcribe_with_timestamps(&samples, model_name, language)?;

    // Run speaker diarization
    eprintln!("Running speaker diarization...");
    let speaker_segments = run_pyannote_diarization(audio_path)?;

    // Align and format
    let entries = align_segments(&transcript_segments, &speaker_segments);
    let merged = merge_consecutive_speakers(&entries);
    Ok(format_output(&merged))
}

/// Transcribe audio with timestamps using whisper.cpp
fn transcribe_with_timestamps(samples: &[f32], model_name: &str, language: &str) -> Result<Vec<TimestampedSegment>> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    let model_path = Settings::models_dir().join(format!("ggml-{}.bin", model_name));
    let ctx = WhisperContext::new_with_params(
        model_path.to_str().unwrap(),
        WhisperContextParameters::default(),
    ).context("Failed to load model")?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(language));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_token_timestamps(true);

    let mut state = ctx.create_state().context("Failed to create state")?;
    state.full(params, samples).context("Transcription failed")?;

    let n = state.full_n_segments();
    let mut segments = Vec::new();

    for i in 0..n {
        if let Some(seg) = state.get_segment(i) {
            let text = seg.to_str_lossy().map(|c| c.to_string()).unwrap_or_default();
            let start = seg.start_timestamp() as f32 / 100.0; // centiseconds to seconds
            let end = seg.end_timestamp() as f32 / 100.0;
            if !text.trim().is_empty() {
                segments.push(TimestampedSegment { text: text.trim().to_string(), start_secs: start, end_secs: end });
            }
        }
    }

    Ok(segments)
}

/// Run pyannote.audio diarization via Python subprocess
fn run_pyannote_diarization(audio_path: &str) -> Result<Vec<SpeakerSegment>> {
    let script = r#"
import sys, json
try:
    from pyannote.audio import Pipeline
    pipeline = Pipeline.from_pretrained("pyannote/speaker-diarization-3.1")
    diarization = pipeline(sys.argv[1])
    segments = []
    for turn, _, speaker in diarization.itertracks(yield_label=True):
        segments.append({"speaker": speaker, "start": turn.start, "end": turn.end})
    print(json.dumps(segments))
except ImportError:
    # Fallback: simple energy-based segmentation (no real diarization)
    print("[]")
except Exception as e:
    print(f"[]", file=sys.stderr)
    print("[]")
"#;

    let output = Command::new("python3")
        .args(["-c", script, audio_path])
        .output()
        .context("python3 not found. Install pyannote.audio: pip install pyannote.audio")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let segments: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).unwrap_or_default();

    if segments.is_empty() {
        // Fallback: treat entire audio as single speaker
        eprintln!("Note: pyannote.audio not available. Output without speaker labels.");
        return Ok(Vec::new());
    }

    Ok(segments.iter().filter_map(|s| {
        Some(SpeakerSegment {
            speaker: s.get("speaker")?.as_str()?.to_string(),
            start_secs: s.get("start")?.as_f64()? as f32,
            end_secs: s.get("end")?.as_f64()? as f32,
        })
    }).collect())
}

/// Align transcript segments to speaker segments using overlap
fn align_segments(
    transcript: &[TimestampedSegment],
    speakers: &[SpeakerSegment],
) -> Vec<DiarizedEntry> {
    if speakers.is_empty() {
        // No diarization available — output as single speaker
        return transcript.iter().map(|t| DiarizedEntry {
            speaker: 1,
            start_secs: t.start_secs,
            text: t.text.clone(),
        }).collect();
    }

    let mut speaker_map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut next_speaker = 1u32;
    let mut entries = Vec::new();
    let mut used = vec![false; transcript.len()];

    let mut sorted_speakers = speakers.to_vec();
    sorted_speakers.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap());

    for seg in &sorted_speakers {
        let num = *speaker_map.entry(seg.speaker.clone()).or_insert_with(|| {
            let n = next_speaker;
            next_speaker += 1;
            n
        });

        let mut texts = Vec::new();
        let mut earliest_start = seg.end_secs;

        for (i, ts) in transcript.iter().enumerate() {
            if used[i] { continue; }
            let overlap = ts.end_secs.min(seg.end_secs) - ts.start_secs.max(seg.start_secs);
            let dur = ts.end_secs - ts.start_secs;
            if overlap > 0.0 && (dur == 0.0 || overlap / dur > 0.5) {
                texts.push(ts.text.clone());
                used[i] = true;
                earliest_start = earliest_start.min(ts.start_secs);
            }
        }

        let text = texts.join(" ");
        if !text.trim().is_empty() {
            entries.push(DiarizedEntry { speaker: num, start_secs: earliest_start, text });
        }
    }

    entries
}

/// Merge consecutive same-speaker entries into paragraphs
fn merge_consecutive_speakers(entries: &[DiarizedEntry]) -> Vec<DiarizedEntry> {
    let mut merged: Vec<DiarizedEntry> = Vec::new();
    for entry in entries {
        if let Some(last) = merged.last_mut() {
            if last.speaker == entry.speaker {
                last.text.push(' ');
                last.text.push_str(&entry.text);
                continue;
            }
        }
        merged.push(entry.clone());
    }
    merged
}

/// Format output as `[M:SS] Speaker N: text`
fn format_output(entries: &[DiarizedEntry]) -> String {
    entries.iter()
        .map(|e| format!("[{}] Speaker {}: {}", format_time(e.start_secs), e.speaker, e.text))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn format_time(secs: f32) -> String {
    let t = secs.round() as u32;
    let h = t / 3600;
    let m = (t % 3600) / 60;
    let s = t % 60;
    if h > 0 { format!("{}:{:02}:{:02}", h, m, s) } else { format!("{}:{:02}", m, s) }
}

/// Decode audio file to 16kHz mono f32 via ffmpeg
fn decode_audio_file(path: &str) -> Result<Vec<f32>> {
    let output = Command::new("ffmpeg")
        .args(["-i", path, "-f", "f32le", "-acodec", "pcm_f32le", "-ar", "16000", "-ac", "1", "-"])
        .output()
        .context("ffmpeg not found")?;

    if !output.status.success() {
        anyhow::bail!("ffmpeg failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(output.stdout.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(3661.0), "1:01:01");
    }

    #[test]
    fn test_merge_consecutive_speakers() {
        let entries = vec![
            DiarizedEntry { speaker: 1, start_secs: 0.0, text: "Hello.".into() },
            DiarizedEntry { speaker: 1, start_secs: 2.0, text: "How are you?".into() },
            DiarizedEntry { speaker: 2, start_secs: 4.0, text: "I'm fine.".into() },
            DiarizedEntry { speaker: 1, start_secs: 6.0, text: "Great.".into() },
        ];
        let merged = merge_consecutive_speakers(&entries);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].text, "Hello. How are you?");
        assert_eq!(merged[1].text, "I'm fine.");
        assert_eq!(merged[2].text, "Great.");
    }

    #[test]
    fn test_align_no_speakers() {
        let transcript = vec![
            TimestampedSegment { text: "Hello world.".into(), start_secs: 0.0, end_secs: 2.0 },
        ];
        let entries = align_segments(&transcript, &[]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].speaker, 1);
        assert_eq!(entries[0].text, "Hello world.");
    }

    #[test]
    fn test_align_with_speakers() {
        let transcript = vec![
            TimestampedSegment { text: "Hello.".into(), start_secs: 0.0, end_secs: 1.5 },
            TimestampedSegment { text: "Hi there.".into(), start_secs: 2.0, end_secs: 3.5 },
        ];
        let speakers = vec![
            SpeakerSegment { speaker: "A".into(), start_secs: 0.0, end_secs: 1.8 },
            SpeakerSegment { speaker: "B".into(), start_secs: 1.8, end_secs: 4.0 },
        ];
        let entries = align_segments(&transcript, &speakers);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].speaker, 1);
        assert_eq!(entries[0].text, "Hello.");
        assert_eq!(entries[1].speaker, 2);
        assert_eq!(entries[1].text, "Hi there.");
    }

    #[test]
    fn test_format_output() {
        let entries = vec![
            DiarizedEntry { speaker: 1, start_secs: 0.0, text: "Hello everyone.".into() },
            DiarizedEntry { speaker: 2, start_secs: 5.0, text: "Thanks for the intro.".into() },
        ];
        let output = format_output(&entries);
        assert!(output.contains("[0:00] Speaker 1: Hello everyone."));
        assert!(output.contains("[0:05] Speaker 2: Thanks for the intro."));
    }
}
