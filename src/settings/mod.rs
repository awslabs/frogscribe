// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub hotkey: HotkeyConfig,
    pub audio: AudioConfig,
    pub transcription: TranscriptionConfig,
    pub refinement: RefinementConfig,
    pub appearance: AppearanceConfig,
    pub general: GeneralConfig,
    #[serde(default)]
    pub auto_transcription: AutoTranscriptionConfig,
    #[serde(default)]
    pub summarization: SummarizationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub toggle_key: String,
    pub hold_key: Option<String>,
    pub activation_method: ActivationMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActivationMethod {
    Toggle,
    HoldToTalk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub device: Option<String>,
    pub office_mode: bool,
    pub ear_protection: EarProtection,
    pub sample_rate: u32,
    #[serde(default)]
    pub capture_desktop_audio: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EarProtection {
    Off,
    On,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    pub model: String,
    pub language: String,
    pub translate_to_english: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementConfig {
    pub enabled: bool,
    pub mode: RefinementMode,
    pub remove_fillers: bool,
    pub fix_capitalization: bool,
    pub custom_vocabulary: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefinementMode {
    Local,
    Smart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_true")]
    pub pill_enabled: bool,
    #[serde(default)]
    pub topbar_enabled: bool,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    // Legacy field for migration
    #[serde(default)]
    pub indicator_style: Option<IndicatorStyle>,
}

fn default_true() -> bool { true }
fn default_accent() -> String { "teal".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IndicatorStyle {
    Pill,
    TopBar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub auto_submit: bool,
    pub history_enabled: bool,
    #[serde(default)]
    pub auto_save_transcriptions: bool,
    #[serde(default = "default_true")]
    pub context_header: bool,
    #[serde(default)]
    pub rainbow_unlocked: bool,
    #[serde(default = "default_true")]
    pub auto_paste: bool,
    #[serde(default = "default_true")]
    pub use_window_picker: bool,
    #[serde(default)]
    pub insertion_method: InsertionMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum InsertionMethod {
    Off,
    TypeEveryCharacter,
    PasteFullTranscript,
}

impl Default for InsertionMethod {
    fn default() -> Self { Self::TypeEveryCharacter }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTranscriptionConfig {
    /// Master switch for automatic transcription
    #[serde(default)]
    pub enabled: bool,
    /// Use energy-based VAD to auto-stop on silence
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
    /// Seconds of silence before auto-stopping
    #[serde(default = "default_silence_seconds")]
    pub silence_seconds: u32,
}

fn default_silence_seconds() -> u32 { 30 }

impl Default for AutoTranscriptionConfig {
    fn default() -> Self {
        Self { enabled: false, vad_enabled: true, silence_seconds: 30 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummarizationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_summarization_model")]
    pub model: String,
}

fn default_summarization_model() -> String { "phi-3-mini".into() }

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self { enabled: false, model: "phi-3-mini".into() }
    }
}

/// Available summarization models
pub fn available_summarization_models() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("phi-3-mini", "~2.3GB", "Detailed meeting notes, structured output (MIT)"),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: HotkeyConfig {
                toggle_key: "Ctrl+Shift+Space".into(),
                hold_key: None,
                activation_method: ActivationMethod::Toggle,
            },
            audio: AudioConfig {
                device: None,
                office_mode: false,
                ear_protection: EarProtection::Off,
                sample_rate: 16000,
                capture_desktop_audio: false,
            },
            transcription: TranscriptionConfig {
                model: "base".into(),
                language: "en".into(),
                translate_to_english: false,
                streaming: false,
            },
            refinement: RefinementConfig {
                enabled: true,
                mode: RefinementMode::Local,
                remove_fillers: true,
                fix_capitalization: true,
                custom_vocabulary: Vec::new(),
            },
            appearance: AppearanceConfig {
                pill_enabled: true,
                topbar_enabled: false,
                accent_color: "teal".into(),
                indicator_style: None,
            },
            general: GeneralConfig {
                auto_submit: false,
                history_enabled: false,
                auto_save_transcriptions: false,
                context_header: true,
                rainbow_unlocked: false,
                auto_paste: true,
                use_window_picker: true,
                insertion_method: InsertionMethod::TypeEveryCharacter,
            },
            auto_transcription: AutoTranscriptionConfig::default(),
            summarization: SummarizationConfig::default(),
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .expect("cannot determine config directory: set $HOME or $XDG_CONFIG_HOME")
            .join("frogscribe")
            .join("settings.toml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            Ok(toml::from_str(&content)?)
        } else {
            let settings = Self::default();
            settings.save()?;
            Ok(settings)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .expect("cannot determine data directory: set $HOME or $XDG_DATA_HOME")
            .join("frogscribe")
    }

    pub fn models_dir() -> PathBuf {
        Self::data_dir().join("models")
    }

    /// Ensure a directory exists and is private to the current user (0700).
    pub fn ensure_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
    }

    /// Write data to a file readable/writable only by the owner (0600),
    /// creating it if needed and normalizing permissions on pre-existing files.
    /// Used for transcription-derived data (history, transcripts, summaries)
    /// so it is never left at the umask default (typically world-readable).
    pub fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(data)?;
        // Normalize perms in case the file already existed with looser bits.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
}
