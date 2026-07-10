#![allow(unused_imports)]

mod audio;
mod auto_transcription;
mod autostart;
mod cli;
mod dbus;
mod diarization;
mod ear_protection;
mod escape_cancel;
mod history;
mod history_window;
mod hotkey;
mod indicator;
mod insertion;
mod known_terms;
mod languages;
mod live_preview;
mod longform;
mod model_doctor;
mod models;
mod notifications;
mod onboarding;
mod practice;
mod refinement;
mod settings;
mod smart_refinement;
mod streaming;
mod transcription;
mod transcript_window;
mod ui;
mod vocabulary;
mod window_picker;

use anyhow::Result;
use clap::Parser;
use gtk3::prelude::*;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "frogscribe", about = "Voice dictation for Linux GNOME")]
pub(crate) struct Args {
    /// Transcribe an audio file instead of running the GUI
    #[arg(long)]
    transcribe: Option<String>,

    /// Download a model (tiny, base, small, medium, large-v3)
    #[arg(long)]
    download_model: Option<String>,

    /// Run the GUI setup wizard
    #[arg(long)]
    setup: bool,

    /// Open the settings window
    #[arg(long)]
    settings: bool,

    /// Open the transcription history viewer
    #[arg(long)]
    history: bool,

    /// Enable speaker diarization (requires pyannote.audio)
    #[arg(long)]
    diarize: bool,

    /// Whisper model to use (tiny, base, small, medium, large-v3)
    #[arg(long, default_value = "base")]
    model: String,

    /// Language for transcription
    #[arg(long, default_value = "en")]
    language: String,

    /// Translate to English
    #[arg(long)]
    translate: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("frogscribe=info".parse()?))
        .init();

    let args = Args::parse();

    // --setup runs GTK4 onboarding wizard (must NOT init GTK3)
    if args.setup {
        onboarding::gui::run_gui();
        return Ok(());
    }

    // --settings opens the settings window
    if args.settings {
        ui::show_settings();
        return Ok(());
    }

    // --history opens the history viewer
    if args.history {
        gtk3::init().expect("Failed to initialize GTK3");
        history_window::show();
        gtk3::main();
        return Ok(());
    }

    // All other modes use GTK3 (for tray). Init before anything touches GDK.
    gtk3::init().expect("Failed to initialize GTK3");

    if let Some(audio_path) = args.transcribe {
        let rt = tokio::runtime::Runtime::new()?;
        if args.diarize {
            rt.block_on(async {
                let output = diarization::diarize_file(&audio_path, &args.model, &args.language).await?;
                println!("{}", output);
                Ok::<(), anyhow::Error>(())
            })?;
        } else {
            rt.block_on(cli::transcribe_file(&audio_path, &args.model, &args.language, args.translate))?;
        }
        return Ok(());
    }

    if let Some(model_name) = args.download_model {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            println!("Downloading model '{}'...", model_name);
            transcription::download_model(&model_name).await?;
            println!("✓ Model '{}' downloaded.", model_name);
            Ok::<(), anyhow::Error>(())
        })?;
        return Ok(());
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // Create event channel shared between tray (main thread) and daemon (bg thread)
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<AppEvent>(64);

    // Run async daemon on background thread
    let daemon_tx = event_tx.clone();
    let daemon_rx = event_rx;
    std::thread::spawn(move || {
        rt.block_on(async {
            if let Err(e) = run_daemon(daemon_tx, daemon_rx).await {
                tracing::error!("Daemon error: {}", e);
            }
        });
    });

    // Run GTK main loop on main thread (needed for indicator overlays and dialogs)
    // The GNOME Shell extension handles the panel indicator/menu.
    gtk3::main();

    Ok(())
}

