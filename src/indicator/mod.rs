// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Recording indicator overlay window (GTK4).
//! Supports Pill and TopBar placement, Ghost and Classic styles,
//! 7 accent colors, full/mini display modes, fade-in/fade-out,
//! click-to-toggle mode, non-activating (doesn't steal focus).

use gtk4::prelude::*;
use gtk4::{self, Window, Box as GtkBox, Label, Orientation, CssProvider};
use gtk4::gdk;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

// gtk-layer-shell FFI for Wayland top-edge anchoring
mod layer_shell {
    use gtk4::Window;
    use gtk4::prelude::ObjectType;

    #[repr(C)]
    #[allow(dead_code)]
    pub enum Edge { Left = 0, Right = 1, Top = 2, Bottom = 3 }

    #[repr(C)]
    #[allow(dead_code)]
    pub enum Layer { Background = 0, Bottom = 1, Top = 2, Overlay = 3 }

    extern "C" {
        fn gtk_layer_init_for_window(window: *mut std::ffi::c_void);
        fn gtk_layer_set_anchor(window: *mut std::ffi::c_void, edge: Edge, anchor: i32);
        fn gtk_layer_set_layer(window: *mut std::ffi::c_void, layer: Layer);
        fn gtk_layer_set_exclusive_zone(window: *mut std::ffi::c_void, zone: i32);
        fn gtk_layer_set_namespace(window: *mut std::ffi::c_void, ns: *const std::ffi::c_char);
        fn gtk_layer_is_supported() -> i32;
    }

    pub fn is_supported() -> bool {
        unsafe { gtk_layer_is_supported() != 0 }
    }

    pub fn init_top_bar(window: &Window, _height: i32) {
        unsafe {
            let ptr = window.as_ptr() as *mut std::ffi::c_void;
            gtk_layer_init_for_window(ptr);
            gtk_layer_set_layer(ptr, Layer::Top);
            gtk_layer_set_anchor(ptr, Edge::Top, 1);
            gtk_layer_set_anchor(ptr, Edge::Left, 1);
            gtk_layer_set_anchor(ptr, Edge::Right, 1);
            gtk_layer_set_anchor(ptr, Edge::Bottom, 0);
            gtk_layer_set_exclusive_zone(ptr, 0); // don't push other windows down
            let ns = std::ffi::CString::new("frogscribe-indicator").unwrap();
            gtk_layer_set_namespace(ptr, ns.as_ptr());
        }
    }
}

/// Display mode: Full shows text + waveform, Mini shows just a dot
const MODE_FULL: u8 = 0;
const MODE_MINI: u8 = 1;

pub struct IndicatorHandle {
    pub visible: Arc<AtomicBool>,
    pub mode: Arc<AtomicU8>,
}

impl IndicatorHandle {
    pub fn hide(&self) {
        self.visible.store(false, Ordering::Relaxed);
    }

    pub fn toggle_mode(&self) {
        let current = self.mode.load(Ordering::Relaxed);
        self.mode.store(if current == MODE_FULL { MODE_MINI } else { MODE_FULL }, Ordering::Relaxed);
    }
}

