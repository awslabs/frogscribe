// SPDX-License-Identifier: Apache-2.0
//! Practice mode: interactive dictation test during onboarding.
//! Records audio, transcribes, and displays result to confirm setup works.

use anyhow::Result;
use std::io::{self, Write};

use crate::audio::Recorder;
use crate::refinement;
use crate::settings::Settings;
use crate::transcription::Engine;

/// Run an interactive practice session. Returns true if user completed practice.
pub async fn run(settings: &Settings, engine: &Engine) -> Result<bool> {
    println!("── Practice Mode ──");
    println!();
    println!("  Let's test that everything works! Press Enter to start recording,");
    println!("  say a few words, then press Enter again to stop.");
    println!("  (Type 'skip' to skip this step)");
    println!();

    loop {
        print!("  Press Enter to record (or 'skip'): ");
        io::stdout().flush()?;
        let input = read_line();

        if input.to_lowercase().starts_with('s') {
            println!("  Skipped practice mode.");
            return Ok(false);
        }

        // Start recording
        let mut recorder = Recorder::new(settings)?;
        println!("  🎙 Recording... speak now! Press Enter when done.");
        recorder.start()?;

        // Wait for Enter
        let _ = read_line();

        // Stop and transcribe
        print!("  ⏳ Transcribing...");
        io::stdout().flush()?;

        match recorder.stop()? {
            Some(audio_data) => {
                if audio_data.duration_secs < 0.5 {
                    println!("\r  ⚠ Recording too short. Try again (speak for at least 1 second).");
                    continue;
                }

                match engine.transcribe(&audio_data).await {
                    Ok(text) => {
                        let refined = refinement::apply(&text, settings);
                        if refined.trim().is_empty() {
                            println!("\r  ⚠ No speech detected. Make sure your microphone is working.");
                            println!("    Try again or type 'skip' to continue.");
                            continue;
                        }
                        println!("\r  ✓ Transcribed:                    ");
                        println!();
                        println!("    \"{}\"", refined);
                        println!();
                        println!("  🎉 It works! FrogScribe is ready to use.");
                        return Ok(true);
                    }
                    Err(e) => {
                        println!("\r  ✗ Transcription error: {}          ", e);
                        println!("    Make sure a model is downloaded. Try again or 'skip'.");
                        continue;
                    }
                }
            }
            None => {
                println!("\r  ⚠ No audio captured. Check your microphone.");
                continue;
            }
        }
    }
}

fn read_line() -> String {
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    input.trim().to_string()
}
