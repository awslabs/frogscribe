// SPDX-License-Identifier: Apache-2.0
//! Live Preview window — singleton tabbed window for streaming transcription.

use gtk4::prelude::*;
use gtk4::{self, Notebook, Window, Box as GtkBox, Button, Label, Orientation, ScrolledWindow, TextView};
use std::sync::{Arc, Mutex};
use std::cell::RefCell;

thread_local! {
    static WINDOW: RefCell<Option<Window>> = RefCell::new(None);
    static NOTEBOOK: RefCell<Option<Notebook>> = RefCell::new(None);
}

/// Open a new tab in the live preview window (creating the window if needed).
/// Returns an Arc<Mutex<String>> that the caller writes live text into.
pub fn open_tab(source: &str) -> Arc<Mutex<String>> {
    let live_text = Arc::new(Mutex::new(String::new()));
    let live_text_ui = live_text.clone();
    let source = source.to_string();

    glib::MainContext::default().invoke(move || {
        let need_new_window = WINDOW.with(|w| {
            w.borrow().as_ref().map_or(true, |win| !win.is_visible())
        });

        if need_new_window {
            let window = Window::new();
            window.set_title(Some("FrogScribe — Live Preview"));
            window.set_default_size(550, 300);

            let vbox = GtkBox::new(Orientation::Vertical, 0);
            let notebook = Notebook::new();
            notebook.set_scrollable(true);
            notebook.set_vexpand(true);
            vbox.append(&notebook);
            window.set_child(Some(&vbox));

            NOTEBOOK.with(|nb| *nb.borrow_mut() = Some(notebook.clone()));
            WINDOW.with(|w| *w.borrow_mut() = Some(window.clone()));
            window.present();
        }

        // Add a new tab
        NOTEBOOK.with(|nb| {
            if let Some(notebook) = nb.borrow().as_ref() {
                let tab_label = make_tab_label(&source);
                let tab_content = make_tab_content(live_text_ui, &source);
                notebook.append_page(&tab_content, Some(&tab_label));
                let n = notebook.n_pages();
                notebook.set_current_page(Some(n - 1));
            }
        });

        WINDOW.with(|w| {
            if let Some(win) = w.borrow().as_ref() {
                win.present();
            }
        });
    });

    live_text
}

fn make_tab_label(source: &str) -> GtkBox {
    let hbox = GtkBox::new(Orientation::Horizontal, 4);
    let dt = glib::DateTime::now_local()
        .and_then(|d| d.format("%H:%M:%S"))
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "new".to_string());
    let label_text = if source.is_empty() { dt } else { format!("{} {}", dt, source) };
    let label = Label::new(Some(&label_text));
    hbox.append(&label);
    hbox
}

fn make_tab_content(live_text: Arc<Mutex<String>>, source: &str) -> GtkBox {
    let vbox = GtkBox::new(Orientation::Vertical, 0);

    let tv = TextView::new();
    tv.set_editable(false);
    tv.set_wrap_mode(gtk4::WrapMode::Word);
    tv.set_top_margin(12);
    tv.set_bottom_margin(40);
    tv.set_left_margin(12);
    tv.set_right_margin(12);
    tv.buffer().set_text("Listening...");

    let scrolled = ScrolledWindow::new();
    scrolled.set_child(Some(&tv));
    scrolled.set_vexpand(true);
    vbox.append(&scrolled);

    // Toolbar
    let toolbar = GtkBox::new(Orientation::Horizontal, 8);
    toolbar.set_margin_top(4);
    toolbar.set_margin_bottom(4);
    toolbar.set_margin_start(8);
    toolbar.set_margin_end(8);

    let save_btn = Button::with_label("💾 Save");
    let tv_save = tv.clone();
    let source_for_save = source.to_string();
    save_btn.connect_clicked(move |btn| {
        let buf = tv_save.buffer();
        let (start, end) = (buf.start_iter(), buf.end_iter());
        let text = buf.text(&start, &end, false).to_string();
        if text.is_empty() || text == "Listening..." { return; }
        let save_content = if let Ok(s) = crate::settings::Settings::load() {
            if s.general.context_header {
                let now = glib::DateTime::now_local()
                    .and_then(|d| d.format("%Y-%m-%d %H:%M:%S"))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                format!("---\nTimestamp: {}\nSource: {}\n---\n\n{}", now, source_for_save, text)
            } else { text.clone() }
        } else { text.clone() };
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
        dialog.connect_response(move |d, response| {
            if response == gtk4::ResponseType::Accept {
                if let Some(file) = d.file() {
                    if let Some(path) = file.path() {
                        let _ = std::fs::write(&path, &save_content);
                    }
                }
            }
            d.close();
        });
        dialog.present();
    });
    save_btn.set_hexpand(true);
    save_btn.set_halign(gtk4::Align::End);
    toolbar.append(&save_btn);
    vbox.append(&toolbar);

    // Timer to update text
    let lt = live_text;
    let tv_update = tv.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        // Stop if the widget has been destroyed (tab closed or window closed)
        if !tv_update.is_realized() {
            return glib::ControlFlow::Break;
        }
        let text = lt.lock().unwrap().clone();
        let buf = tv_update.buffer();
        let display = if text.is_empty() { "Listening...".to_string() } else { text };
        let (start, end) = (buf.start_iter(), buf.end_iter());
        let existing = buf.text(&start, &end, false).to_string();
        if display != existing {
            buf.set_text(&display);
        }
        glib::ControlFlow::Continue
    });

    vbox
}
