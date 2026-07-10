use gtk3::prelude::*;
use gtk3::{self, Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Label, Notebook, Orientation, ProgressBar, RadioButton};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use crate::settings::{ActivationMethod, Settings};

const TOTAL_PAGES: u32 = 7;

pub fn run_gui() {
    let app = Application::builder()
        .application_id("com.frogscribe.onboarding")
        .build();

    app.connect_activate(build_wizard);
    app.run_with_args::<&str>(&[]);
}

fn build_wizard(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Welcome to FrogScribe")
        .default_width(550)
        .default_height(450)
        .resizable(false)
        .build();

    let notebook = Notebook::new();
    notebook.set_show_tabs(false);

    let selected_model = Rc::new(RefCell::new("base".to_string()));
    let selected_method = Rc::new(RefCell::new(ActivationMethod::Toggle));
    let selected_hotkey = Rc::new(RefCell::new("Ctrl+Shift+Space".to_string()));

    // Page 0: Welcome
    notebook.append_page(&build_welcome_page(&notebook), None::<&Label>);
    // Page 1: Permissions
    notebook.append_page(&build_permissions_page(&notebook), None::<&Label>);
    // Page 2: Activation Method
    notebook.append_page(&build_activation_page(&notebook, &selected_method), None::<&Label>);
    // Page 3: Hotkey Configuration
    notebook.append_page(&build_hotkey_page(&notebook, &selected_hotkey), None::<&Label>);
    // Page 4: Model
    notebook.append_page(&build_model_page(&notebook, &selected_model), None::<&Label>);
    // Page 5: Practice
    notebook.append_page(&build_practice_page(&notebook, &selected_model), None::<&Label>);
    // Page 6: Complete
    notebook.append_page(&build_complete_page(&window, &selected_model, &selected_method, &selected_hotkey), None::<&Label>);

    window.add(&notebook);
    window.show_all();
}

fn progress_label(step: u32) -> Label {
    let lbl = Label::new(Some(&format!("Step {} of {}", step, TOTAL_PAGES)));
    lbl.set_halign(gtk3::Align::End);
    lbl.set_opacity(0.6);
    lbl
}

fn build_welcome_page(notebook: &Notebook) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(1), false, false, 0);

    // FrogScribe logo
    let icon_path = "/usr/share/icons/hicolor/128x128/apps/frogscribe.png";
    let fallback = std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|d| d.join("../resources/frogscribe-128.png").to_path_buf()))
        .unwrap_or_default();
    let path = if std::path::Path::new(icon_path).exists() { icon_path.to_string() } else { fallback.to_string_lossy().to_string() };
    if std::path::Path::new(&path).exists() {
        let image = gtk3::Image::from_file(&path);
        page.pack_start(&image, false, false, 8);
    }

    page.pack_start(&Label::new(Some("Welcome to FrogScribe")), false, false, 8);
    page.pack_start(&Label::new(Some("Voice dictation powered by on-device AI.\nPress a hotkey, speak, and text appears at your cursor.")), false, false, 8);
    page.pack_start(&Label::new(Some("✓ Private — all processing on your device\n✓ Fast — transcription in under a second\n✓ Works everywhere — inserts text in any app")), false, false, 8);

    let btn = Button::with_label("Get Started →");
    let nb = notebook.clone();
    btn.connect_clicked(move |_| { nb.next_page(); });
    page.pack_end(&btn, false, false, 8);
    page
}

fn build_permissions_page(notebook: &Notebook) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(2), false, false, 0);
    page.pack_start(&Label::new(Some("Permissions")), false, false, 8);

    let input_ok = std::process::Command::new("groups").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("input")).unwrap_or(false);
    page.pack_start(&Label::new(Some(&format!("{} Input group (hotkeys)", if input_ok { "✓" } else { "⚠" }))), false, false, 4);

    let tool_ok = std::process::Command::new("which").arg("ydotool").output()
        .map(|o| o.status.success()).unwrap_or(false)
        || std::process::Command::new("which").arg("xdotool").output()
        .map(|o| o.status.success()).unwrap_or(false);
    page.pack_start(&Label::new(Some(&format!("{} Text insertion", if tool_ok { "✓" } else { "⚠" }))), false, false, 4);

    let parec_ok = std::process::Command::new("which").arg("parec").output()
        .map(|o| o.status.success()).unwrap_or(false);
    page.pack_start(&Label::new(Some(&format!("{} Audio capture", if parec_ok { "✓" } else { "⚠" }))), false, false, 4);

    let btn = Button::with_label("Next →");
    let nb = notebook.clone();
    btn.connect_clicked(move |_| { nb.next_page(); });
    page.pack_end(&btn, false, false, 8);
    page
}

