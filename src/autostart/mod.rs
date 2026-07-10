#![allow(dead_code)]
use anyhow::Result;
use std::path::PathBuf;

const DESKTOP_ENTRY: &str = "[Desktop Entry]
Type=Application
Name=FrogScribe
Comment=Voice dictation for Linux
Exec=frogscribe
Icon=audio-input-microphone
Terminal=false
Categories=Utility;Accessibility;
StartupNotify=false
X-GNOME-Autostart-enabled=true
";

fn autostart_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("autostart")
        .join("frogscribe.desktop")
}

pub fn is_enabled() -> bool {
    autostart_path().exists()
}

pub fn enable() -> Result<()> {
    let path = autostart_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, DESKTOP_ENTRY)?;
    Ok(())
}

pub fn disable() -> Result<()> {
    let path = autostart_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled { enable() } else { disable() }
}
