// SPDX-License-Identifier: Apache-2.0
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use zbus::{interface, Connection, ConnectionBuilder, SignalContext};

use crate::AppEvent;

pub struct FrogScribeService {
    tx: mpsc::Sender<AppEvent>,
    status: String,
    pub auto_transcription_paused: Arc<AtomicBool>,
}

/// The UID this daemon runs as, taken from the owner of `/proc/self`.
fn own_uid() -> Option<u32> {
    std::fs::metadata("/proc/self").ok().map(|m| m.uid())
}

/// Authorize a sensitive D-Bus call.
///
/// The daemon lives on the *session* bus, which is shared by every process of
/// the current user. The only non-spoofable identity D-Bus can give us about a
/// peer is its UID, provided by the kernel via `GetConnectionUnixUser`, so we
/// authorize a caller iff it runs as the same user as the daemon.
///
/// We deliberately do NOT match on process name (`/proc/PID/comm`): a name is
/// not an identity — any process can be named `gnome-shell`, and legitimate
/// callers use the generic `gdbus`/`dbus-send` tools — so a name allowlist
/// admits (or is spoofed by) essentially any caller. A same-UID process is
/// inherently inside this trust boundary; see T2 in docs/THREAT_MODEL.md.
async fn is_authorized(conn: &Connection, sender: &str) -> bool {
    let our_uid = match own_uid() {
        Some(uid) => uid,
        None => return false,
    };
    let proxy = match zbus::fdo::DBusProxy::new(conn).await {
        Ok(p) => p,
        Err(_) => return false,
    };
    let name = match zbus::names::BusName::try_from(sender) {
        Ok(n) => n,
        Err(_) => return false,
    };
    match proxy.get_connection_unix_user(name).await {
        Ok(uid) => uid == our_uid,
        Err(_) => false,
    }
}

#[interface(name = "com.frogscribe.Daemon")]
impl FrogScribeService {
    async fn start_recording(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::StartRecording).await;
        "ok".to_string()
    }

    async fn stop_recording(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::StopRecording).await;
        "ok".to_string()
    }

    async fn toggle_recording(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::ToggleRecording).await;
        "ok".to_string()
    }

    async fn get_status(&self) -> String {
        self.status.clone()
    }

    async fn quit(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::Quit).await;
        "ok".to_string()
    }

    async fn start_long_form(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::StartLongForm).await;
        "ok".to_string()
    }

    async fn stop_long_form(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::StopLongForm).await;
        "ok".to_string()
    }

    async fn reload_hotkey(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        let _ = self.tx.send(AppEvent::ReloadHotkey).await;
        "ok".to_string()
    }

    async fn set_auto_transcription_paused(&self, #[zbus(connection)] conn: &Connection, #[zbus(header)] hdr: zbus::message::Header<'_>, paused: bool) -> String {
        if let Some(sender) = hdr.sender() {
            if !is_authorized(conn, sender.as_str()).await { return "denied".to_string(); }
        }
        self.auto_transcription_paused.store(paused, Ordering::Relaxed);
        tracing::info!("Auto-transcription paused: {}", paused);
        "ok".to_string()
    }

    async fn get_auto_transcription_enabled(&self) -> String {
        match crate::settings::Settings::load() {
            Ok(s) => if s.auto_transcription.enabled { "true" } else { "false" }.to_string(),
            Err(_) => "false".to_string(),
        }
    }

    #[zbus(signal)]
    async fn status_changed(ctx: &SignalContext<'_>, status: &str) -> zbus::Result<()>;
}

pub async fn start_service(tx: mpsc::Sender<AppEvent>, auto_paused: Arc<AtomicBool>) -> anyhow::Result<Connection> {
    let service = FrogScribeService {
        tx,
        status: "idle".to_string(),
        auto_transcription_paused: auto_paused,
    };

    let conn = ConnectionBuilder::session()?
        .name("com.frogscribe.Daemon")?
        .serve_at("/com/frogscribe/Daemon", service)?
        .build()
        .await?;

    tracing::info!("D-Bus service started at com.frogscribe.Daemon");
    Ok(conn)
}

pub async fn emit_status(conn: &Connection, status: &str) {
    let object_server = conn.object_server();
    if let Ok(iface_ref) = object_server.interface::<_, FrogScribeService>("/com/frogscribe/Daemon").await {
        let ctx = iface_ref.signal_context();
        let _ = FrogScribeService::status_changed(ctx, status).await;
    }
}
