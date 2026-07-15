// SPDX-License-Identifier: Apache-2.0
use gtk4::prelude::*;
use gtk4::{self, Window, Box as GtkBox, Button, Label, Orientation, ScrolledWindow, Separator, TextView};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

pub struct TranscriptWindowState {
    pub text: Arc<Mutex<String>>,
    pub is_active: Arc<AtomicBool>,
    pub is_processing: Arc<AtomicBool>,
    pub start_time: Instant,
}

impl TranscriptWindowState {
    pub fn new() -> Self {
        Self {
            text: Arc::new(Mutex::new(String::new())),
            is_active: Arc::new(AtomicBool::new(true)),
            is_processing: Arc::new(AtomicBool::new(false)),
            start_time: Instant::now(),
        }
    }
    pub fn append_text(&self, chunk: &str) {
        let mut t = self.text.lock().unwrap();
        if !t.is_empty() { t.push(' '); }
        t.push_str(chunk);
    }
    pub fn full_text(&self) -> String { self.text.lock().unwrap().clone() }
    pub fn elapsed_secs(&self) -> u64 { self.start_time.elapsed().as_secs() }
}

#[derive(Clone)]
pub struct TranscriptCallbacks {
    pub on_stop: Arc<dyn Fn() + Send + Sync>,
    pub on_copy_all: Arc<dyn Fn() + Send + Sync>,
    pub on_start_new: Arc<dyn Fn() + Send + Sync>,
    pub on_done: Arc<dyn Fn() + Send + Sync>,
}

pub fn show(state: Arc<TranscriptWindowState>, callbacks: TranscriptCallbacks) {
    let st = state.clone();
    glib::MainContext::default().invoke(move || {
        let window = Window::new();
        window.set_title(Some("FrogScribe — Long-Form Dictation"));
        window.set_default_size(500, 400);

        let vbox = GtkBox::new(Orientation::Vertical, 0);

        let header = GtkBox::new(Orientation::Horizontal, 8);
        header.set_margin_top(8);
        header.set_margin_start(12);
        header.set_margin_end(12);
        let dot = Label::new(None);
        dot.set_markup("<span foreground='red'>●</span> Recording");
        let processing_label = Label::new(None);
        processing_label.set_markup("<span foreground='#888'>⏳ Processing...</span>");
        processing_label.set_visible(false);
        header.append(&dot);
        header.append(&processing_label);
        let timer = Label::new(Some("0:00:00"));
        timer.set_hexpand(true);
        timer.set_halign(gtk4::Align::End);
        header.append(&timer);
        vbox.append(&header);
        vbox.append(&Separator::new(Orientation::Horizontal));

        let scrolled = ScrolledWindow::new();
        let tv = TextView::new();
        tv.set_editable(false);
        tv.set_wrap_mode(gtk4::WrapMode::Word);
        tv.set_top_margin(8);
        tv.set_left_margin(12);
        tv.set_right_margin(12);
        scrolled.set_child(Some(&tv));
        scrolled.set_vexpand(true);
        vbox.append(&scrolled);

        vbox.append(&Separator::new(Orientation::Horizontal));

        let toolbar = GtkBox::new(Orientation::Horizontal, 8);
        toolbar.set_margin_top(8);
        toolbar.set_margin_bottom(8);
        toolbar.set_margin_start(12);
        toolbar.set_margin_end(12);

        let stop_btn = Button::with_label("⏹ Stop");
        let cb = callbacks.on_stop.clone();
        stop_btn.connect_clicked(move |_| (cb)());
        toolbar.append(&stop_btn);

        let copy_btn = Button::with_label("📋 Copy All");
        let st2 = st.clone();
        let cb = callbacks.on_copy_all.clone();
        copy_btn.connect_clicked(move |_| {
            if let Ok(mut clip) = arboard::Clipboard::new() { let _ = clip.set_text(&st2.full_text()); }
            (cb)();
        });
        toolbar.append(&copy_btn);

        let save_btn = Button::with_label("💾 Save");
        let st_save = st.clone();
        save_btn.connect_clicked(move |btn| {
            let text = st_save.full_text();
            if text.is_empty() { return; }
            let parent_window = btn.root().and_then(|r| r.downcast::<Window>().ok());
            let dialog = gtk4::FileChooserDialog::new(
                Some("Save Transcript"),
                parent_window.as_ref(),
                gtk4::FileChooserAction::Save,
                &[("Cancel", gtk4::ResponseType::Cancel), ("Save", gtk4::ResponseType::Accept)],
            );
            let dt = glib::DateTime::now_local().unwrap();
            let fname = format!("transcript-{}.txt", dt.format("%Y%m%d-%H%M%S").unwrap_or_else(|_| glib::GString::from("unknown")));
            dialog.set_current_name(&fname);
            let filter = gtk4::FileFilter::new();
            filter.add_pattern("*.txt");
            filter.set_name(Some("Text files"));
            dialog.add_filter(&filter);
            let text_clone = text.clone();
            dialog.connect_response(move |d, response| {
                if response == gtk4::ResponseType::Accept {
                    if let Some(file) = d.file() {
                        if let Some(path) = file.path() {
                            let save_content = if let Ok(s) = crate::settings::Settings::load() {
                                if s.general.context_header {
                                    let now = glib::DateTime::now_local()
                                        .and_then(|d| d.format("%Y-%m-%d %H:%M:%S"))
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|_| "unknown".to_string());
                                    format!("---\nTimestamp: {}\nSource: frogscribe (long-form)\n---\n\n{}", now, text_clone)
                                } else { text_clone.clone() }
                            } else { text_clone.clone() };
                            let _ = std::fs::write(&path, &save_content);
                        }
                    }
                }
                d.close();
            });
            dialog.present();
        });
        toolbar.append(&save_btn);

        let new_btn = Button::with_label("🔄 New");
        let cb = callbacks.on_start_new.clone();
        new_btn.connect_clicked(move |_| (cb)());
        toolbar.append(&new_btn);

        let done_btn = Button::with_label("Done");
        let cb = callbacks.on_done.clone();
        let w = window.clone();
        done_btn.connect_clicked(move |_| { (cb)(); w.close(); });
        done_btn.set_hexpand(true);
        done_btn.set_halign(gtk4::Align::End);
        toolbar.append(&done_btn);

        vbox.append(&toolbar);
        window.set_child(Some(&vbox));
        window.present();

        let st3 = st.clone();
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let e = st3.elapsed_secs();
            timer.set_text(&format!("{}:{:02}:{:02}", e/3600, (e%3600)/60, e%60));
            let current = st3.full_text();
            let buf = tv.buffer();
            let (start, end) = (buf.start_iter(), buf.end_iter());
            let existing = buf.text(&start, &end, false).to_string();
            if current != existing { buf.set_text(&current); }
            // Processing indicator
            let processing = st3.is_processing.load(Ordering::Relaxed);
            processing_label.set_visible(processing);
            if !st3.is_active.load(Ordering::Relaxed) {
                dot.set_text("Session Complete");
                processing_label.set_visible(false);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    });
}