fn build_activation_page(notebook: &Notebook, selected_method: &Rc<RefCell<ActivationMethod>>) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(3), false, false, 0);
    page.pack_start(&Label::new(Some("Activation Method")), false, false, 8);
    page.pack_start(&Label::new(Some("How would you like to trigger dictation?")), false, false, 4);

    let toggle_radio = RadioButton::with_label("Toggle Hotkey — press once to start, again to stop");
    let hold_radio = RadioButton::with_label_from_widget(&toggle_radio, "Hold to Talk — hold key to record, release to transcribe");
    toggle_radio.set_active(true);

    let sm = selected_method.clone();
    toggle_radio.connect_toggled(move |r| {
        if r.is_active() {
            *sm.borrow_mut() = ActivationMethod::Toggle;
        }
    });
    let sm2 = selected_method.clone();
    hold_radio.connect_toggled(move |r| {
        if r.is_active() {
            *sm2.borrow_mut() = ActivationMethod::HoldToTalk;
        }
    });

    page.pack_start(&toggle_radio, false, false, 4);
    page.pack_start(&hold_radio, false, false, 4);

    let btn = Button::with_label("Next →");
    let nb = notebook.clone();
    btn.connect_clicked(move |_| { nb.next_page(); });
    page.pack_end(&btn, false, false, 8);
    page
}

fn build_hotkey_page(notebook: &Notebook, selected_hotkey: &Rc<RefCell<String>>) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(4), false, false, 0);
    page.pack_start(&Label::new(Some("Hotkey Configuration")), false, false, 8);
    page.pack_start(&Label::new(Some("Press your desired key combination below:")), false, false, 4);

    let hotkey_label = Label::new(Some("Ctrl+Shift+Space"));
    hotkey_label.set_markup("<span size='x-large'><b>Ctrl+Shift+Space</b></span>");

    let frame = gtk3::Frame::new(None);
    frame.set_shadow_type(gtk3::ShadowType::EtchedIn);
    let event_box = gtk3::EventBox::new();
    event_box.add(&hotkey_label);
    frame.add(&event_box);
    frame.set_size_request(-1, 60);
    page.pack_start(&frame, false, false, 8);

    let capture_btn = Button::with_label("Press to set hotkey...");
    let _hl = hotkey_label.clone();
    let _sh = selected_hotkey.clone();
    let capturing = Rc::new(RefCell::new(false));
    let cap2 = capturing.clone();
    let eb_for_focus = event_box.clone();

    capture_btn.connect_clicked(move |btn| {
        *cap2.borrow_mut() = true;
        btn.set_label("Press any key combination...");
        btn.set_sensitive(false);
        eb_for_focus.grab_focus();
    });

    let sh2 = selected_hotkey.clone();
    let hl2 = hotkey_label.clone();
    let cap3 = capturing.clone();
    let cap_btn2 = capture_btn.clone();
    event_box.set_can_focus(true);
    event_box.add_events(gtk3::gdk::EventMask::KEY_PRESS_MASK);
    event_box.connect_key_press_event(move |_widget, event| {
        if !*cap3.borrow() {
            return glib::Propagation::Proceed;
        }
        let keyval = event.keyval();
        let state = event.state();

        // Ignore bare modifier presses
        if matches!(keyval, gtk3::gdk::keys::constants::Shift_L | gtk3::gdk::keys::constants::Shift_R
            | gtk3::gdk::keys::constants::Control_L | gtk3::gdk::keys::constants::Control_R
            | gtk3::gdk::keys::constants::Alt_L | gtk3::gdk::keys::constants::Alt_R
            | gtk3::gdk::keys::constants::Super_L | gtk3::gdk::keys::constants::Super_R) {
            return glib::Propagation::Stop;
        }

        let mut parts = Vec::new();
        if state.contains(gtk3::gdk::ModifierType::CONTROL_MASK) { parts.push("Ctrl"); }
        if state.contains(gtk3::gdk::ModifierType::MOD1_MASK) { parts.push("Alt"); }
        if state.contains(gtk3::gdk::ModifierType::SUPER_MASK) { parts.push("Super"); }
        if state.contains(gtk3::gdk::ModifierType::SHIFT_MASK) { parts.push("Shift"); }

        let key_name = keyval.name().unwrap_or_else(|| "Unknown".into());
        // Capitalize first letter for display
        let key_display = capitalize(&key_name);
        parts.push(&key_display);

        let combo = parts.join("+");
        hl2.set_markup(&format!("<span size='x-large'><b>{}</b></span>", combo));
        *sh2.borrow_mut() = combo;
        *cap3.borrow_mut() = false;
        cap_btn2.set_label("Press to set hotkey...");
        cap_btn2.set_sensitive(true);

        glib::Propagation::Stop
    });

    page.pack_start(&capture_btn, false, false, 4);

    let reset_btn = Button::with_label("Reset to Ctrl+Shift+Space");
    let hl3 = hotkey_label;
    let sh3 = selected_hotkey.clone();
    reset_btn.connect_clicked(move |_| {
        hl3.set_markup("<span size='x-large'><b>Ctrl+Shift+Space</b></span>");
        *sh3.borrow_mut() = "Ctrl+Shift+Space".to_string();
    });
    page.pack_start(&reset_btn, false, false, 4);

    let btn = Button::with_label("Next →");
    let nb = notebook.clone();
    btn.connect_clicked(move |_| { nb.next_page(); });
    page.pack_end(&btn, false, false, 8);
    page
}

