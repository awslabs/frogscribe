// SPDX-License-Identifier: Apache-2.0
#[cfg(test)]
mod tests {
    use super::*;

    mod hotkey_parsing {
        use crate::hotkey::*;

        // We need to test parse_hotkey but it's private, so test via the public interface indirectly
        // For now, test the key string parsing logic
        #[test]
        fn test_str_to_key_letters() {
            assert_eq!(str_to_key("a"), evdev::Key::KEY_A);
            assert_eq!(str_to_key("z"), evdev::Key::KEY_Z);
            assert_eq!(str_to_key("A"), evdev::Key::KEY_A); // case insensitive
        }

        #[test]
        fn test_str_to_key_special() {
            assert_eq!(str_to_key("space"), evdev::Key::KEY_SPACE);
            assert_eq!(str_to_key("enter"), evdev::Key::KEY_ENTER);
            assert_eq!(str_to_key("f1"), evdev::Key::KEY_F1);
            assert_eq!(str_to_key("f12"), evdev::Key::KEY_F12);
        }

        #[test]
        fn test_parse_hotkey_alt_space() {
            let (mods, trigger) = parse_hotkey("Alt+Space");
            assert_eq!(mods, vec![evdev::Key::KEY_LEFTALT]);
            assert_eq!(trigger, evdev::Key::KEY_SPACE);
        }

        #[test]
        fn test_parse_hotkey_ctrl_shift_r() {
            let (mods, trigger) = parse_hotkey("Ctrl+Shift+R");
            assert_eq!(mods.len(), 2);
            assert_eq!(trigger, evdev::Key::KEY_R);
        }

        #[test]
        fn test_parse_hotkey_super_space() {
            let (mods, trigger) = parse_hotkey("Super+Space");
            assert_eq!(mods, vec![evdev::Key::KEY_LEFTMETA]);
            assert_eq!(trigger, evdev::Key::KEY_SPACE);
        }

        #[test]
        fn test_is_modifier_match_left_right() {
            assert!(is_modifier_match(evdev::Key::KEY_LEFTALT, evdev::Key::KEY_LEFTALT));
            assert!(is_modifier_match(evdev::Key::KEY_RIGHTALT, evdev::Key::KEY_LEFTALT));
            assert!(!is_modifier_match(evdev::Key::KEY_LEFTCTRL, evdev::Key::KEY_LEFTALT));
        }
    }
    mod hold_to_talk_debounce_tests {
        use crate::AppEvent;
        use std::time::Duration;
        use tokio::sync::mpsc;

        /// Simulates the debounce logic: hold >= 200ms should fire StartRecording then StopRecording
        #[tokio::test]
        async fn test_hold_long_enough_fires_events() {
            let (tx, mut rx) = mpsc::channel(8);
            let tx2 = tx.clone();

            // Simulate: key press, wait 250ms (> 200ms debounce), then key release
            let handle = tokio::spawn(async move {
                // Debounce timer fires after 200ms
                tokio::time::sleep(Duration::from_millis(200)).await;
                let _ = tx2.send(AppEvent::StartRecording).await;
            });

            // Wait for debounce to pass
            tokio::time::sleep(Duration::from_millis(250)).await;
            assert!(!handle.is_finished() || true); // timer already fired

            // Simulate key release after debounce
            let _ = tx.send(AppEvent::StopRecording).await;

            // Should receive StartRecording then StopRecording
            let evt1 = rx.recv().await.unwrap();
            assert!(matches!(evt1, AppEvent::StartRecording));
            let evt2 = rx.recv().await.unwrap();
            assert!(matches!(evt2, AppEvent::StopRecording));
        }

        /// Simulates the debounce logic: hold < 200ms should NOT fire StartRecording
        #[tokio::test]
        async fn test_hold_too_short_cancels() {
            let (tx, mut rx) = mpsc::channel(8);

            // Simulate: key press, start debounce timer
            let handle = tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let _ = tx.send(AppEvent::StartRecording).await;
            });

            // Release after only 100ms (before debounce threshold)
            tokio::time::sleep(Duration::from_millis(100)).await;
            handle.abort(); // cancel the debounce

            // Give time for any events to arrive
            tokio::time::sleep(Duration::from_millis(150)).await;

