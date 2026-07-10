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
                history_enabled: true,
                auto_save_transcriptions: false,
                context_header: true,
                rainbow_unlocked: false,
                auto_paste: true,
                use_window_picker: true,
                insertion_method: InsertionMethod::TypeEveryCharacter,
            },
            auto_transcription: AutoTranscriptionConfig::default(),
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
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
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("frogscribe")
    }

    pub fn models_dir() -> PathBuf {
        Self::data_dir().join("models")
    }
}
