// SPDX-License-Identifier: Apache-2.0
use gtk3::prelude::*;
use gtk3::{self, Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, Entry, Label, Notebook, Orientation, Separator, Switch, TextView};
use std::cell::RefCell;
use std::rc::Rc;

use crate::settings::Settings;

pub fn show_settings() {
    let app = Application::builder()
        .application_id("com.frogscribe.settings")
        .build();

    app.connect_activate(|app| {
        // If window already exists, raise it
        if let Some(window) = app.active_window() {
            window.set_urgency_hint(true);
            window.present_with_time(0);
            window.set_urgency_hint(false);
            return;
        }
        build_ui(app);
    });
    app.run_with_args::<&str>(&[]);
}

fn build_ui(app: &Application) {
    let settings = Rc::new(RefCell::new(Settings::load().unwrap_or_default()));

    let window = ApplicationWindow::builder()
        .application(app)
        .title("FrogScribe Settings")
        .default_width(520)
        .default_height(450)
        .build();

    let notebook = Notebook::new();

    notebook.append_page(&build_general_tab(&settings), Some(&Label::new(Some("General"))));
    notebook.append_page(&build_audio_tab(&settings), Some(&Label::new(Some("Audio"))));
    notebook.append_page(&build_transcription_tab(&settings), Some(&Label::new(Some("Transcription"))));
    notebook.append_page(&build_refinement_tab(&settings), Some(&Label::new(Some("Refinement"))));
    notebook.append_page(&build_appearance_tab(&settings), Some(&Label::new(Some("Appearance"))));

    window.add(&notebook);
    window.show_all();
}

fn save(settings: &Rc<RefCell<Settings>>) {
    if let Err(e) = settings.borrow().save() {
        tracing::error!("Failed to save settings: {}", e);
    }
}