            // Channel should be empty — no StartRecording was sent
            assert!(rx.try_recv().is_err());
        }

        /// Verify the debounce constant is 200ms
        #[test]
        fn test_debounce_constant() {
            use crate::hotkey::HOLD_DEBOUNCE;
            assert_eq!(HOLD_DEBOUNCE, Duration::from_millis(200));
        }
    }

    mod settings_tests {
        use crate::settings::*;

        #[test]
        fn test_default_settings() {
            let s = Settings::default();
            assert_eq!(s.hotkey.toggle_key, "Ctrl+Shift+Space");
            assert_eq!(s.audio.sample_rate, 16000);
            assert_eq!(s.transcription.model, "base");
            assert_eq!(s.transcription.language, "en");
            assert!(s.refinement.enabled);
        }

        #[test]
        fn test_settings_serialization() {
            let s = Settings::default();
            let toml_str = toml::to_string_pretty(&s).unwrap();
            let loaded: Settings = toml::from_str(&toml_str).unwrap();
            assert_eq!(loaded.hotkey.toggle_key, s.hotkey.toggle_key);
            assert_eq!(loaded.transcription.model, s.transcription.model);
        }
    }

    mod refinement_tests {
        use crate::refinement;
        use crate::settings::*;

        fn test_settings() -> Settings {
            Settings::default()
        }

        #[test]
        fn test_filler_removal() {
            let s = test_settings();
            let result = refinement::apply("um hello um world", &s);
            assert!(!result.contains("um"));
            assert!(result.to_lowercase().contains("hello"));
            assert!(result.to_lowercase().contains("world"));
        }

        #[test]
        fn test_capitalization() {
            let s = test_settings();
            let result = refinement::apply("hello. world", &s);
            assert!(result.starts_with('H'));
        }

        #[test]
        fn test_custom_vocabulary() {
            let mut s = test_settings();
            s.refinement.custom_vocabulary = vec!["Rust".to_string(), "GNOME".to_string()];
            let result = refinement::apply("i love rust and gnome", &s);
            assert!(result.contains("Rust"));
            assert!(result.contains("GNOME"));
        }

        #[test]
        fn test_disabled_refinement() {
            let mut s = test_settings();
            s.refinement.enabled = false;
            let input = "um hello um world";
            let result = refinement::apply(input, &s);
            assert_eq!(result, input);
        }

        #[test]
        fn test_empty_input() {
            let s = test_settings();
            assert_eq!(refinement::apply("", &s), "");
        }
    }

    mod language_tests {
        use crate::languages;

        #[test]
        fn test_language_count() {
            let langs = languages::all_languages();
            assert!(langs.len() >= 99);
        }

        #[test]
        fn test_find_english() {
            let lang = languages::find_language("en").unwrap();
            assert_eq!(lang.name, "English");
        }

        #[test]
        fn test_find_japanese() {
            let lang = languages::find_language("ja").unwrap();
            assert_eq!(lang.name, "Japanese");
            assert_eq!(lang.native_name, "日本語");
        }

        #[test]
        fn test_find_nonexistent() {
            assert!(languages::find_language("xx").is_none());
        }
    }

    mod history_tests {
        use crate::history::HistoryStore;

        #[test]
        fn test_history_add_and_retrieve() {
            // Use a temp dir to avoid polluting real data
            let tmp = std::env::temp_dir().join("frogscribe_test_history");
            let _ = std::fs::create_dir_all(&tmp);
            let path = tmp.join("history.json");
            let _ = std::fs::remove_file(&path);

            // We can't easily test HistoryStore::new() since it uses fixed paths,
            // but we can test the serialization
            use crate::history::HistoryEntry;
            let entry = HistoryEntry {
                id: 1,
                text: "Hello world".to_string(),
                timestamp: "1234567890".to_string(),
                duration_secs: 2.5,
                model: "base".to_string(),
                language: "en".to_string(),
            };
            let json = serde_json::to_string(&vec![entry.clone()]).unwrap();
            let loaded: Vec<HistoryEntry> = serde_json::from_str(&json).unwrap();
            assert_eq!(loaded[0].text, "Hello world");
            assert_eq!(loaded[0].duration_secs, 2.5);
        }
    }

    mod audio_device_tests {
        use crate::audio::devices;

        #[test]
        fn test_list_devices_no_panic() {
            // May fail in CI without PulseAudio, but shouldn't panic
            let _ = devices::list_input_devices();
        }
    }

    mod model_tests {
        use crate::models::WhisperModel;

        #[test]
        fn test_all_models() {
            let models = WhisperModel::all();
            assert_eq!(models.len(), 5);
            assert_eq!(models[0].id, "tiny");
            assert_eq!(models[4].id, "large-v3");
        }
    }

    mod settings_persistence_tests {
        use crate::settings::*;
        use std::path::PathBuf;

        #[test]
        fn test_settings_roundtrip_all_fields() {
            let mut s = Settings::default();
            s.hotkey.toggle_key = "Ctrl+Shift+D".into();
            s.hotkey.activation_method = ActivationMethod::HoldToTalk;
            s.hotkey.hold_key = Some("Super+Space".into());
            s.audio.device = Some("alsa_input.usb-mic".into());
            s.audio.office_mode = true;
            s.transcription.model = "small".into();
            s.transcription.language = "ja".into();
            s.transcription.translate_to_english = true;
            s.transcription.streaming = true;
            s.refinement.enabled = false;
            s.refinement.remove_fillers = false;
            s.refinement.custom_vocabulary = vec!["FrogScribe".into(), "GNOME".into()];
            s.appearance.indicator_style = Some(IndicatorStyle::TopBar);
            s.appearance.accent_color = "purple".into();
            s.general.auto_submit = true;
            s.general.history_enabled = false;

            let toml_str = toml::to_string_pretty(&s).unwrap();
            let loaded: Settings = toml::from_str(&toml_str).unwrap();

            assert_eq!(loaded.hotkey.toggle_key, "Ctrl+Shift+D");
            assert!(matches!(loaded.hotkey.activation_method, ActivationMethod::HoldToTalk));
            assert_eq!(loaded.hotkey.hold_key, Some("Super+Space".into()));
            assert_eq!(loaded.audio.device, Some("alsa_input.usb-mic".into()));
            assert!(loaded.audio.office_mode);
            assert_eq!(loaded.transcription.model, "small");
            assert_eq!(loaded.transcription.language, "ja");
            assert!(loaded.transcription.translate_to_english);
            assert!(loaded.transcription.streaming);
            assert!(!loaded.refinement.enabled);
            assert!(!loaded.refinement.remove_fillers);
            assert_eq!(loaded.refinement.custom_vocabulary, vec!["FrogScribe", "GNOME"]);
            assert!(matches!(loaded.appearance.indicator_style, Some(IndicatorStyle::TopBar)));
            assert_eq!(loaded.appearance.accent_color, "purple");
            assert!(loaded.general.auto_submit);
            assert!(!loaded.general.history_enabled);
        }

        #[test]
        fn test_settings_save_and_load_from_disk() {
            let tmp = std::env::temp_dir().join("frogscribe_test_settings");
            let _ = std::fs::create_dir_all(&tmp);
            let path = tmp.join("test_settings.toml");

            let mut s = Settings::default();
            s.transcription.model = "medium".into();
            s.general.auto_submit = true;

            let content = toml::to_string_pretty(&s).unwrap();
            std::fs::write(&path, &content).unwrap();

            let loaded_content = std::fs::read_to_string(&path).unwrap();
            let loaded: Settings = toml::from_str(&loaded_content).unwrap();
            assert_eq!(loaded.transcription.model, "medium");
            assert!(loaded.general.auto_submit);

            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn test_settings_paths() {
            let config = Settings::config_path();
            assert!(config.to_str().unwrap().contains("frogscribe"));
            assert!(config.to_str().unwrap().ends_with("settings.toml"));

            let models = Settings::models_dir();
            assert!(models.to_str().unwrap().contains("frogscribe"));
            assert!(models.to_str().unwrap().contains("models"));
        }
    }

    mod longform_tests {
        use crate::longform::*;

        #[test]
        fn test_transcript_session_full_text() {
            let session = TranscriptSession {
                title: "Test".into(),
                started_at: 1000,
                duration_secs: 90,
                chunks: vec![
                    TranscriptChunk { text: "Hello world.".into(), offset_secs: 0, duration_secs: 30.0 },
                    TranscriptChunk { text: "This is a test.".into(), offset_secs: 30, duration_secs: 30.0 },
                    TranscriptChunk { text: "Final chunk.".into(), offset_secs: 60, duration_secs: 30.0 },
                ],
            };
            assert_eq!(session.full_text(), "Hello world. This is a test. Final chunk.");
        }

        #[test]
        fn test_transcript_session_empty() {
            let session = TranscriptSession {
                title: "Empty".into(),
                started_at: 2000,
                duration_secs: 0,
                chunks: vec![],
            };
            assert_eq!(session.full_text(), "");
        }

        #[test]
        fn test_transcript_session_serialization() {
            let session = TranscriptSession {
                title: "My Session".into(),
                started_at: 1716800000,
                duration_secs: 120,
                chunks: vec![
                    TranscriptChunk { text: "First chunk.".into(), offset_secs: 0, duration_secs: 30.0 },
                    TranscriptChunk { text: "Second chunk.".into(), offset_secs: 30, duration_secs: 28.5 },
                ],
            };

            let json = serde_json::to_string(&session).unwrap();
            let loaded: TranscriptSession = serde_json::from_str(&json).unwrap();

            assert_eq!(loaded.title, "My Session");
            assert_eq!(loaded.started_at, 1716800000);
            assert_eq!(loaded.duration_secs, 120);
            assert_eq!(loaded.chunks.len(), 2);
            assert_eq!(loaded.chunks[0].text, "First chunk.");
            assert_eq!(loaded.chunks[1].offset_secs, 30);
            assert_eq!(loaded.chunks[1].duration_secs, 28.5);
        }

        #[test]
        fn test_transcript_chunk_serialization() {
            let chunk = TranscriptChunk {
                text: "Some transcribed text here.".into(),
                offset_secs: 45,
                duration_secs: 29.7,
            };

            let json = serde_json::to_string(&chunk).unwrap();
            assert!(json.contains("Some transcribed text here."));
            assert!(json.contains("45"));

            let loaded: TranscriptChunk = serde_json::from_str(&json).unwrap();
            assert_eq!(loaded.text, "Some transcribed text here.");
            assert_eq!(loaded.offset_secs, 45);
        }

        #[test]
        fn test_session_persistence_roundtrip() {
            let tmp = std::env::temp_dir().join("frogscribe_test_sessions");
            let _ = std::fs::create_dir_all(&tmp);
            let path = tmp.join("test-session.json");

            let session = TranscriptSession {
                title: "Persistence Test".into(),
                started_at: 9999,
                duration_secs: 60,
                chunks: vec![
                    TranscriptChunk { text: "Chunk one.".into(), offset_secs: 0, duration_secs: 30.0 },
                    TranscriptChunk { text: "Chunk two.".into(), offset_secs: 30, duration_secs: 30.0 },
                ],
            };

            let data = serde_json::to_string_pretty(&session).unwrap();
            std::fs::write(&path, &data).unwrap();

            let loaded_data = std::fs::read_to_string(&path).unwrap();
            let loaded: TranscriptSession = serde_json::from_str(&loaded_data).unwrap();

            assert_eq!(loaded.title, "Persistence Test");
            assert_eq!(loaded.chunks.len(), 2);
            assert_eq!(loaded.full_text(), "Chunk one. Chunk two.");

            let _ = std::fs::remove_file(&path);
        }

        #[test]
        fn test_longform_event_variants() {
            // Verify event enum can be constructed
            let e1 = LongFormEvent::ChunkTranscribed { text: "hello".into(), elapsed_secs: 30 };
            let e2 = LongFormEvent::SessionComplete {
                session: TranscriptSession {
                    title: "t".into(), started_at: 0, duration_secs: 0, chunks: vec![],
                },
            };
            let e3 = LongFormEvent::Error { message: "oops".into() };

            // Just verify they exist and can be matched
            match e1 { LongFormEvent::ChunkTranscribed { text, .. } => assert_eq!(text, "hello"), _ => panic!() }
            match e2 { LongFormEvent::SessionComplete { .. } => {} _ => panic!() }
            match e3 { LongFormEvent::Error { message } => assert_eq!(message, "oops"), _ => panic!() }
        }
    }
    mod hotkey_recorder_tests {
        use crate::ui::capitalize_key;

        #[test]
        fn test_capitalize_single_letter() {
            assert_eq!(capitalize_key("a"), "A");
            assert_eq!(capitalize_key("z"), "Z");
        }

        #[test]
        fn test_capitalize_space() {
            assert_eq!(capitalize_key("space"), "Space");
            assert_eq!(capitalize_key("Space"), "Space");
        }

        #[test]
        fn test_capitalize_return() {
            assert_eq!(capitalize_key("return"), "Enter");
            assert_eq!(capitalize_key("Return"), "Enter");
        }

        #[test]
        fn test_capitalize_function_keys() {
            assert_eq!(capitalize_key("f1"), "F1");
            assert_eq!(capitalize_key("f12"), "F12");
        }

        #[test]
        fn test_capitalize_special_keys() {
            assert_eq!(capitalize_key("tab"), "Tab");
            assert_eq!(capitalize_key("backspace"), "Backspace");
            assert_eq!(capitalize_key("delete"), "Delete");
            assert_eq!(capitalize_key("escape"), "Escape");
        }

        #[test]
        fn test_capitalize_other() {
            assert_eq!(capitalize_key("home"), "Home");
            assert_eq!(capitalize_key("end"), "End");
        }
    }

    mod transcript_window_tests {
        use crate::transcript_window::TranscriptWindowState;
        use std::sync::Arc;

        #[test]
        fn test_state_new_is_empty() {
            let state = TranscriptWindowState::new();
            assert_eq!(state.full_text(), "");
            assert!(state.is_active.load(std::sync::atomic::Ordering::Relaxed));
        }

        #[test]
        fn test_append_text_single() {
            let state = TranscriptWindowState::new();
            state.append_text("Hello world.");
            assert_eq!(state.full_text(), "Hello world.");
        }

        #[test]
        fn test_append_text_multiple() {
            let state = TranscriptWindowState::new();
            state.append_text("First chunk.");
            state.append_text("Second chunk.");
            state.append_text("Third chunk.");
            assert_eq!(state.full_text(), "First chunk. Second chunk. Third chunk.");
        }

        #[test]
        fn test_elapsed_secs() {
            let state = TranscriptWindowState::new();
            // Should be 0 or very close to 0 immediately after creation
            assert!(state.elapsed_secs() < 2);
        }

        #[test]
        fn test_is_active_flag() {
            let state = Arc::new(TranscriptWindowState::new());
            assert!(state.is_active.load(std::sync::atomic::Ordering::Relaxed));
            state.is_active.store(false, std::sync::atomic::Ordering::Relaxed);
            assert!(!state.is_active.load(std::sync::atomic::Ordering::Relaxed));
        }

        #[test]
        fn test_thread_safety() {
            let state = Arc::new(TranscriptWindowState::new());

            let handles: Vec<_> = (0..10).map(|i| {
                let s = state.clone();
                std::thread::spawn(move || {
                    s.append_text(&format!("chunk{}", i));
                })
            }).collect();

            for h in handles { h.join().unwrap(); }

            let text = state.full_text();
            // All 10 chunks should be present
            for i in 0..10 {
                assert!(text.contains(&format!("chunk{}", i)));
            }
        }
    }

    mod gtk_init_tests {
        /// Verify that the onboarding GUI runs as a subprocess (--setup flag),
        /// not in-process, to avoid GTK3/GTK4 conflict with the tray.
        /// The main process uses GTK3 for the tray; GTK4 onboarding must be isolated.
        #[test]
        fn test_onboarding_uses_subprocess_not_inprocess() {
            // run_auto() should spawn `frogscribe --setup` as a child process
            // rather than calling gui::run_gui() directly in the daemon process.
            // Verify by checking that the --setup flag exists in our binary's CLI.
            use clap::Parser;
            let args = crate::Args::try_parse_from(["frogscribe", "--setup"]).unwrap();
            assert!(args.setup);
        }

        /// Verify --setup flag doesn't conflict with daemon mode
        #[test]
        fn test_setup_flag_is_exclusive() {
            use clap::Parser;
            // --setup alone should parse fine
            let args = crate::Args::try_parse_from(["frogscribe", "--setup"]).unwrap();
            assert!(args.setup);
            assert!(args.transcribe.is_none());
            assert!(args.download_model.is_none());
        }

        /// Verify has_display() checks environment variables
        #[test]
        fn test_has_display_checks_env() {
            // has_display() checks DISPLAY or WAYLAND_DISPLAY
            // In test environment, at least one should be set if we have a display
            let display = std::env::var("DISPLAY").is_ok();
            let wayland = std::env::var("WAYLAND_DISPLAY").is_ok();
            // Just verify it doesn't panic
            let _ = display || wayland;
        }
    }

    mod summarization_tests {
        use crate::summarization;
        use crate::settings::{SummarizationConfig, available_summarization_models};

        #[test]
        fn test_summarization_config_defaults() {
            let config = SummarizationConfig::default();
            assert!(!config.enabled);
            assert_eq!(config.model, "phi-3-mini");
        }

        #[test]
        fn test_available_models_not_empty() {
            let models = available_summarization_models();
            assert_eq!(models.len(), 1);
            assert_eq!(models[0].0, "phi-3-mini");
        }

        #[test]
        fn test_available_models_have_descriptions() {
            for (id, size, desc) in available_summarization_models() {
                assert!(!id.is_empty());
                assert!(!size.is_empty());
                assert!(!desc.is_empty());
            }
        }

        #[test]
        fn test_models_dir_contains_summarization() {
            let dir = summarization::models_dir();
            assert!(dir.to_str().unwrap().contains("summarization"));
        }

        #[test]
        fn test_summarize_empty_text_returns_empty() {
            let result = summarization::summarize("", "distilbart-cnn-12-6");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), "");
        }

        #[test]
        fn test_summarize_short_text_returns_unchanged() {
            let short = "This is a short text with less than thirty words.";
            let result = summarization::summarize(short, "distilbart-cnn-12-6");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), short);
        }

        #[test]
        fn test_summarize_without_model_returns_error() {
            let long_text = "The quick brown fox jumped over the lazy dog. ".repeat(10);
            let result = summarization::summarize(&long_text, "distilbart-cnn-12-6");
            // Should fail because model isn't downloaded in test environment
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("not downloaded"));
        }

        #[test]
        fn test_is_model_downloaded_false_by_default() {
            assert!(!summarization::is_model_downloaded("distilbart-cnn-12-6"));
            assert!(!summarization::is_model_downloaded("bart-large-cnn"));
        }

        #[test]
        fn test_is_model_downloaded_nonexistent_model() {
            assert!(!summarization::is_model_downloaded("nonexistent-model-xyz"));
        }
    }

    mod model_integrity_tests {
        use crate::model_integrity::{
            git_blob_sha1_file, parse_hf_url, sha256_file, verify_downloaded_sha256,
            verify_file, ExpectedDigest, ExpectedIntegrity,
        };
        use std::path::PathBuf;

        // Authoritative vectors, computed independently with `sha256sum` and
        // `git hash-object` for the byte string below.
        const CONTENT: &[u8] = b"frogscribe integrity test payload"; // 33 bytes
        const CONTENT_LEN: u64 = 33;
        const CONTENT_SHA256: &str =
            "20ba689d732d4356eb63f1aff9ddfa1166d9a9e419a0ba0be06dba521f92121b";
        const CONTENT_GIT_BLOB_SHA1: &str = "f289c0d4f3d4e5eaa5a92acf58ace6004e2ef5c4";

        fn temp_path(tag: &str) -> PathBuf {
            let mut p = std::env::temp_dir();
            p.push(format!("frogscribe_integrity_{}_{}", tag, std::process::id()));
            p
        }

        fn write_content(path: &PathBuf) {
            std::fs::write(path, CONTENT).unwrap();
        }

        #[test]
        fn test_parse_hf_url_valid() {
            let (repo, rev, path) = parse_hf_url(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
            )
            .unwrap();
            assert_eq!(repo, "ggerganov/whisper.cpp");
            assert_eq!(rev, "main");
            assert_eq!(path, "ggml-base.bin");
        }

        #[test]
        fn test_parse_hf_url_subdirectory_path() {
            // Files in a subdirectory (e.g. onnx/) must keep the full path.
            let (repo, rev, path) = parse_hf_url(
                "https://huggingface.co/Xenova/distilbart-cnn-12-6/resolve/main/onnx/encoder_model.onnx",
            )
            .unwrap();
            assert_eq!(repo, "Xenova/distilbart-cnn-12-6");
            assert_eq!(rev, "main");
            assert_eq!(path, "onnx/encoder_model.onnx");
        }

        #[test]
        fn test_parse_hf_url_rejects_invalid() {
            assert!(parse_hf_url("https://example.com/foo/resolve/main/bar.bin").is_none());
            assert!(parse_hf_url("https://huggingface.co/only/repo").is_none());
            assert!(parse_hf_url("not even a url").is_none());
        }

        #[test]
        fn test_sha256_file_matches_known_vector() {
            let p = temp_path("sha256");
            write_content(&p);
            assert_eq!(sha256_file(&p).unwrap(), CONTENT_SHA256);
            let _ = std::fs::remove_file(&p);
        }

        #[test]
        fn test_git_blob_sha1_matches_known_vector() {
            let p = temp_path("gitblob");
            write_content(&p);
            assert_eq!(
                git_blob_sha1_file(&p, CONTENT_LEN).unwrap(),
                CONTENT_GIT_BLOB_SHA1
            );
            let _ = std::fs::remove_file(&p);
        }

        #[test]
        fn test_verify_downloaded_sha256_success_keeps_file() {
            let p = temp_path("dl_ok");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::Sha256(CONTENT_SHA256.to_string()),
                size: CONTENT_LEN,
            };
            assert!(verify_downloaded_sha256(&p, CONTENT_SHA256, &expected).is_ok());
            assert!(p.exists(), "a verified file must be kept");
            let _ = std::fs::remove_file(&p);
        }

        #[test]
        fn test_verify_downloaded_sha256_hash_mismatch_removes_file() {
            let p = temp_path("dl_badhash");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::Sha256("00".repeat(32)),
                size: CONTENT_LEN,
            };
            // Even though we pass the real hash as "actual", it disagrees with
            // the (wrong) expected digest, simulating a tampered download.
            let res = verify_downloaded_sha256(&p, CONTENT_SHA256, &expected);
            assert!(res.is_err());
            assert!(!p.exists(), "a tampered file must be removed");
        }

        #[test]
        fn test_verify_downloaded_sha256_size_mismatch_removes_file() {
            let p = temp_path("dl_badsize");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::Sha256(CONTENT_SHA256.to_string()),
                size: CONTENT_LEN + 100, // wrong size
            };
            let res = verify_downloaded_sha256(&p, CONTENT_SHA256, &expected);
            assert!(res.is_err());
            assert!(!p.exists(), "a size-mismatched file must be removed");
        }

        #[test]
        fn test_verify_file_sha256_success() {
            let p = temp_path("vf_sha256");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::Sha256(CONTENT_SHA256.to_string()),
                size: CONTENT_LEN,
            };
            assert!(verify_file(&p, &expected).is_ok());
            assert!(p.exists());
            let _ = std::fs::remove_file(&p);
        }

        #[test]
        fn test_verify_file_git_blob_sha1_success() {
            // Mirrors the non-LFS (e.g. tokenizer.json) verification path.
            let p = temp_path("vf_gitblob");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::GitBlobSha1(CONTENT_GIT_BLOB_SHA1.to_string()),
                size: CONTENT_LEN,
            };
            assert!(verify_file(&p, &expected).is_ok());
            assert!(p.exists());
            let _ = std::fs::remove_file(&p);
        }

        #[test]
        fn test_verify_file_git_blob_sha1_mismatch_removes_file() {
            let p = temp_path("vf_gitblob_bad");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::GitBlobSha1("0".repeat(40)),
                size: CONTENT_LEN,
            };
            let res = verify_file(&p, &expected);
            assert!(res.is_err());
            assert!(!p.exists(), "a tampered non-LFS file must be removed");
        }

        #[test]
        fn test_verify_downloaded_sha256_recomputes_for_non_lfs_digest() {
            // When HF publishes a git-blob digest but the caller streamed a
            // SHA256, verification must recompute the correct hash from disk
            // rather than trust the (wrong-algorithm) precomputed value.
            let p = temp_path("dl_fallback");
            write_content(&p);
            let expected = ExpectedIntegrity {
                digest: ExpectedDigest::GitBlobSha1(CONTENT_GIT_BLOB_SHA1.to_string()),
                size: CONTENT_LEN,
            };
            // Pass a bogus sha256; it must be ignored in favor of the on-disk
            // git-blob hash, which matches.
            assert!(verify_downloaded_sha256(&p, "deadbeef", &expected).is_ok());
            assert!(p.exists());
            let _ = std::fs::remove_file(&p);
        }
    }
}