/// Show the recording indicator. Returns a handle to control it.
pub fn show(placement: &str, style: &str, color: &str) -> IndicatorHandle {
    let visible = Arc::new(AtomicBool::new(true));
    let mode = Arc::new(AtomicU8::new(MODE_FULL));
    let vis = visible.clone();
    let md = mode.clone();
    let color = color.to_string();
    let style = style.to_string();
    let placement = placement.to_string();

    // Run on GTK4 main loop (shared with tray)
    let vis2 = vis.clone();
    let md2 = md.clone();
    glib::MainContext::default().invoke(move || {
        // This closure runs once on the GTK main thread
        let is_topbar = placement == "TopBar";
        let is_ghost = style == "Ghost";

        let window = Window::new();
        window.set_decorated(false);

        let (full_width, height) = indicator_dimensions(&placement);

        if is_topbar {
            let supported = layer_shell::is_supported();
            tracing::info!("TopBar mode: layer_shell_supported={}", supported);
            if supported {
                layer_shell::init_top_bar(&window, height);
                window.set_size_request(-1, height);
            } else {
                // GNOME fallback: set default size
                window.set_default_size(full_width, height);
            }
        } else {
            tracing::info!("Pill mode");
            window.set_default_size(full_width, height);
        }

        // CSS styling for background and labels
        let accent = accent_hex(&color);
        let css_text = if is_topbar {
            format!(
                "window {{ background-color: alpha({}, 0.85); }}\n\
                 .ghost-label {{ color: white; font-weight: bold; font-size: 14px; padding: 8px 16px; }}\n\
                 .classic-label {{ color: white; font-size: 13px; padding: 8px 12px; }}\n\
                 .recording-dot {{ color: {}; font-size: 18px; }}", accent, accent
            )
        } else if is_ghost {
            format!(
                "window {{ background-color: alpha({}, 0.8); border-radius: 22px; }}\n\
                 .ghost-label {{ color: white; font-weight: bold; font-size: 14px; padding: 8px 16px; }}", accent
            )
        } else {
            format!(
                "window {{ background-color: alpha(#1a1a2e, 0.85); border-radius: 12px; }}\n\
                 .classic-label {{ color: white; font-size: 13px; padding: 8px 12px; }}\n\
                 .recording-dot {{ color: {}; font-size: 18px; }}", accent
            )
        };

        let css_provider = CssProvider::new();
        css_provider.load_from_data(&css_text);
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(&display, &css_provider, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }

        // Build content based on style
        let content = GtkBox::new(Orientation::Horizontal, 8);
        content.set_halign(gtk4::Align::Center);
        content.set_valign(gtk4::Align::Center);

        if !is_topbar {
            if is_ghost {
                let label = Label::new(Some("🎙 Recording..."));
                label.add_css_class("ghost-label");
                content.append(&label);
            } else {
                // Classic: dot + text
                let dot = Label::new(Some("●"));
                dot.add_css_class("recording-dot");
                content.append(&dot);

                let label = Label::new(Some("Recording"));
                label.add_css_class("classic-label");
                content.append(&label);
            }
        }

        window.set_child(Some(&content));

        // Start with 0 opacity for fade-in
        window.set_opacity(0.0);
        window.present();

        // Fade-in animation
        let win_fade = window.clone();
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let current = win_fade.opacity();
            if current < 0.95 {
                win_fade.set_opacity(current + 0.1);
                glib::ControlFlow::Continue
            } else {
                win_fade.set_opacity(1.0);
                glib::ControlFlow::Break
            }
        });

        // Pulse animation for TopBar (subtle opacity breathing)
        if is_topbar {
            let win_pulse = window.clone();
            let vis_pulse = vis.clone();
            let start = std::time::Instant::now();
            glib::timeout_add_local(Duration::from_millis(50), move || {
                if !vis_pulse.load(Ordering::Relaxed) {
                    return glib::ControlFlow::Break;
                }
                let elapsed = start.elapsed().as_secs_f64();
                let pulse = 0.7 + 0.3 * (elapsed * 2.5).sin().abs();
                win_pulse.set_opacity(pulse);
                glib::ControlFlow::Continue
            });
        }

        // Click to toggle display mode
        let gesture = gtk4::GestureClick::new();
        let md2_click = md2.clone();
        gesture.connect_pressed(move |_, _n_press, _, _| {
            let current = md2_click.load(Ordering::Relaxed);
            md2_click.store(if current == MODE_FULL { MODE_MINI } else { MODE_FULL }, Ordering::Relaxed);
        });
        window.add_controller(gesture);

        // Poll visibility and mode changes
        let win_poll = window.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            if !vis2.load(Ordering::Relaxed) {
                // Fade out
                let opacity = win_poll.opacity();
                if opacity > 0.05 {
                    win_poll.set_opacity(opacity - 0.15);
                    return glib::ControlFlow::Continue;
                }
                win_poll.set_visible(false);
                return glib::ControlFlow::Break;
            }

            // Handle mode toggle (resize)
            let current_mode = md.load(Ordering::Relaxed);
            let target_width = if current_mode == MODE_MINI { 100 } else { full_width };
            if !is_topbar {
                win_poll.set_default_size(target_width, height);
            }

            glib::ControlFlow::Continue
        });
    });

    IndicatorHandle { visible, mode }
}

/// Hide the indicator (triggers fade-out)
pub fn hide(handle: &IndicatorHandle) {
    handle.visible.store(false, Ordering::Relaxed);
}

fn accent_hex(color: &str) -> &str {
    match color {
        "teal" => "#14b8a6",
        "blue" => "#3b82f6",
        "purple" => "#8b5cf6",
        "pink" => "#ec4899",
        "orange" => "#f97316",
        "green" => "#22c55e",
        "yellow" => "#eab308",
        "rainbow" => "#14b8a6", // fallback for non-draw contexts
        _ => "#14b8a6",
    }
}

fn hex_to_rgb(hex: &str) -> (f64, f64, f64) {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f64 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f64 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f64 / 255.0;
    (r, g, b)
}