fn build_general_tab(settings: &Rc<RefCell<Settings>>) -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    // Hotkey recorder
    let hotkey_row = GtkBox::new(Orientation::Horizontal, 8);
    hotkey_row.set_tooltip_text(Some("Keyboard shortcut to start/stop recording"));
    hotkey_row.pack_start(&Label::new(Some("Hotkey:")), false, false, 0);
    let hotkey_btn = Button::with_label(&settings.borrow().hotkey.toggle_key);
    let _s = settings.clone();
    let capturing = Rc::new(RefCell::new(false));
    let cap = capturing.clone();
    let _btn_ref = hotkey_btn.clone();
    hotkey_btn.connect_clicked(move |btn| {
        *cap.borrow_mut() = true;
        btn.set_label("Press a key combo...");
    });
    let cap2 = capturing.clone();
    let s2 = settings.clone();
    hotkey_btn.connect_key_press_event(move |btn, event| {
        if !*cap2.borrow() {
            return glib::Propagation::Proceed;
        }
        let keyval = event.keyval();
        let state = event.state();
        // Ignore bare modifier presses
        if matches!(keyval,
            gtk3::gdk::keys::constants::Shift_L | gtk3::gdk::keys::constants::Shift_R |
            gtk3::gdk::keys::constants::Control_L | gtk3::gdk::keys::constants::Control_R |
            gtk3::gdk::keys::constants::Alt_L | gtk3::gdk::keys::constants::Alt_R |
            gtk3::gdk::keys::constants::Super_L | gtk3::gdk::keys::constants::Super_R) {
            return glib::Propagation::Stop;
        }
        let mut parts = Vec::new();
        if state.contains(gtk3::gdk::ModifierType::CONTROL_MASK) { parts.push("Ctrl".to_string()); }
        if state.contains(gtk3::gdk::ModifierType::MOD1_MASK) { parts.push("Alt".to_string()); }
        if state.contains(gtk3::gdk::ModifierType::SUPER_MASK) { parts.push("Super".to_string()); }
        if state.contains(gtk3::gdk::ModifierType::SHIFT_MASK) { parts.push("Shift".to_string()); }
        let key_name = keyval.name().map(|n| capitalize_key(&n)).unwrap_or_else(|| "Unknown".into());
        parts.push(key_name);
        let combo = parts.join("+");
        btn.set_label(&combo);
        s2.borrow_mut().hotkey.toggle_key = combo;
        save(&s2);
        // Notify daemon to reload hotkey
        let _ = std::process::Command::new("dbus-send")
            .args(["--session", "--dest=com.frogscribe.Daemon", "--type=method_call",
                   "/com/frogscribe/Daemon", "com.frogscribe.Daemon.ReloadHotkey"])
            .spawn();
        *cap2.borrow_mut() = false;
        glib::Propagation::Stop
    });
    hotkey_row.pack_start(&hotkey_btn, false, false, 0);
    vbox.pack_start(&hotkey_row, false, false, 0);

    // Activation method
    let method_row = GtkBox::new(Orientation::Horizontal, 8);
    method_row.set_tooltip_text(Some("Toggle: press once to start, again to stop. Hold: hold key to record, release to transcribe"));
    method_row.pack_start(&Label::new(Some("Activation:")), false, false, 0);
    let method_combo = ComboBoxText::new();
    method_combo.append_text("Toggle");
    method_combo.append_text("HoldToTalk");
    let active = match settings.borrow().hotkey.activation_method {
        crate::settings::ActivationMethod::Toggle => 0,
        crate::settings::ActivationMethod::HoldToTalk => 1,
    };
    method_combo.set_active(Some(active));
    let s = settings.clone();
    method_combo.connect_changed(move |combo| {
        if let Some(text) = combo.active_text() {
            s.borrow_mut().hotkey.activation_method = match text.as_str() {
                "HoldToTalk" => crate::settings::ActivationMethod::HoldToTalk,
                _ => crate::settings::ActivationMethod::Toggle,
            };
            save(&s);
        }
    });
    method_row.pack_start(&method_combo, false, false, 0);
    vbox.pack_start(&method_row, false, false, 0);

    // Switches
    let s = settings.clone();
    vbox.pack_start(&switch_row("Auto-submit", "Press Enter after inserting text (useful for chat fields)", settings.borrow().general.auto_submit, move |a| { s.borrow_mut().general.auto_submit = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Auto-paste transcription", "Paste transcribed text into a window (requires ydotool). Disable to only copy to clipboard.", settings.borrow().general.auto_paste, move |a| { s.borrow_mut().general.auto_paste = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("  Use window picker", "Show a window picker to choose which window receives the paste, instead of pasting into the currently focused window", settings.borrow().general.use_window_picker, move |a| { s.borrow_mut().general.use_window_picker = a; save(&s); }), false, false, 0);

    let insert_row = GtkBox::new(Orientation::Horizontal, 8);
    insert_row.set_tooltip_text(Some("How text is typed into the target window. 'Type Every Character' works everywhere but is slower. 'Paste Full Transcript' uses clipboard + Ctrl+V and is faster."));
    insert_row.pack_start(&Label::new(Some("  Insertion method:")), false, false, 0);
    let insert_combo = ComboBoxText::new();
    insert_combo.append(Some("off"), "Off");
    insert_combo.append(Some("type"), "Type Every Character");
    insert_combo.append(Some("paste"), "Paste Full Transcript");
    let active_id = match settings.borrow().general.insertion_method {
        crate::settings::InsertionMethod::Off => "off",
        crate::settings::InsertionMethod::TypeEveryCharacter => "type",
        crate::settings::InsertionMethod::PasteFullTranscript => "paste",
    };
    insert_combo.set_active_id(Some(active_id));
    let s = settings.clone();
    insert_combo.connect_changed(move |c| {
        if let Some(id) = c.active_id() {
            s.borrow_mut().general.insertion_method = match id.as_str() {
                "off" => crate::settings::InsertionMethod::Off,
                "paste" => crate::settings::InsertionMethod::PasteFullTranscript,
                _ => crate::settings::InsertionMethod::TypeEveryCharacter,
            };
            save(&s);
        }
    });
    insert_row.pack_start(&insert_combo, false, false, 0);
    vbox.pack_start(&insert_row, false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Transcription History", "Save past transcriptions locally", settings.borrow().general.history_enabled, move |a| { s.borrow_mut().general.history_enabled = a; save(&s); }), false, false, 0);
    let _s = settings.clone();
    let s = settings.clone();
    vbox.pack_start(&switch_row("Auto-save Transcriptions", "Automatically save all transcriptions as text files in ~/.frogscribe/transcriptions", settings.borrow().general.auto_save_transcriptions, move |a| { s.borrow_mut().general.auto_save_transcriptions = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Context Header in Saved Files", "Include metadata (time, duration, source app) at the top of saved transcription files", settings.borrow().general.context_header, move |a| { s.borrow_mut().general.context_header = a; save(&s); }), false, false, 0);

    vbox
}

fn build_audio_tab(settings: &Rc<RefCell<Settings>>) -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let s = settings.clone();
    vbox.pack_start(&switch_row("Office Mode", "Boost soft-spoken audio for better accuracy in quiet environments", settings.borrow().audio.office_mode, move |a| { s.borrow_mut().audio.office_mode = a; save(&s); }), false, false, 0);

    let s = settings.clone();
    let ear_active = settings.borrow().audio.ear_protection == crate::settings::EarProtection::On;
    vbox.pack_start(&switch_row("Ear Protection (Bluetooth)", "Reduce pop/click when Bluetooth audio profile switches", ear_active, move |a| {
        s.borrow_mut().audio.ear_protection = if a { crate::settings::EarProtection::On } else { crate::settings::EarProtection::Off };
        save(&s);
    }), false, false, 0);

    let s = settings.clone();
    vbox.pack_start(&switch_row("Capture Desktop Audio", "Mix speaker/headphone output with microphone input for full meeting transcription (captures both sides of calls)", settings.borrow().audio.capture_desktop_audio, move |a| { s.borrow_mut().audio.capture_desktop_audio = a; save(&s); }), false, false, 0);

    vbox
}

fn build_transcription_tab(settings: &Rc<RefCell<Settings>>) -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let model_row = GtkBox::new(Orientation::Horizontal, 8);
    model_row.set_tooltip_text(Some("Whisper model size — larger models are more accurate but slower"));
    model_row.pack_start(&Label::new(Some("Model:")), false, false, 0);
    let model_combo = ComboBoxText::new();
    for (id, _, _) in crate::transcription::available_models() {
        model_combo.append(Some(id), id);
    }
    model_combo.set_active_id(Some(&settings.borrow().transcription.model));
    let s = settings.clone();
    model_combo.connect_changed(move |c| { if let Some(id) = c.active_id() { s.borrow_mut().transcription.model = id.to_string(); save(&s); } });
    model_row.pack_start(&model_combo, false, false, 0);
    vbox.pack_start(&model_row, false, false, 0);

    let downloaded = crate::transcription::downloaded_models();
    let dl_status = Label::new(Some(&format!("Downloaded: {}", if downloaded.is_empty() { "none".into() } else { downloaded.join(", ") })));
    dl_status.set_halign(gtk3::Align::Start);
    vbox.pack_start(&dl_status, false, false, 0);

    let dl_row = GtkBox::new(Orientation::Horizontal, 8);
    let dl_combo = ComboBoxText::new();
    for (id, size, desc) in crate::transcription::available_models() {
        let mark = if downloaded.contains(&id.to_string()) { " ✓" } else { "" };
        dl_combo.append(Some(id), &format!("{} ({}) — {}{}", id, size, desc, mark));
    }
    dl_combo.set_active(Some(0));
    dl_row.pack_start(&dl_combo, true, true, 0);

    let dl_btn = Button::with_label("Download");
    let dl_progress = Label::new(None);
    dl_progress.set_no_show_all(true);

    let dl_combo_ref = dl_combo.clone();
    let dl_btn_ref = dl_btn.clone();
    let dl_progress_ref = dl_progress.clone();
    let dl_status_ref = dl_status.clone();
    dl_btn.connect_clicked(move |_| {
        let model_id = match dl_combo_ref.active_id() {
            Some(id) => id.to_string(),
            None => return,
        };
        dl_btn_ref.set_sensitive(false);
        dl_progress_ref.set_text("Downloading...");
        dl_progress_ref.set_visible(true);

        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done2 = done.clone();
        let mid = model_id.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _ = rt.block_on(crate::transcription::download_model(&mid));
            done2.store(true, std::sync::atomic::Ordering::Relaxed);
        });

        let btn = dl_btn_ref.clone();
        let prog = dl_progress_ref.clone();
        let status = dl_status_ref.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            if done.load(std::sync::atomic::Ordering::Relaxed) {
                prog.set_text("✓ Done");
                btn.set_sensitive(true);
                let dl = crate::transcription::downloaded_models();
                status.set_text(&format!("Downloaded: {}", if dl.is_empty() { "none".into() } else { dl.join(", ") }));
                let prog2 = prog.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(3), move || { prog2.set_visible(false); });
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    });
    dl_row.pack_start(&dl_btn, false, false, 0);
    vbox.pack_start(&dl_row, false, false, 0);
    vbox.pack_start(&dl_progress, false, false, 0);

    let s = settings.clone();
    vbox.pack_start(&switch_row("Streaming (Live Preview)", "Show live transcription preview while recording", settings.borrow().transcription.streaming, move |a| { s.borrow_mut().transcription.streaming = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Translate to English", "Translate non-English speech into English text", settings.borrow().transcription.translate_to_english, move |a| { s.borrow_mut().transcription.translate_to_english = a; save(&s); }), false, false, 0);

    vbox.pack_start(&Separator::new(Orientation::Horizontal), false, false, 8);
    let auto_header = Label::new(None);
    auto_header.set_markup("<b>Automatic Transcription</b>");
    auto_header.set_halign(gtk3::Align::Start);
    vbox.pack_start(&auto_header, false, false, 0);

    let s = settings.clone();
    vbox.pack_start(&switch_row("Enable Auto Transcription", "Automatically start recording when another app uses the microphone", settings.borrow().auto_transcription.enabled, move |a| { s.borrow_mut().auto_transcription.enabled = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Voice Activity Detection", "Auto-stop recording after silence (disable for meetings with long pauses)", settings.borrow().auto_transcription.vad_enabled, move |a| { s.borrow_mut().auto_transcription.vad_enabled = a; save(&s); }), false, false, 0);

    let silence_row = GtkBox::new(Orientation::Horizontal, 8);
    silence_row.set_tooltip_text(Some("Seconds of silence before auto-stopping recording"));
    silence_row.pack_start(&Label::new(Some("Silence timeout (seconds):")), false, false, 0);
    let silence_spin = gtk3::SpinButton::with_range(5.0, 120.0, 5.0);
    silence_spin.set_value(settings.borrow().auto_transcription.silence_seconds as f64);
    let s = settings.clone();
    silence_spin.connect_value_changed(move |spin| { s.borrow_mut().auto_transcription.silence_seconds = spin.value() as u32; save(&s); });
    silence_row.pack_start(&silence_spin, false, false, 0);
    vbox.pack_start(&silence_row, false, false, 0);

    vbox
}

fn build_refinement_tab(settings: &Rc<RefCell<Settings>>) -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let s = settings.clone();
    vbox.pack_start(&switch_row("Text Refinement", "Clean up transcribed text after recognition", settings.borrow().refinement.enabled, move |a| { s.borrow_mut().refinement.enabled = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Remove fillers", "Remove um, uh, like, and other filler words", settings.borrow().refinement.remove_fillers, move |a| { s.borrow_mut().refinement.remove_fillers = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Fix capitalization", "Correct capitalization and punctuation", settings.borrow().refinement.fix_capitalization, move |a| { s.borrow_mut().refinement.fix_capitalization = a; save(&s); }), false, false, 0);

    vbox.pack_start(&Separator::new(Orientation::Horizontal), false, false, 4);
    vbox.pack_start(&Label::new(Some("Custom Vocabulary (one per line):")), false, false, 0);
    let tv = TextView::new();
    tv.buffer().unwrap().set_text(&settings.borrow().refinement.custom_vocabulary.join("\n"));
    let s = settings.clone();
    tv.buffer().unwrap().connect_changed(move |buf| {
        let text = buf.text(&buf.start_iter(), &buf.end_iter(), false).unwrap_or_default().to_string();
        s.borrow_mut().refinement.custom_vocabulary = text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        save(&s);
    });
    vbox.pack_start(&tv, true, true, 0);

    vbox
}

fn build_appearance_tab(settings: &Rc<RefCell<Settings>>) -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 12);
    vbox.set_margin_top(16);
    vbox.set_margin_bottom(16);
    vbox.set_margin_start(16);
    vbox.set_margin_end(16);

    let s = settings.clone();
    vbox.pack_start(&switch_row("Pill Indicator", "Show a floating pill overlay when recording", settings.borrow().appearance.pill_enabled, move |a| { s.borrow_mut().appearance.pill_enabled = a; save(&s); }), false, false, 0);
    let s = settings.clone();
    vbox.pack_start(&switch_row("Top Bar Indicator", "Show a colored bar below the GNOME panel when recording", settings.borrow().appearance.topbar_enabled, move |a| { s.borrow_mut().appearance.topbar_enabled = a; save(&s); }), false, false, 0);

    let color_row = GtkBox::new(Orientation::Horizontal, 8);
    color_row.set_tooltip_text(Some("Accent color for the recording indicator"));
    let color_label = Label::new(Some("Color:"));
    // Easter egg: 5 clicks on "Color:" label unlocks Rainbow
    let color_event_box = gtk3::EventBox::new();
    color_event_box.add(&color_label);
    let click_count = Rc::new(RefCell::new(0u32));
    let last_click = Rc::new(RefCell::new(std::time::Instant::now()));
    let store_for_egg: Rc<RefCell<Option<gtk3::ListStore>>> = Rc::new(RefCell::new(None));
    let s_egg = settings.clone();
    let store_ref = store_for_egg.clone();
    color_event_box.connect_button_press_event(move |_, _| {
        let now = std::time::Instant::now();
        if now.duration_since(*last_click.borrow()) > std::time::Duration::from_secs(2) {
            *click_count.borrow_mut() = 0;
        }
        *last_click.borrow_mut() = now;
        *click_count.borrow_mut() += 1;
        if *click_count.borrow() >= 5 {
            if !s_egg.borrow().general.rainbow_unlocked {
                s_egg.borrow_mut().general.rainbow_unlocked = true;
                save(&s_egg);
                if let Some(ref store) = *store_ref.borrow() {
                    store.set(&store.append(), &[(0, &"rainbow".to_string()), (1, &"🌈 rainbow".to_string())]);
                }
            }
            *click_count.borrow_mut() = 0;
        }
        glib::Propagation::Proceed
    });
    color_row.pack_start(&color_event_box, false, false, 0);
    let color_store = gtk3::ListStore::new(&[glib::Type::STRING, glib::Type::STRING]);
    for (id, hex) in &[
        ("teal", "#14b8a6"), ("blue", "#3b82f6"), ("purple", "#8b5cf6"),
        ("pink", "#ec4899"), ("orange", "#f97316"), ("green", "#22c55e"), ("yellow", "#eab308"),
    ] {
        let markup = format!("<span foreground='{}'><b>{}</b></span>", hex, id);
        color_store.set(&color_store.append(), &[(0, &id.to_string()), (1, &markup)]);
    }
    if settings.borrow().general.rainbow_unlocked {
        color_store.set(&color_store.append(), &[(0, &"rainbow".to_string()), (1, &"🌈 rainbow".to_string())]);
    }
    let color_combo = gtk3::ComboBox::with_model(&color_store);
    let renderer = gtk3::CellRendererText::new();
    gtk3::prelude::CellLayoutExt::pack_start(&color_combo, &renderer, true);
    gtk3::prelude::CellLayoutExt::add_attribute(&color_combo, &renderer, "markup", 1);
    // Set active to current color
    let current_color = settings.borrow().appearance.accent_color.clone();
    let mut active_idx: u32 = 0;
    let iter = color_store.iter_first();
    if let Some(ref it) = iter {
        let mut i = 0u32;
        loop {
            let val: String = color_store.value(it, 0).get().unwrap_or_default();
            if val == current_color { active_idx = i; break; }
            i += 1;
            if !color_store.iter_next(it) { break; }
        }
    }
    color_combo.set_active(Some(active_idx));
    *store_for_egg.borrow_mut() = Some(color_store.clone());
    let s = settings.clone();
    color_combo.connect_changed(move |c| {
        if let Some(iter) = c.active_iter() {
            if let Some(model) = c.model() {
                let id: String = model.value(&iter, 0).get().unwrap_or_default();
                s.borrow_mut().appearance.accent_color = id;
                save(&s);
            }
        }
    });
    color_row.pack_start(&color_combo, false, false, 0);
    vbox.pack_start(&color_row, false, false, 0);

    vbox
}

fn switch_row(label: &str, tooltip: &str, active: bool, on_change: impl Fn(bool) + 'static) -> GtkBox {
    let row = GtkBox::new(Orientation::Horizontal, 8);
    row.set_tooltip_text(Some(tooltip));
    let lbl = Label::new(Some(label));
    row.pack_start(&lbl, true, true, 0);
    let switch = Switch::new();
    switch.set_active(active);
    switch.connect_state_set(move |_, state| { on_change(state); glib::Propagation::Proceed });
    row.pack_end(&switch, false, false, 0);
    row
}

pub fn capitalize_key(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "space" => "Space".into(),
        "return" => "Enter".into(),
        "tab" => "Tab".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "escape" => "Escape".into(),
        k if k.len() == 1 => k.to_uppercase(),
        k if k.starts_with("f") && k[1..].parse::<u32>().is_ok() => k.to_uppercase(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(first) => first.to_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        }
    }
}
