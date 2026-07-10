// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Recording indicator overlay window (GTK3).
//! Supports Pill and TopBar placement, Ghost and Classic styles,
//! 7 accent colors, full/mini display modes, fade-in/fade-out,
//! click-to-toggle mode, non-activating (doesn't steal focus).

use gtk3::prelude::*;
use gtk3::{self, Window, WindowType, Box as GtkBox, Label, Orientation, CssProvider, StyleContext, DrawingArea};
use gtk3::gdk;
use gtk3::cairo;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

// gtk-layer-shell FFI for Wayland top-edge anchoring
mod layer_shell {
    use gtk3::Window;
    use glib::ObjectType;

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

    // Run on GTK3 main loop (shared with tray)
    let vis2 = vis.clone();
    let md2 = md.clone();
    glib::MainContext::default().invoke(move || {
        // This closure runs once on the GTK main thread
        let is_topbar = placement == "TopBar";
        let is_ghost = style == "Ghost";

        let window = Window::new(WindowType::Toplevel);
        window.set_decorated(false);
        window.set_accept_focus(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);

        let (full_width, height) = indicator_dimensions(&placement);

        if is_topbar {
            let supported = layer_shell::is_supported();
            tracing::info!("TopBar mode: layer_shell_supported={}", supported);
            if supported {
                layer_shell::init_top_bar(&window, height);
                window.set_size_request(-1, height);
            } else {
                // GNOME fallback: position before realize using gravity
                window.set_default_size(full_width, height);
                window.set_gravity(gdk::Gravity::North);
                window.set_position(gtk3::WindowPosition::None);
            }
        } else {
            tracing::info!("Pill mode: using Notification hint");
            window.set_type_hint(gdk::WindowTypeHint::Notification);
            window.set_keep_above(true);
            window.set_default_size(full_width, height);
        }

        // Enable RGBA for translucency
        if let Some(screen) = gdk::Screen::default() {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
            }
        }
        window.set_app_paintable(true);

        // Paint background with cairo (reliable on Wayland + RGBA)
        let bg_hex = accent_hex(&color).to_string();
        let is_rainbow = color == "rainbow";
        let is_topbar_draw = is_topbar;
        let draw_start = std::time::Instant::now();
        window.connect_draw(move |_win, cr| {
            cr.set_operator(cairo::Operator::Source);
            let alloc = _win.allocation();
            let w = alloc.width() as f64;
            let h = alloc.height() as f64;

            // Guard against invalid dimensions during initial allocation
            if w < 2.0 || h < 2.0 {
                return glib::Propagation::Proceed;
            }

            if is_rainbow && !is_topbar_draw {
                // Pill: cycle through rainbow colors over time
                let elapsed = draw_start.elapsed().as_secs_f64();
                let hue = (elapsed * 30.0) % 360.0; // cycle every 12 seconds
                let (r, g, b) = hue_to_rgb(hue);
                cr.set_source_rgba(r, g, b, 0.8);
                let radius = h / 2.0;
                cr.new_sub_path();
                cr.arc(w - radius, radius, radius, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
                cr.arc(radius, radius, radius, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
                cr.close_path();
                let _ = cr.fill();
            } else {
                let (r, g, b) = hex_to_rgb(&bg_hex);
                if is_topbar_draw {
                    cr.set_source_rgba(r, g, b, 0.85);
                } else {
                    cr.set_source_rgba(r, g, b, 0.8);
                }
                if !is_topbar_draw {
                    let radius = h / 2.0;
                    cr.new_sub_path();
                    cr.arc(w - radius, radius, radius, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
                    cr.arc(radius, radius, radius, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
                    cr.close_path();
                    let _ = cr.fill();
                } else {
                    cr.rectangle(0.0, 0.0, w, h);
                    let _ = cr.fill();
                }
            }
            glib::Propagation::Proceed
        });

        // CSS for text labels
        let label_css = CssProvider::new();
        let accent = accent_hex(&color);
        let _ = label_css.load_from_data(format!(
            ".ghost-label {{ color: white; font-weight: bold; font-size: 14px; padding: 8px 16px; }}
             .classic-label {{ color: white; font-size: 13px; padding: 8px 12px; }}
             .recording-dot {{ color: {}; font-size: 18px; }}", accent
        ).as_bytes());
        if let Some(screen) = gdk::Screen::default() {
            StyleContext::add_provider_for_screen(&screen, &label_css, gtk3::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }

        // Build content based on style
        let content = GtkBox::new(Orientation::Horizontal, 8);
        content.set_halign(gtk3::Align::Center);
        content.set_valign(gtk3::Align::Center);

        if !is_topbar {
            if is_ghost {
                let label = Label::new(Some("🎙 Recording..."));
                label.style_context().add_class("ghost-label");
                content.pack_start(&label, false, false, 0);
            } else {
                // Classic: dot + text + waveform
                let dot = Label::new(Some("●"));
                dot.style_context().add_class("recording-dot");
                content.pack_start(&dot, false, false, 4);

                let label = Label::new(Some("Recording"));
                label.style_context().add_class("classic-label");
                content.pack_start(&label, false, false, 0);

                // Waveform visualization
                let waveform = DrawingArea::new();
                waveform.set_size_request(80, 28);
                let _col = color.clone();
                waveform.connect_draw(move |_area, cr| {
                    draw_waveform(cr, 80.0, 28.0);
                    glib::Propagation::Proceed
                });
                content.pack_start(&waveform, false, false, 4);

                // Animate waveform by queuing redraws
                let wf = waveform.clone();
                glib::timeout_add_local(Duration::from_millis(100), move || {
                    wf.queue_draw();
                    glib::ControlFlow::Continue
                });
            }
        }

        window.add(&content);

        // Fade in: start at 0 opacity
        window.set_opacity(0.0);
        window.show_all();

        // Position at top center of screen (Pill only; TopBar uses layer_shell)
        if !is_topbar {
            if let Some(screen) = gtk3::prelude::WidgetExt::screen(&window) {
                let display = screen.display();
                let monitor = display.primary_monitor().unwrap_or_else(|| display.monitor(0).unwrap());
                let geom = monitor.geometry();
                let (x, y) = indicator_position(&placement, geom.x(), geom.y(), geom.width(), full_width);
                window.move_(x, y);
            }
        } else if !layer_shell::is_supported() {
            // GNOME fallback: move and resize before show
            if let Some(screen) = gtk3::prelude::WidgetExt::screen(&window) {
                let display = screen.display();
                let monitor = display.primary_monitor().unwrap_or_else(|| display.monitor(0).unwrap());
                let geom = monitor.geometry();
                window.resize(geom.width(), height);
                window.move_(geom.x(), geom.y());
            }
        }

        // Fade-in animation
        let win_fade = window.clone();
        let _is_topbar_anim = is_topbar;
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

        // Rainbow: continuous redraw for color cycling
        if is_rainbow {
            let win_rainbow = window.clone();
            let vis_rainbow = vis.clone();
            glib::timeout_add_local(Duration::from_millis(100), move || {
                if !vis_rainbow.load(Ordering::Relaxed) {
                    win_rainbow.hide();
                    return glib::ControlFlow::Break;
                }
                if !win_rainbow.is_visible() {
                    return glib::ControlFlow::Break;
                }
                win_rainbow.queue_draw();
                glib::ControlFlow::Continue
            });
        }

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
        window.connect_button_press_event(move |_, _| {
            let current = md2.load(Ordering::Relaxed);
            md2.store(if current == MODE_FULL { MODE_MINI } else { MODE_FULL }, Ordering::Relaxed);
            glib::Propagation::Proceed
        });

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
                win_poll.hide();
                return glib::ControlFlow::Break;
            }

            // Handle mode toggle (resize)
            let current_mode = md.load(Ordering::Relaxed);
            let target_width = if current_mode == MODE_MINI { 100 } else { full_width };
            if !is_topbar {
                win_poll.set_keep_above(true);
                win_poll.resize(target_width, height);
                if let Some(screen) = gtk3::prelude::WidgetExt::screen(&win_poll) {
                    let display = screen.display();
                    let monitor = display.primary_monitor().unwrap_or_else(|| display.monitor(0).unwrap());
                    let geom = monitor.geometry();
                    let x = (geom.width() - target_width) / 2;
                    win_poll.move_(x, 30);
                }
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

/// Draw animated waveform bars
fn draw_waveform(cr: &cairo::Context, width: f64, height: f64) {
    let mid = height / 2.0;
    let bars = 12;
    let bar_width = width / (bars as f64 * 2.0);

    cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);

    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis();

    for i in 0..bars {
        let phase = ((seed as f64 / 100.0) + i as f64 * 0.7).sin().abs();
        let bar_h = 4.0 + phase * (mid - 4.0);
        let x = i as f64 * bar_width * 2.0 + bar_width * 0.5;
        let y = mid - bar_h;
        cr.rectangle(x, y, bar_width, bar_h * 2.0);
    }
    let _ = cr.fill();
}
