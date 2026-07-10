// SPDX-License-Identifier: Apache-2.0
use gtk3::prelude::*;
use gtk3::{self, Window, WindowType, Box as GtkBox, Button, Label, ListBox, ListBoxRow, Orientation, ScrolledWindow, Separator};

use crate::history::{HistoryEntry, HistoryStore};

pub fn show() {
    glib::MainContext::default().invoke(|| {
        let window = Window::new(WindowType::Toplevel);
        window.set_title("FrogScribe — Transcription History");
        window.set_default_size(500, 450);

        let vbox = GtkBox::new(Orientation::Vertical, 0);

        let header = GtkBox::new(Orientation::Horizontal, 8);
        header.set_margin_top(12);
        header.set_margin_bottom(8);
        header.set_margin_start(12);
        header.set_margin_end(12);
        let title = Label::new(None);
        title.set_markup("<b>Transcription History</b>");
        header.pack_start(&title, true, true, 0);

        let clear_btn = Button::with_label("Clear All");
        let w_ref = window.clone();
        clear_btn.connect_clicked(move |_| {
            if let Ok(mut store) = HistoryStore::new() { let _ = store.clear(); }
            w_ref.close();
        });
        header.pack_end(&clear_btn, false, false, 0);
        vbox.pack_start(&header, false, false, 0);
        vbox.pack_start(&Separator::new(Orientation::Horizontal), false, false, 0);

        let scrolled = ScrolledWindow::new(gtk3::Adjustment::NONE, gtk3::Adjustment::NONE);
        let list_box = ListBox::new();
        list_box.set_selection_mode(gtk3::SelectionMode::None);

        if let Ok(store) = HistoryStore::new() {
            let mut entries: Vec<_> = store.entries().to_vec();
            entries.reverse();
            if entries.is_empty() {
                let empty = Label::new(Some("No transcriptions yet."));
                empty.set_margin_top(24);
                empty.set_opacity(0.5);
                list_box.add(&empty);
            } else {
                for entry in &entries {
                    list_box.add(&build_history_row(entry));
                }
            }
        }

        scrolled.add(&list_box);
        vbox.pack_start(&scrolled, true, true, 0);

        window.add(&vbox);
        window.show_all();
    });
}

fn build_history_row(entry: &HistoryEntry) -> ListBoxRow {
    let row = ListBoxRow::new();
    let vbox = GtkBox::new(Orientation::Vertical, 4);
    vbox.set_margin_top(8);
    vbox.set_margin_bottom(8);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);

    // Timestamp + duration header
    let meta_box = GtkBox::new(Orientation::Horizontal, 8);
    let ts_label = Label::new(None);
    let human_ts = entry.timestamp.parse::<i64>().ok()
        .and_then(|epoch| glib::DateTime::from_unix_local(epoch).ok())
        .and_then(|dt| dt.format("%b %d, %Y  %H:%M").ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| entry.timestamp.clone());
    ts_label.set_markup(&format!("<small>{}</small>", glib::markup_escape_text(&human_ts)));
    ts_label.set_halign(gtk3::Align::Start);
    ts_label.set_opacity(0.6);
    meta_box.pack_start(&ts_label, true, true, 0);

    let dur_label = Label::new(None);
    dur_label.set_markup(&format!("<small>{:.1}s · {}</small>", entry.duration_secs, entry.model));
    dur_label.set_opacity(0.6);
    meta_box.pack_end(&dur_label, false, false, 0);
    vbox.pack_start(&meta_box, false, false, 0);

    // Text
    let text = if entry.text.len() > 200 { format!("{}…", &entry.text[..200]) } else { entry.text.clone() };
    let text_label = Label::new(Some(&text));
    text_label.set_halign(gtk3::Align::Start);
    text_label.set_line_wrap(true);
    text_label.set_max_width_chars(60);
    text_label.set_selectable(true);
    vbox.pack_start(&text_label, false, false, 0);

    // Copy button
    let btn_box = GtkBox::new(Orientation::Horizontal, 4);
    let copy_btn = Button::with_label("📋 Copy");
    let entry_text = entry.text.clone();
    copy_btn.connect_clicked(move |_| {
        if let Ok(mut clip) = arboard::Clipboard::new() { let _ = clip.set_text(&entry_text); }
    });
    btn_box.pack_end(&copy_btn, false, false, 0);
    vbox.pack_start(&btn_box, false, false, 0);

    vbox.pack_start(&Separator::new(Orientation::Horizontal), false, false, 4);

    row.add(&vbox);
    row
}