/// Convert HSV hue (0-360) to RGB (0.0-1.0), full saturation/value
fn hue_to_rgb(hue: f64) -> (f64, f64, f64) {
    let h = hue / 60.0;
    let i = h.floor() as i32;
    let f = h - i as f64;
    let q = 1.0 - f;
    match i % 6 {
        0 => (1.0, f, 0.0),
        1 => (q, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, q, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, q),
    }
}

/// Returns (width, height) for the indicator window.
/// Width of 800 for TopBar is a default; it gets resized to screen width.
pub fn indicator_dimensions(placement: &str) -> (i32, i32) {
    if placement == "TopBar" { (800, 6) } else { (240, 44) }
}

/// Generate the CSS for the indicator window.
pub fn indicator_css(placement: &str, style: &str, color: &str) -> String {
    let bg = accent_hex(color);
    if placement == "TopBar" {
        format!("window {{ background-color: {}; border-radius: 3px; }}", bg)
    } else if style == "Ghost" {
        format!(
            "window {{ background-color: alpha({}, 0.7); border-radius: 22px; }}\n\
             .ghost-label {{ color: white; font-weight: bold; font-size: 14px; padding: 8px 16px; }}",
            bg
        )
    } else {
        format!(
            "window {{ background-color: alpha(#1a1a2e, 0.85); border-radius: 12px; }}\n\
             .classic-label {{ color: white; font-size: 13px; padding: 8px 12px; }}\n\
             .recording-dot {{ color: {}; font-size: 18px; }}",
            bg
        )
    }
}

/// Returns (x, y) position for the indicator given screen geometry.
pub fn indicator_position(placement: &str, screen_x: i32, screen_y: i32, screen_width: i32, window_width: i32) -> (i32, i32) {
    if placement == "TopBar" {
        (screen_x, screen_y)
    } else {
        ((screen_width - window_width) / 2, 30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accent_hex_known_colors() {
        assert_eq!(accent_hex("teal"), "#14b8a6");
        assert_eq!(accent_hex("blue"), "#3b82f6");
        assert_eq!(accent_hex("purple"), "#8b5cf6");
        assert_eq!(accent_hex("pink"), "#ec4899");
        assert_eq!(accent_hex("orange"), "#f97316");
        assert_eq!(accent_hex("green"), "#22c55e");
        assert_eq!(accent_hex("yellow"), "#eab308");
    }

    #[test]
    fn test_accent_hex_unknown_defaults_to_teal() {
        assert_eq!(accent_hex("invalid"), "#14b8a6");
        assert_eq!(accent_hex(""), "#14b8a6");
    }

    #[test]
    fn test_dimensions_topbar() {
        let (w, h) = indicator_dimensions("TopBar");
        assert_eq!(h, 6);
        assert!(w > 0);
    }

    #[test]
    fn test_dimensions_pill() {
        let (w, h) = indicator_dimensions("Pill");
        assert_eq!(w, 240);
        assert_eq!(h, 44);
    }

    #[test]
    fn test_css_topbar_contains_color() {
        let css = indicator_css("TopBar", "Ghost", "blue");
        assert!(css.contains("#3b82f6"), "TopBar CSS should contain the accent color");
        assert!(css.contains("border-radius: 3px"), "TopBar should have rounded corners");
    }

    #[test]
    fn test_css_topbar_no_ghost_label() {
        let css = indicator_css("TopBar", "Ghost", "blue");
        assert!(!css.contains("ghost-label"), "TopBar should not have ghost-label class");
    }

    #[test]
    fn test_css_pill_ghost() {
        let css = indicator_css("Pill", "Ghost", "purple");
        assert!(css.contains("#8b5cf6"));
        assert!(css.contains("ghost-label"));
        assert!(css.contains("border-radius: 22px"));
    }

    #[test]
    fn test_css_pill_classic() {
        let css = indicator_css("Pill", "Classic", "orange");
        assert!(css.contains("#f97316"));
        assert!(css.contains("recording-dot"));
        assert!(css.contains("#1a1a2e"));
    }

    #[test]
    fn test_position_topbar_uses_screen_origin() {
        let (x, y) = indicator_position("TopBar", 100, 50, 1920, 800);
        assert_eq!(x, 100);
        assert_eq!(y, 50);
    }

    #[test]
    fn test_position_pill_centered() {
        let (x, y) = indicator_position("Pill", 0, 0, 1920, 240);
        assert_eq!(x, (1920 - 240) / 2);
        assert_eq!(y, 30);
    }
}
