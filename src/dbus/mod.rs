use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::mpsc;
use zbus::{interface, Connection, ConnectionBuilder, SignalContext};

use crate::AppEvent;

/// Allowed caller binaries for sensitive D-Bus methods
const ALLOWED_CALLERS: &[&str] = &["gnome-shell", "frogscribe", "gdbus", "dbus-send"];

pub struct FrogScribeService {
    tx: mpsc::Sender<AppEvent>,
    status: String,
    pub auto_transcription_paused: Arc<AtomicBool>,
}

/// Check if a D-Bus caller (by unique bus name) is authorized
async fn is_authorized(conn: &Connection, sender: &str) -> bool {
    let proxy = match zbus::fdo::DBusProxy::new(conn).await {
        Ok(p) => p,
        Err(_) => return false,
    };
    let pid = match proxy.get_connection_unix_process_id(
        zbus::names::UniqueName::try_from(sender).unwrap().into()
    ).await {
        Ok(p) => p,
        Err(_) => return false,
    };
    if pid == 0 { return false; }
    let comm_path = format!("/proc/{}/comm", pid);
    match std::fs::read_to_string(&comm_path) {
        Ok(comm) => {
            let name = comm.trim();
            ALLOWED_CALLERS.iter().any(|allowed| name == *allowed)
        }
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
