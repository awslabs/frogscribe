// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use tokio::sync::mpsc;

use crate::settings::{ActivationMethod, HotkeyConfig};
use crate::AppEvent;

/// Minimum hold duration before recording starts (filters accidental taps)
pub const HOLD_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(200);

/// Parse a hotkey string like "Alt+Space", "Ctrl+Shift+R", "Super+Space" into modifier keys and a trigger key.
pub fn parse_hotkey(hotkey: &str) -> (Vec<Key>, Key) {
    let parts: Vec<&str> = hotkey.split('+').map(|s| s.trim()).collect();
    let mut modifiers = Vec::new();
    let trigger = parts.last().map(|k| str_to_key(k)).unwrap_or(Key::KEY_SPACE);

    for part in &parts[..parts.len().saturating_sub(1)] {
        match part.to_lowercase().as_str() {
            "alt" => modifiers.push(Key::KEY_LEFTALT),
            "ctrl" | "control" => modifiers.push(Key::KEY_LEFTCTRL),
            "shift" => modifiers.push(Key::KEY_LEFTSHIFT),
            "super" | "meta" => modifiers.push(Key::KEY_LEFTMETA),
            _ => {}
        }
    }

    (modifiers, trigger)
}

pub fn str_to_key(s: &str) -> Key {
    match s.to_lowercase().as_str() {
        "space" => Key::KEY_SPACE,
        "enter" | "return" => Key::KEY_ENTER,
        "tab" => Key::KEY_TAB,
        "0" => Key::KEY_0,
        "1" => Key::KEY_1,
        "2" => Key::KEY_2,
        "3" => Key::KEY_3,
        "4" => Key::KEY_4,
        "5" => Key::KEY_5,
        "6" => Key::KEY_6,
        "7" => Key::KEY_7,
        "8" => Key::KEY_8,
        "9" => Key::KEY_9,
        "a" => Key::KEY_A,
        "b" => Key::KEY_B,
        "c" => Key::KEY_C,
        "d" => Key::KEY_D,
        "e" => Key::KEY_E,
        "f" => Key::KEY_F,
        "g" => Key::KEY_G,
        "h" => Key::KEY_H,
        "i" => Key::KEY_I,
        "j" => Key::KEY_J,
        "k" => Key::KEY_K,
        "l" => Key::KEY_L,
        "m" => Key::KEY_M,
        "n" => Key::KEY_N,
        "o" => Key::KEY_O,
        "p" => Key::KEY_P,
        "q" => Key::KEY_Q,
        "r" => Key::KEY_R,
        "s" => Key::KEY_S,
        "t" => Key::KEY_T,
        "u" => Key::KEY_U,
        "v" => Key::KEY_V,
        "w" => Key::KEY_W,
        "x" => Key::KEY_X,
        "y" => Key::KEY_Y,
        "z" => Key::KEY_Z,
        "f1" => Key::KEY_F1,
        "f2" => Key::KEY_F2,
        "f3" => Key::KEY_F3,
        "f4" => Key::KEY_F4,
        "f5" => Key::KEY_F5,
        "f6" => Key::KEY_F6,
        "f7" => Key::KEY_F7,
        "f8" => Key::KEY_F8,
        "f9" => Key::KEY_F9,
        "f10" => Key::KEY_F10,
        "f11" => Key::KEY_F11,
        "f12" => Key::KEY_F12,
        _ => Key::KEY_SPACE,
    }
}

pub fn is_modifier_match(key: Key, modifier: Key) -> bool {
    match modifier {
        Key::KEY_LEFTALT => key == Key::KEY_LEFTALT || key == Key::KEY_RIGHTALT,
        Key::KEY_LEFTCTRL => key == Key::KEY_LEFTCTRL || key == Key::KEY_RIGHTCTRL,
        Key::KEY_LEFTSHIFT => key == Key::KEY_LEFTSHIFT || key == Key::KEY_RIGHTSHIFT,
        Key::KEY_LEFTMETA => key == Key::KEY_LEFTMETA || key == Key::KEY_RIGHTMETA,
        _ => key == modifier,
    }
}

