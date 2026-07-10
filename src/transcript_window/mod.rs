use gtk3::prelude::*;
use gtk3::{self, Window, WindowType, Box as GtkBox, Button, Label, Orientation, ScrolledWindow, Separator, TextView};
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
        let window = Window::new(WindowType::Toplevel);
        window.set_title("FrogScribe — Long-Form Dictation");
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
        processing_label.set_no_show_all(true);
        header.pack_start(&dot, false, false, 0);
        header.pack_start(&processing_label, false, false, 8);
        let timer = Label::new(Some("0:00:00"));
        header.pack_end(&timer, false, false, 0);
        vbox.pack_start(&header, false, false, 4);
        vbox.pack_start(&Separator::new(Orientation::Horizontal), false, false, 0);

        let scrolled = ScrolledWindow::new(gtk3::Adjustment::NONE, gtk3::Adjustment::NONE);
        let tv = TextView::new();
        tv.set_editable(false);
        tv.set_wrap_mode(gtk3::WrapMode::Word);
        tv.set_top_margin(8);
        tv.set_left_margin(12);
        tv.set_right_margin(12);
        scrolled.add(&tv);
        vbox.pack_start(&scrolled, true, true, 0);

        vbox.pack_start(&Separator::new(Orientation::Horizontal), false, false, 0);

        let toolbar = GtkBox::new(Orientation::Horizontal, 8);
        toolbar.set_margin_top(8);
        toolbar.set_margin_bottom(8);
        toolbar.set_margin_start(12);
        toolbar.set_margin_end(12);

        let stop_btn = Button::with_label("⏹ Stop");
        let cb = callbacks.on_stop.clone();
        stop_btn.connect_clicked(move |_| (cb)());
        toolbar.pack_start(&stop_btn, false, false, 0);

        let copy_btn = Button::with_label("📋 Copy All");
        let st2 = st.clone();
        let cb = callbacks.on_copy_all.clone();
        copy_btn.connect_clicked(move |_| {
            if let Ok(mut clip) = arboard::Clipboard::new() { let _ = clip.set_text(&st2.full_text()); }
            (cb)();
        });
        toolbar.pack_start(&copy_btn, false, false, 0);

        let save_btn = Button::with_label("💾 Save");
        let st_save = st.clone();
        save_btn.connect_clicked(move |btn| {
            let text = st_save.full_text();
            if text.is_empty() { return; }
            let dialog = gtk3::FileChooserDialog::with_buttons(
                Some("Save Transcript"),
                btn.toplevel().and_then(|w| w.downcast::<Window>().ok()).as_ref(),
                gtk3::FileChooserAction::Save,
                &[("Cancel", gtk3::ResponseType::Cancel), ("Save", gtk3::ResponseType::Accept)],
            );
            let dt = glib::DateTime::now_local().unwrap();
            let fname = format!("transcript-{}.txt", dt.format("%Y%m%d-%H%M%S").unwrap_or_else(|_| glib::GString::from("unknown")));
            dialog.set_current_name(&fname);
            let filter = gtk3::FileFilter::new();
            filter.add_pattern("*.txt");
            filter.set_name(Some("Text files"));
            dialog.add_filter(filter);
            if dialog.run() == gtk3::ResponseType::Accept {
                if let Some(path) = dialog.filename() {
                    let save_content = if let Ok(s) = crate::settings::Settings::load() {
                        if s.general.context_header {
                            let now = glib::DateTime::now_local()
                                .and_then(|d| d.format("%Y-%m-%d %H:%M:%S"))
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| "unknown".to_string());
                            format!("---\nTimestamp: {}\nSource: frogscribe (long-form)\n---\n\n{}", now, text)
                        } else { text.clone() }
                    } else { text.clone() };
                    let _ = std::fs::write(&path, &save_content);
                }
            }
            unsafe { dialog.destroy(); }
        });
        toolbar.pack_start(&save_btn, false, false, 0);

        let new_btn = Button::with_label("🔄 New");
        let cb = callbacks.on_start_new.clone();
        new_btn.connect_clicked(move |_| (cb)());
        toolbar.pack_start(&new_btn, false, false, 0);

        let done_btn = Button::with_label("Done");
        let cb = callbacks.on_done.clone();
        let w = window.clone();
        done_btn.connect_clicked(move |_| { (cb)(); w.close(); });
        toolbar.pack_end(&done_btn, false, false, 0);

        vbox.pack_start(&toolbar, false, false, 0);
        window.add(&vbox);
        window.show_all();

        let st3 = st.clone();
        glib::timeout_add_local(Duration::from_millis(500), move || {
            let e = st3.elapsed_secs();
            timer.set_text(&format!("{}:{:02}:{:02}", e/3600, (e%3600)/60, e%60));
            let current = st3.full_text();
            if let Some(buf) = tv.buffer() {
                let existing = buf.text(&buf.start_iter(), &buf.end_iter(), false).map(|s| s.to_string()).unwrap_or_default();
                if current != existing { buf.set_text(&current); }
            }
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