async fn run_daemon(event_tx: tokio::sync::mpsc::Sender<AppEvent>, mut event_rx: tokio::sync::mpsc::Receiver<AppEvent>) -> Result<()> {
    tracing::info!("Starting FrogScribe daemon");

    // Run onboarding on first launch
    if !onboarding::is_complete() {
        let model = onboarding::run_auto().await?;
        let mut settings = settings::Settings::load()?;
        settings.transcription.model = model;
        settings.save()?;
    }

    let mut settings = settings::Settings::load()?;

    // Check ydotool accessibility at startup
    if let Err(msg) = insertion::check_ydotool() {
        tracing::warn!("ydotool check failed: {}", msg);
        notifications::notify_error(&msg);
    }


    // Check model health at startup
    if model_doctor::check_and_repair(&settings.transcription.model) {
        notifications::notify_error("Model was corrupted and removed. Please re-download via Settings.");
    }

    // Start D-Bus service
    let auto_paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let dbus_conn = dbus::start_service(event_tx.clone(), auto_paused.clone()).await?;

    // Start hotkey monitor
    let hotkey_tx = event_tx.clone();
    let hotkey_config = settings.hotkey.clone();
    let mut hotkey_handle = tokio::spawn(async move {
        if let Err(e) = hotkey::monitor(hotkey_tx, &hotkey_config).await {
            tracing::error!("Hotkey monitor error: {}", e);
        }
    });

    // Start auto-transcription monitor
    let auto_tx = event_tx.clone();
    let auto_config = settings.auto_transcription.clone();
    tokio::spawn(async move {
        if let Err(e) = auto_transcription::monitor(auto_tx, auto_config).await {
            tracing::error!("Auto-transcription monitor error: {}", e);
        }
    });

    // Initialize subsystems
    let mut recorder = audio::Recorder::new(&settings)?;
    let engine = transcription::Engine::new(&settings).await?;
    let mut history_store = history::HistoryStore::new().unwrap_or_else(|e| {
        tracing::warn!("Failed to load history: {}", e);
        history::HistoryStore::new().unwrap()
    });
    let mut longform_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>> = None;
    let mut escape_handle: Option<std::sync::Arc<std::sync::atomic::AtomicBool>> = None;
    let mut streaming_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>> = None;
    let mut auto_trigger_app: String = String::new();

    loop {
        match event_rx.recv().await {
            Some(AppEvent::ToggleRecording) => {
                if recorder.is_recording() {
                    if let Some(h) = escape_handle.take() { escape_cancel::stop_monitoring(&h); }
                    // Pill hide handled by extension via D-Bus idle status
                    // Stop streaming and get final text
                    let streaming_text = stop_streaming(&mut streaming_stop).await;
                    ear_protection::deactivate();
                    dbus::emit_status(&dbus_conn, "idle").await;
                    handle_stop_recording(
                                            &mut recorder, &engine, &settings, &mut history_store, streaming_text, "frogscribe",
                                        ).await;
                } else {
                    settings = settings::Settings::load().unwrap_or(settings);
                    tracing::info!("Recording started");
                    ear_protection::activate(settings.audio.ear_protection.clone());
                    recorder.start()?;
                    escape_handle = Some(escape_cancel::start_monitoring(event_tx.clone()));
                    // Pill indicator now handled by GNOME Shell extension
                    if settings.transcription.streaming {
                        let model_path = settings::Settings::models_dir()
                            .join(format!("ggml-{}.bin", settings.transcription.model));
                        let save_source = if auto_trigger_app.is_empty() {
                            "frogscribe (streaming)".to_string()
                        } else {
                            format!("auto:{} (streaming)", auto_trigger_app)
                        };
                        if let Ok((rx, stop)) = streaming::start_streaming(
                            recorder.samples_ref(),
                            model_path.to_str().unwrap_or(""),
                            &settings.transcription.language,
                            settings.transcription.translate_to_english,
                        ) {
                            streaming_stop = Some(stop);
                            let live_text = live_preview::open_tab(&save_source);
                            let mut srx = rx;
                            tokio::spawn(async move {
                                while let Some(event) = srx.recv().await {
                                    if let streaming::StreamingEvent::Partial { confirmed, unconfirmed } = event {
                                                                            let full = if confirmed.is_empty() { unconfirmed } else { format!("{} {}", confirmed, unconfirmed) };
                                                                            *live_text.lock().unwrap() = full;
                                                                        }
                                }
                            });
                        }
                    }
                    dbus::emit_status(&dbus_conn, &format!("recording:{}:{}:{}", settings.appearance.accent_color, if settings.appearance.topbar_enabled { "1" } else { "0" }, if settings.appearance.pill_enabled { "1" } else { "0" })).await;
                }
            }
            Some(AppEvent::StartRecording) => {
                if !recorder.is_recording() {
                    settings = settings::Settings::load().unwrap_or(settings);
                    tracing::info!("Recording started (hold-to-talk)");
                    ear_protection::activate(settings.audio.ear_protection.clone());
                    recorder.start()?;
                    escape_handle = Some(escape_cancel::start_monitoring(event_tx.clone()));
                    // Pill indicator now handled by GNOME Shell extension
                    dbus::emit_status(&dbus_conn, &format!("recording:{}:{}:{}", settings.appearance.accent_color, if settings.appearance.topbar_enabled { "1" } else { "0" }, if settings.appearance.pill_enabled { "1" } else { "0" })).await;
                }
            }
            Some(AppEvent::StopRecording) => {
                if recorder.is_recording() {
                    if let Some(h) = escape_handle.take() { escape_cancel::stop_monitoring(&h); }
                    // Pill hide handled by extension via D-Bus idle status
                    let streaming_text = stop_streaming(&mut streaming_stop).await;
                    ear_protection::deactivate();
                    dbus::emit_status(&dbus_conn, "idle").await;
                    handle_stop_recording(
                                            &mut recorder, &engine, &settings, &mut history_store, streaming_text, "frogscribe",
                                        ).await;
                }
            }
            Some(AppEvent::CancelRecording) => {
                if recorder.is_recording() {
                    tracing::info!("Recording cancelled (Escape)");
                    if let Some(h) = escape_handle.take() { escape_cancel::stop_monitoring(&h); }
                    // Pill hide handled by extension via D-Bus idle status
                    ear_protection::deactivate();
                    let _ = recorder.stop(); // discard audio
                    dbus::emit_status(&dbus_conn, "idle").await;
                    notifications::notify_transcription("Recording cancelled");
                }
            }
            Some(AppEvent::StartLongForm) => {
                tracing::info!("Long-form dictation started");
                settings = settings::Settings::load().unwrap_or(settings);
                dbus::emit_status(&dbus_conn, &format!("recording:{}:{}:{}", settings.appearance.accent_color, if settings.appearance.topbar_enabled { "1" } else { "0" }, if settings.appearance.pill_enabled { "1" } else { "0" })).await;
                match longform::start_session(&mut recorder, &engine, &settings).await {
                    Ok((mut rx, stop)) => {
                        longform_stop = Some(stop);

                        // Create transcript window state
                        let tw_state = std::sync::Arc::new(transcript_window::TranscriptWindowState::new());
                        let tw_state_events = tw_state.clone();

                        // Open transcript window
                        let stop_clone = longform_stop.clone();
                        let event_tx_clone = event_tx.clone();
                        transcript_window::show(tw_state.clone(), transcript_window::TranscriptCallbacks {
                            on_stop: std::sync::Arc::new(move || {
                                if let Some(s) = &stop_clone { s.store(true, std::sync::atomic::Ordering::Relaxed); }
                            }),
                            on_copy_all: std::sync::Arc::new(|| {}),
                            on_start_new: std::sync::Arc::new({
                                let tx = event_tx_clone.clone();
                                move || { let _ = tx.blocking_send(AppEvent::StopLongForm); let _ = tx.blocking_send(AppEvent::StartLongForm); }
                            }),
                            on_done: std::sync::Arc::new({
                                let tx = event_tx_clone;
                                move || { let _ = tx.blocking_send(AppEvent::StopLongForm); }
                            }),
                        });

                        // Spawn task to consume long-form events and feed transcript window
                        tokio::spawn(async move {
                            while let Some(event) = rx.recv().await {
                                match event {
                                    longform::LongFormEvent::ChunkProcessing => {
                                        tw_state_events.is_processing.store(true, std::sync::atomic::Ordering::Relaxed);
                                    }
                                    longform::LongFormEvent::ChunkTranscribed { text, elapsed_secs } => {
                                        tracing::info!("[{}s] {}", elapsed_secs, text);
                                        tw_state_events.append_text(&text);
                                        // Brief delay before clearing processing indicator so UI can show it
                                        let proc_flag = tw_state_events.is_processing.clone();
                                        tokio::spawn(async move {
                                            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                                            proc_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                                        });
                                    }
                                    longform::LongFormEvent::SessionComplete { session } => {
                                        tracing::info!("Long-form complete: {} chunks, {}s", session.chunks.len(), session.duration_secs);
                                        tw_state_events.is_active.store(false, std::sync::atomic::Ordering::Relaxed);
                                        notifications::notify_transcription(&format!("Session complete ({} chunks)", session.chunks.len()));
                                    }
                                    longform::LongFormEvent::Error { message } => {
                                        tracing::error!("Long-form error: {}", message);
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("Failed to start long-form: {}", e);
                        notifications::notify_error(&format!("Long-form failed: {}", e));
                    }
                }
            }
            Some(AppEvent::StopLongForm) => {
                tracing::info!("Long-form dictation stopped");
                if let Some(stop) = longform_stop.take() {
                    stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                let _ = recorder.stop();
                dbus::emit_status(&dbus_conn, "idle").await;
            }
            Some(AppEvent::StartAutoTranscription(app_name)) => {
                if !recorder.is_recording() && !auto_paused.load(std::sync::atomic::Ordering::Relaxed) {
                    auto_trigger_app = app_name;
                    settings = settings::Settings::load().unwrap_or(settings);
                    tracing::info!("Auto-transcription: recording started");
                    recorder.start()?;
                    dbus::emit_status(&dbus_conn, &format!("recording:{}:{}:{}", settings.appearance.accent_color, if settings.appearance.topbar_enabled { "1" } else { "0" }, if settings.appearance.pill_enabled { "1" } else { "0" })).await;
                    notifications::notify_transcription("🎙 Auto-transcription started");
                    // Start streaming + live preview if enabled
                    if settings.transcription.streaming {
                        let model_path = settings::Settings::models_dir()
                            .join(format!("ggml-{}.bin", settings.transcription.model));
                        let save_source = format!("auto:{} (streaming)", auto_trigger_app);
                        if let Ok((rx, stop)) = streaming::start_streaming(
                            recorder.samples_ref(),
                            model_path.to_str().unwrap_or(""),
                            &settings.transcription.language,
                            settings.transcription.translate_to_english,
                        ) {
                            streaming_stop = Some(stop);
                            let live_text = live_preview::open_tab(&save_source);
                            let mut srx = rx;
                            tokio::spawn(async move {
                                while let Some(event) = srx.recv().await {
                                    if let streaming::StreamingEvent::Partial { confirmed, unconfirmed } = event {
                                                                            let full = if confirmed.is_empty() { unconfirmed } else { format!("{} {}", confirmed, unconfirmed) };
                                                                            *live_text.lock().unwrap() = full;
                                                                        }
                                }
                            });
                        }
                    }
                    // If VAD enabled, spawn a task to monitor silence
                    if settings.auto_transcription.vad_enabled {
                        let silence_secs = settings.auto_transcription.silence_seconds;
                        let samples_ref = recorder.samples_ref();
                        let vad_tx = event_tx.clone();
                        tokio::spawn(async move {
                            let mut silent_since: Option<tokio::time::Instant> = None;
                            loop {
                                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                let has_voice = {
                                    let s = samples_ref.lock().unwrap();
                                    let recent = if s.len() > 8000 { &s[s.len()-8000..] } else { &s };
                                    auto_transcription::detect_voice(recent, auto_transcription::VAD_ENERGY_THRESHOLD)
                                };
                                if has_voice {
                                    silent_since = None;
                                } else {
                                    let since = silent_since.get_or_insert_with(tokio::time::Instant::now);
                                    if since.elapsed() >= std::time::Duration::from_secs(silence_secs as u64) {
                                        tracing::info!("Auto-transcription: silence timeout ({}s), stopping", silence_secs);
                                        let _ = vad_tx.send(AppEvent::StopAutoTranscription).await;
                                        break;
                                    }
                                }
                            }
                        });
                    }
                }
            }
            Some(AppEvent::StopAutoTranscription) => {
                if recorder.is_recording() {
                    tracing::info!("Auto-transcription: recording stopped");
                    dbus::emit_status(&dbus_conn, "idle").await;
                    let streaming_text = stop_streaming(&mut streaming_stop).await;
                    let source = format!("auto:{}", auto_trigger_app);
                    handle_stop_recording(
                                            &mut recorder, &engine, &settings, &mut history_store, streaming_text, &source,
                                        ).await;
                    // Re-arm: if mic is still in use by another app, restart after cooldown
                    if settings.auto_transcription.enabled {
                        let rearm_tx = event_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            // Check if external mic usage is still active
                            let has_external = tokio::task::spawn_blocking(|| {
                                auto_transcription::has_external_mic_usage()
                            }).await.unwrap_or(false);
                            if has_external {
                                tracing::info!("Auto-transcription: mic still active, re-arming");
                                let _ = rearm_tx.send(AppEvent::StartAutoTranscription("resumed".to_string())).await;
                            }
                        });
                    }
                }
            }
            Some(AppEvent::ReloadHotkey) => {
                tracing::info!("Reloading hotkey config");
                hotkey_handle.abort();
                let new_settings = settings::Settings::load().unwrap_or(settings.clone());
                settings.hotkey = new_settings.hotkey.clone();
                let hotkey_tx = event_tx.clone();
                let hotkey_config = settings.hotkey.clone();
                hotkey_handle = tokio::spawn(async move {
                    if let Err(e) = hotkey::monitor(hotkey_tx, &hotkey_config).await {
                        tracing::error!("Hotkey monitor error: {}", e);
                    }
                });
            }
            Some(AppEvent::Quit) => break,
            None => break,
        }
    }

    Ok(())
}

struct TranscriptionContext {
    duration_secs: f32,
    source: String, // "frogscribe" or "auto:<app_name>"
}

fn auto_save_transcription(text: &str, settings: &settings::Settings, ctx: &TranscriptionContext) {
    let dir = dirs::home_dir().unwrap_or_default().join(".frogscribe/transcriptions");
    if std::fs::create_dir_all(&dir).is_err() { return; }
    let dt = glib::DateTime::now_local()
        .and_then(|d| d.format("%Y%m%d-%H%M%S"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let path = dir.join(format!("transcript-{}.txt", dt));

    let content = if settings.general.context_header {
        let now_human = glib::DateTime::now_local()
            .and_then(|d| d.format("%Y-%m-%d %H:%M:%S"))
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let dur_min = ctx.duration_secs / 60.0;
        format!(
            "---\nTimestamp: {}\nDuration: {:.1}s ({:.1} min)\nSource: {}\n---\n\n{}",
            now_human, ctx.duration_secs, dur_min, ctx.source, text
        )
    } else {
        text.to_string()
    };

    if let Err(e) = std::fs::write(&path, &content) {
        tracing::error!("Auto-save failed: {}", e);
    } else {
        tracing::info!("Auto-saved transcription to {:?}", path);
    }
}

async fn stop_streaming(
    stop: &mut Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Option<String> {
    if let Some(s) = stop.take() {
        s.store(true, std::sync::atomic::Ordering::Relaxed);
        // Give streaming thread time to finish final transcription
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    None // Streaming result used directly; fall back to batch transcription
}

async fn handle_stop_recording(
    recorder: &mut audio::Recorder,
    engine: &transcription::Engine,
    settings: &settings::Settings,
    history_store: &mut history::HistoryStore,
    streaming_text: Option<String>,
    source: &str,
) {
    tracing::info!("Recording stopped");
    match recorder.stop() {
        Ok(Some(audio_data)) => {
            // Use streaming result if available, otherwise transcribe from scratch
            let transcribe_result = if let Some(text) = streaming_text {
                Ok(text)
            } else {
                engine.transcribe(&audio_data).await
            };
            match transcribe_result {
                Ok(text) => {
                    let refined = apply_refinement(&text, settings).await;
                    if settings.general.history_enabled {
                        let _ = history_store.add(
                            &refined, audio_data.duration_secs,
                            &settings.transcription.model, &settings.transcription.language,
                        );
                    }
                    notifications::notify_transcription(&refined);
                    if settings.general.auto_save_transcriptions {
                        auto_save_transcription(&refined, settings, &TranscriptionContext {
                            duration_secs: audio_data.duration_secs,
                            source: source.to_string(),
                        });
                    }
                    if settings.general.auto_paste {
                        if settings.general.use_window_picker {
                            // Show picker, activate chosen window, then paste
                            let text_for_picker = refined.clone();
                            let auto_submit = settings.general.auto_submit;
                            tokio::task::spawn_blocking(move || {
                                if let Some(win_id) = window_picker::show_picker(&text_for_picker) {
                                    window_picker::activate_window(&win_id);
                                    std::thread::sleep(std::time::Duration::from_millis(300));
                                    let rt = tokio::runtime::Handle::current();
                                    rt.block_on(async {
                                        if let Err(e) = insertion::insert_text(&text_for_picker).await {
                                            tracing::error!("Insertion error: {}", e);
                                        }
                                        if auto_submit {
                                            let _ = insertion::press_enter().await;
                                        }
                                    });
                                }
                            });
                        } else {
                            if let Err(e) = insertion::insert_text(&refined).await {
                                tracing::error!("Insertion error: {}", e);
                                notifications::notify_error(&format!("Insertion failed: {}", e));
                            }
                            if settings.general.auto_submit {
                                let _ = insertion::press_enter().await;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Transcription error: {}", e);
                    notifications::notify_error(&format!("Transcription failed: {}", e));
                }
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Recorder stop error: {}", e);
            notifications::notify_error(&format!("Recording error: {}", e));
        }
    }
}

/// Apply refinement based on configured mode (Local rules or Smart AI via Bedrock Claude)
async fn apply_refinement(text: &str, settings: &settings::Settings) -> String {
    if !settings.refinement.enabled || text.is_empty() {
        return text.to_string();
    }
    let refined = match settings.refinement.mode {
        settings::RefinementMode::Local => refinement::apply(text, settings),
        settings::RefinementMode::Smart => {
            smart_refinement::refine(text, &settings.refinement.custom_vocabulary).await
        }
    };
    // Apply known terms dictionary as post-pass
    known_terms::apply(&refined)
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    ToggleRecording,
    StartRecording,
    StopRecording,
    CancelRecording,
    StartLongForm,
    StopLongForm,
    StartAutoTranscription(String),
    StopAutoTranscription,
    ReloadHotkey,
    Quit,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
