pub mod gui;

use anyhow::Result;
use std::path::PathBuf;

use crate::settings::Settings;
use crate::transcription;

const ONBOARDING_MARKER: &str = ".onboarding-complete";

fn marker_path() -> PathBuf {
    Settings::data_dir().join(ONBOARDING_MARKER)
}

pub fn is_complete() -> bool {
    marker_path().exists()
}

pub fn mark_complete() {
    let _ = std::fs::create_dir_all(Settings::data_dir());
    let _ = std::fs::write(marker_path(), "1");
}

/// Returns true if a graphical display is available
fn has_display() -> bool {
    std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok()
}

/// Run onboarding — launches GUI wizard as subprocess (avoids GTK3/GTK4 conflict in main process).
pub async fn run_auto() -> Result<String> {
    if has_display() {
        // Run GUI onboarding as a subprocess to avoid GTK version conflicts
        // (main process uses GTK3 for tray)
        let exe = std::env::current_exe().unwrap_or_else(|_| "frogscribe".into());
        let status = std::process::Command::new(&exe)
            .arg("--setup")
            .status();

        match status {
            Ok(s) if s.success() => {
                let settings = Settings::load()?;
                Ok(settings.transcription.model)
            }
            _ => {
                // Fallback to terminal if subprocess fails
                run().await
            }
        }
    } else {
        run().await
    }
}

/// Run the terminal-based onboarding flow. Returns the selected model name.
pub async fn run() -> Result<String> {
    println!();
    println!("╔══════════════════════════════════════════╗");
    println!("║         Welcome to FrogScribe for Linux        ║");
    println!("║   Voice dictation powered by on-device   ║");
    println!("║   AI. Press a hotkey, speak, and text    ║");
    println!("║   appears at your cursor.                ║");
    println!("╚══════════════════════════════════════════╝");
    println!();

    // Step 1: Check permissions
    println!("── Step 1/4: Permissions ──");
    check_permissions();
    println!();

    // Step 2: Select and download model
    println!("── Step 2/4: Choose a Model ──");
    println!();
    let models = transcription::available_models();
    for (i, (id, size, desc)) in models.iter().enumerate() {
        let downloaded = is_model_downloaded(id);
        let marker = if downloaded { " ✓" } else { "" };
        println!("  {}. {} ({}) — {}{}", i + 1, id, size, desc, marker);
    }
    println!();

    let model = select_model(&models).await?;
    println!();

    // Step 3: Hotkey info
    println!("── Step 3/4: How to Use ──");
    println!();
    let settings = Settings::load().unwrap_or_default();
    println!("  Hotkey:  {} (toggle recording)", settings.hotkey.toggle_key);
    println!("  Mode:    {:?}", settings.hotkey.activation_method);
    println!();
    println!("  Press the hotkey, speak, and text will be inserted at your cursor.");
    println!("  Edit settings in: ~/.config/frogscribe/settings.toml");
    println!();

    // Step 4: Practice
    println!("── Step 4/4: Practice ──");
    println!();
    // Build engine for practice
    let practice_settings = Settings {
        transcription: crate::settings::TranscriptionConfig {
            model: model.clone(),
            ..settings.transcription.clone()
        },
        ..settings.clone()
    };
    let engine = crate::transcription::Engine::new(&practice_settings).await?;
    let _ = crate::practice::run(&practice_settings, &engine).await;
    println!();
    println!();

    mark_complete();
    println!("✓ Setup complete! FrogScribe is now running.");
    println!();

    Ok(model)
}

fn check_permissions() {
    // Check input group
    let in_input_group = std::process::Command::new("groups")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("input"))
        .unwrap_or(false);

    if in_input_group {
        println!("  ✓ User is in 'input' group (hotkeys will work)");
    } else {
        println!("  ⚠ User is NOT in 'input' group. Global hotkeys may not work.");
        println!("    Fix: sudo usermod -aG input $USER && logout/login");
    }

    // Check ydotool/xdotool
    let has_ydotool = std::process::Command::new("which")
        .arg("ydotool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let has_xdotool = std::process::Command::new("which")
        .arg("xdotool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_ydotool || has_xdotool {
        let tool = if has_ydotool { "ydotool" } else { "xdotool" };
        println!("  ✓ Text insertion available ({})", tool);
    } else {
        println!("  ⚠ Neither ydotool nor xdotool found. Text insertion won't work.");
        println!("    Fix: sudo apt install ydotool");
    }

    // Check parec
    let has_parec = std::process::Command::new("which")
        .arg("parec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_parec {
        println!("  ✓ Audio capture available (parec)");
    } else {
        println!("  ⚠ parec not found. Audio recording won't work.");
        println!("    Fix: sudo apt install pulseaudio-utils");
    }
}

fn is_model_downloaded(model_id: &str) -> bool {
    Settings::models_dir()
        .join(format!("ggml-{}.bin", model_id))
        .exists()
}

async fn select_model(models: &[(&str, &str, &str)]) -> Result<String> {
    // Check if any model is already downloaded
    let downloaded: Vec<_> = models.iter().filter(|(id, _, _)| is_model_downloaded(id)).collect();
    if !downloaded.is_empty() {
        println!("  Already downloaded: {}", downloaded.iter().map(|(id, _, _)| *id).collect::<Vec<_>>().join(", "));
        print!("  Use existing model? [Y/n]: ");
        let answer = read_line();
        if answer.is_empty() || answer.starts_with('y') || answer.starts_with('Y') {
            return Ok(downloaded[0].0.to_string());
        }
    }

    print!("  Select model [1-{}] (default: 2 for 'base'): ", models.len());
    let input = read_line();
    let choice: usize = input.trim().parse().unwrap_or(2);
    let idx = choice.saturating_sub(1).min(models.len() - 1);
    let model_id = models[idx].0;

    if is_model_downloaded(model_id) {
        println!("  ✓ Model '{}' already downloaded.", model_id);
        return Ok(model_id.to_string());
    }

    println!("  Downloading '{}' ({})...", model_id, models[idx].1);
    download_with_progress(model_id).await?;
    println!("  ✓ Model '{}' downloaded successfully.", model_id);

    Ok(model_id.to_string())
}

async fn download_with_progress(model_name: &str) -> Result<()> {
    use futures::StreamExt;
    use std::io::Write;

    let models_dir = Settings::models_dir();
    std::fs::create_dir_all(&models_dir)?;

    let filename = format!("ggml-{}.bin", model_name);
    let dest = models_dir.join(&filename);
    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        filename
    );

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await?;
    let total = response.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&dest)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut last_pct = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;

        if total > 0 {
            let pct = (downloaded * 100) / total;
            if pct != last_pct {
                print!("\r  [{:>3}%] {:.1} / {:.1} MB", pct, downloaded as f64 / 1e6, total as f64 / 1e6);
                let _ = std::io::stdout().flush();
                last_pct = pct;
            }
        }
    }
    println!();

    Ok(())
}

fn read_line() -> String {
    use std::io::{self, Write};
    let _ = io::stdout().flush();
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    input.trim().to_string()
}