fn build_model_page(notebook: &Notebook, selected_model: &Rc<RefCell<String>>) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(5), false, false, 0);
    page.pack_start(&Label::new(Some("Choose a Model")), false, false, 8);

    let combo = ComboBoxText::new();
    for (id, size, desc) in crate::transcription::available_models() {
        let dl = if Settings::models_dir().join(format!("ggml-{}.bin", id)).exists() { " ✓" } else { "" };
        combo.append(Some(id), &format!("{} ({}) — {}{}", id, size, desc, dl));
    }
    combo.set_active_id(Some("base"));
    let sm = selected_model.clone();
    combo.connect_changed(move |c| { if let Some(id) = c.active_id() { *sm.borrow_mut() = id.to_string(); } });
    page.pack_start(&combo, false, false, 8);

    let progress = ProgressBar::new();
    progress.set_visible(false);
    page.pack_start(&progress, false, false, 4);

    let status = Label::new(Some(""));
    page.pack_start(&status, false, false, 4);

    let dl_btn = Button::with_label("Download Model");
    let sm2 = selected_model.clone();
    let p = progress.clone();
    let st = status.clone();
    dl_btn.connect_clicked(move |btn| {
        let model_id = sm2.borrow().clone();
        if Settings::models_dir().join(format!("ggml-{}.bin", model_id)).exists() {
            st.set_text("✓ Already downloaded!");
            return;
        }
        btn.set_sensitive(false);
        p.set_visible(true);
        st.set_text(&format!("Downloading {}...", model_id));

        let done = Arc::new(AtomicBool::new(false));
        let done2 = done.clone();
        let mid = model_id.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(crate::transcription::download_model(&mid));
            done2.store(true, Ordering::Relaxed);
        });

        let p2 = p.clone();
        let st2 = st.clone();
        let btn2 = btn.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            p2.pulse();
            if done.load(Ordering::Relaxed) {
                p2.set_fraction(1.0);
                st2.set_text("✓ Download complete!");
                btn2.set_sensitive(true);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    });
    page.pack_start(&dl_btn, false, false, 4);

    let btn = Button::with_label("Next →");
    let nb = notebook.clone();
    btn.connect_clicked(move |_| { nb.next_page(); });
    page.pack_end(&btn, false, false, 8);
    page
}