/// Find all keyboard devices in /dev/input/
fn find_keyboard_devices() -> Vec<Device> {
    let mut keyboards = Vec::new();
    for (_path, device) in evdev::enumerate() {
        if let Some(keys) = device.supported_keys() {
            if keys.contains(Key::KEY_A) && keys.contains(Key::KEY_SPACE) {
                keyboards.push(device);
            }
        }
    }
    keyboards
}

/// Monitor keyboard events via evdev for global hotkey detection.
/// Requires user to be in the `input` group or run as root.
pub async fn monitor(tx: mpsc::Sender<AppEvent>, config: &HotkeyConfig) -> Result<()> {
    let devices = find_keyboard_devices();
    if devices.is_empty() {
        anyhow::bail!(
            "No keyboard device found. Ensure user is in 'input' group: sudo usermod -aG input $USER"
        );
    }

    let key_str = match config.activation_method {
        ActivationMethod::HoldToTalk => config.hold_key.as_deref().unwrap_or(&config.toggle_key),
        ActivationMethod::Toggle => &config.toggle_key,
    };

    let (modifiers, trigger) = parse_hotkey(key_str);
    let activation = config.activation_method.clone();

    tracing::info!(
        "Monitoring {} keyboard device(s), hotkey: {}, mode: {:?}",
        devices.len(),
        key_str,
        activation
    );

    let (merged_tx, mut merged_rx) = mpsc::channel(256);

    for device in devices {
        let name = device.name().unwrap_or("unknown").to_string();
        tracing::info!("Monitoring keyboard: {}", name);
        let merged_tx = merged_tx.clone();
        tokio::spawn(async move {
            let mut stream = match device.into_event_stream() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("Failed to open event stream for {}: {}", name, e);
                    return;
                }
            };
            loop {
                match stream.next_event().await {
                    Ok(ev) => {
                        if merged_tx.send(ev).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Event stream error for {}: {}", name, e);
                        break;
                    }
                }
            }
        });
    }
    drop(merged_tx);

    let mut modifier_state: Vec<bool> = vec![false; modifiers.len()];
    let mut hold_press_time: Option<tokio::time::Instant> = None;
    let mut debounce_handle: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        let event = merged_rx
            .recv()
            .await
            .context("All keyboard streams closed")?;

        if let InputEventKind::Key(key) = event.kind() {
            // Update modifier state
            for (i, m) in modifiers.iter().enumerate() {
                if is_modifier_match(key, *m) {
                    modifier_state[i] = event.value() != 0; // 1=press, 2=repeat, 0=release
                }
            }

            // Check trigger key
            if key == trigger {
                let all_mods_held = modifier_state.iter().all(|&held| held);
                if !all_mods_held {
                    continue;
                }

                match activation {
                    ActivationMethod::Toggle => {
                        if event.value() == 1 {
                            // key press
                            tracing::debug!("Toggle hotkey triggered");
                            let _ = tx.send(AppEvent::ToggleRecording).await;
                        }
                    }
                    ActivationMethod::HoldToTalk => {
                        if event.value() == 1 {
                            // key press — start 200ms debounce timer
                            tracing::debug!("Hold-to-talk: key down, starting debounce");
                            hold_press_time = Some(tokio::time::Instant::now());
                            // Schedule delayed start
                            let tx2 = tx.clone();
                            debounce_handle = Some(tokio::spawn(async move {
                                tokio::time::sleep(HOLD_DEBOUNCE).await;
                                tracing::debug!("Hold-to-talk: debounce passed, starting");
                                let _ = tx2.send(AppEvent::StartRecording).await;
                            }));
                        } else if event.value() == 0 {
                            // key release
                            if let Some(press_time) = hold_press_time.take() {
                                let held = press_time.elapsed();
                                if held < HOLD_DEBOUNCE {
                                    // Too short — cancel the debounce, don't record
                                    if let Some(h) = debounce_handle.take() { h.abort(); }
                                    tracing::debug!("Hold-to-talk: released before debounce ({:?}), ignoring", held);
                                } else {
                                    // Valid hold — stop recording
                                    tracing::debug!("Hold-to-talk: stop");
                                    let _ = tx.send(AppEvent::StopRecording).await;
                                }
                            }
                            debounce_handle = None;
                        }
                    }
                }
            }
        }
    }
}
