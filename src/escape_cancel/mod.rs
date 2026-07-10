//! Escape-to-cancel: monitors Escape key during recording to abort without transcribing.

use evdev::{InputEventKind, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::AppEvent;

/// Spawn an Escape key monitor. Sends CancelRecording when Escape is pressed.
/// Returns a stop handle to disable monitoring.
pub fn start_monitoring(tx: mpsc::Sender<AppEvent>) -> Arc<AtomicBool> {
    let active = Arc::new(AtomicBool::new(true));
    let active_clone = active.clone();

    tokio::spawn(async move {
        let devices: Vec<_> = evdev::enumerate()
            .filter(|(_, d)| {
                d.supported_keys()
                    .map(|k| k.contains(Key::KEY_ESC))
                    .unwrap_or(false)
            })
            .collect();

        if devices.is_empty() {
            return;
        }

        let (merged_tx, mut merged_rx) = mpsc::channel(64);

        for (_path, device) in devices {
            let merged_tx = merged_tx.clone();
            tokio::spawn(async move {
                let mut stream = match device.into_event_stream() {
                    Ok(s) => s,
                    Err(_) => return,
                };
                loop {
                    match stream.next_event().await {
                        Ok(ev) => { let _ = merged_tx.send(ev).await; }
                        Err(_) => break,
                    }
                }
            });
        }
        drop(merged_tx);

        while let Some(event) = merged_rx.recv().await {
            if !active_clone.load(Ordering::Relaxed) {
                break;
            }
            if let InputEventKind::Key(Key::KEY_ESC) = event.kind() {
                if event.value() == 1 { // key press
                    tracing::debug!("Escape pressed — cancelling recording");
                    let _ = tx.send(AppEvent::CancelRecording).await;
                }
            }
        }
    });

    active
}

/// Stop monitoring Escape key
pub fn stop_monitoring(handle: &Arc<AtomicBool>) {
    handle.store(false, Ordering::Relaxed);
}