fn build_practice_page(notebook: &Notebook, selected_model: &Rc<RefCell<String>>) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(6), false, false, 0);
    page.pack_start(&Label::new(Some("Practice")), false, false, 8);
    page.pack_start(&Label::new(Some("Test that everything works!\nClick Record, say a few words, then click Stop.")), false, false, 4);

    // Mic status display
    let mic_box = GtkBox::new(Orientation::Vertical, 4);
    let mic_status = Label::new(None);
    mic_status.set_halign(gtk3::Align::Start);
    let unmute_btn = Button::with_label("🔊 Unmute Microphone");
    unmute_btn.set_no_show_all(true);

    let (mic_name, muted, volume) = get_mic_status();
    if muted {
        mic_status.set_markup(&format!("<span foreground='red'>🎤 {} — MUTED</span>", glib::markup_escape_text(&mic_name)));
        unmute_btn.set_visible(true);
    } else {
        mic_status.set_markup(&format!("🎤 {} — Volume {}%", glib::markup_escape_text(&mic_name), volume));
        unmute_btn.set_visible(false);
    }
    let mic_status_ref = mic_status.clone();
    let unmute_btn_ref = unmute_btn.clone();
    let mic_name_ref = mic_name.clone();
    unmute_btn.connect_clicked(move |_| {
        let _ = std::process::Command::new("pactl")
            .args(["set-source-mute", "@DEFAULT_SOURCE@", "0"])
            .status();
        mic_status_ref.set_markup(&format!("🎤 {} — Unmuted ✓", glib::markup_escape_text(&mic_name_ref)));
        unmute_btn_ref.set_visible(false);
    });
    mic_box.pack_start(&mic_status, false, false, 0);
    mic_box.pack_start(&unmute_btn, false, false, 0);
    page.pack_start(&mic_box, false, false, 4);

    let result_label = Label::new(Some(""));
    result_label.set_line_wrap(true);
    result_label.set_max_width_chars(80);

    let record_btn = Button::with_label("🎙 Record");
    let stop_btn = Button::with_label("⏹ Stop");
    stop_btn.set_sensitive(false);

    let recording = Arc::new(AtomicBool::new(false));
    let audio_samples: Arc<std::sync::Mutex<Vec<f32>>> = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Record button
    let rec2 = recording.clone();
    let samples2 = audio_samples.clone();
    let stop2 = stop_btn.clone();
    let rl2 = result_label.clone();
    record_btn.connect_clicked(move |btn| {
        rec2.store(true, Ordering::Relaxed);
        samples2.lock().unwrap().clear();
        btn.set_sensitive(false);
        stop2.set_sensitive(true);
        rl2.set_text("🎙 Recording... speak now!");

        let recording_flag = rec2.clone();
        let samples_ref = samples2.clone();
        std::thread::spawn(move || {
            let _ = capture_practice_audio(samples_ref, recording_flag);
        });
    });

    // Stop button
    let rec3 = recording.clone();
    let samples3 = audio_samples.clone();
    let rl3 = result_label.clone();
    let rec_btn3 = record_btn.clone();
    let sm3 = selected_model.clone();
    stop_btn.connect_clicked(move |btn| {
        rec3.store(false, Ordering::Relaxed);
        btn.set_sensitive(false);
        rl3.set_text("⏳ Transcribing...");

        // Give capture thread time to stop
        std::thread::sleep(std::time::Duration::from_millis(150));

        let samples = samples3.lock().unwrap().clone();
        let model_id = sm3.borrow().clone();
        let rl4 = rl3.clone();
        let rec_btn4 = rec_btn3.clone();

        let done = Arc::new(AtomicBool::new(false));
        let result_text: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new(String::new()));
        let done2 = done.clone();
        let rt2 = result_text.clone();
        let rt3 = result_text.clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let text = rt.block_on(async {
                let settings = Settings {
                    transcription: crate::settings::TranscriptionConfig {
                        model: model_id,
                        ..Settings::default().transcription
                    },
                    ..Settings::default()
                };
                let audio = crate::audio::AudioData {
                    samples,
                    sample_rate: 16000,
                    duration_secs: 0.0, // computed below
                };
                if audio.samples.len() < 8000 {
                    return "⚠ Recording too short. Try speaking for at least 1 second.".to_string();
                }
                match crate::transcription::Engine::new(&settings).await {
                    Ok(engine) => {
                        let audio = crate::audio::AudioData {
                            duration_secs: audio.samples.len() as f32 / 16000.0,
                            ..audio
                        };
                        match engine.transcribe(&audio).await {
                            Ok(text) if text.trim().is_empty() => "⚠ No speech detected. Check your microphone.".to_string(),
                            Ok(text) => format!("✓ \"{}\"\n\n🎉 It works!", text.trim()),
                            Err(e) => format!("✗ Error: {}", e),
                        }
                    }
                    Err(e) => format!("✗ Could not load model: {}", e),
                }
            });
            *rt2.lock().unwrap() = text;
            done2.store(true, Ordering::Relaxed);
        });

        let rl5 = rl4.clone();
        let rec_btn5 = rec_btn4.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            if done.load(Ordering::Relaxed) {
                let text = rt3.lock().unwrap().clone();
                rl5.set_text(&text);
                rec_btn5.set_sensitive(true);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    });

    let btn_box = GtkBox::new(Orientation::Horizontal, 8);
    btn_box.pack_start(&record_btn, true, true, 0);
    btn_box.pack_start(&stop_btn, true, true, 0);
    page.pack_start(&btn_box, false, false, 8);
    page.pack_start(&result_label, false, false, 8);

    let skip_btn = Button::with_label("Skip →");
    let nb = notebook.clone();
    skip_btn.connect_clicked(move |_| { nb.next_page(); });

    let next_btn = Button::with_label("Next →");
    let nb2 = notebook.clone();
    next_btn.connect_clicked(move |_| { nb2.next_page(); });

    let nav_box = GtkBox::new(Orientation::Horizontal, 8);
    nav_box.pack_start(&skip_btn, true, true, 0);
    nav_box.pack_start(&next_btn, true, true, 0);
    page.pack_end(&nav_box, false, false, 8);
    page
}

fn build_complete_page(
    window: &ApplicationWindow,
    selected_model: &Rc<RefCell<String>>,
    selected_method: &Rc<RefCell<ActivationMethod>>,
    selected_hotkey: &Rc<RefCell<String>>,
) -> GtkBox {
    let page = page_box();
    page.pack_start(&progress_label(7), false, false, 0);
    page.pack_start(&Label::new(Some("🎉 You're all set!")), false, false, 16);
    page.pack_start(&Label::new(Some("FrogScribe is ready. It will run in your system tray.\n\n• Press your hotkey to start dictating\n• Right-click tray icon for settings")), false, false, 8);

    let btn = Button::with_label("Start Using FrogScribe");
    let w = window.clone();
    let sm = selected_model.clone();
    let am = selected_method.clone();
    let hk = selected_hotkey.clone();
    btn.connect_clicked(move |_| {
        if let Ok(mut s) = Settings::load() {
            s.transcription.model = sm.borrow().clone();
            s.hotkey.activation_method = am.borrow().clone();
            s.hotkey.toggle_key = hk.borrow().clone();
            let _ = s.save();
        }
        crate::onboarding::mark_complete();
        w.close();
    });
    page.pack_end(&btn, false, false, 8);
    page
}

fn page_box() -> GtkBox {
    let b = GtkBox::new(Orientation::Vertical, 8);
    b.set_margin_top(32);
    b.set_margin_bottom(32);
    b.set_margin_start(40);
    b.set_margin_end(40);
    b
}

fn capture_practice_audio(
    samples: Arc<std::sync::Mutex<Vec<f32>>>,
    recording: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut child = Command::new("parec")
        .args(["--format=float32le", "--rate=16000", "--channels=1", "--latency-msec=50"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let mut stdout = child.stdout.take().unwrap();
    let mut buf = [0u8; 6400];

    while recording.load(Ordering::Relaxed) {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let float_samples: Vec<f32> = buf[..n]
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                samples.lock().unwrap().extend(float_samples);
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    Ok(())
}

/// Query default microphone name, mute state, and volume via pactl
fn get_mic_status() -> (String, bool, u32) {
    let output = std::process::Command::new("pactl")
        .args(["get-default-source"])
        .output();
    let source_name = output.ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Get details for this source
    let info = std::process::Command::new("pactl")
        .args(["list", "sources"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let mut name = source_name.clone();
    let mut muted = false;
    let mut volume = 100u32;
    let mut in_source = false;

    for line in info.lines() {
        if line.contains("Name:") && line.contains(&source_name) {
            in_source = true;
        }
        if in_source {
            if line.trim().starts_with("Description:") {
                name = line.trim().trim_start_matches("Description:").trim().to_string();
            }
            if line.trim().starts_with("Mute:") {
                muted = line.contains("yes");
            }
            if line.trim().starts_with("Volume:") {
                if let Some(pct) = line.split('/').nth(1) {
                    volume = pct.trim().trim_end_matches('%').parse().unwrap_or(100);
                }
                break; // got all we need
            }
        }
    }

    (name, muted, volume)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}
