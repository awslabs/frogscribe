// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
//! Model health check: detects corrupted, truncated, or incomplete model downloads.
//! Repairs by deleting bad files so next load triggers a clean re-download.

use anyhow::Result;
use std::path::PathBuf;

use crate::settings::Settings;

#[derive(Debug, Clone)]
pub struct ModelDiagnosis {
    pub model_name: String,
    pub issue: Option<ModelIssue>,
}

impl ModelDiagnosis {
    pub fn is_healthy(&self) -> bool { self.issue.is_none() }
}

#[derive(Debug, Clone)]
pub enum ModelIssue {
    Missing,
    TooSmall { actual_bytes: u64, minimum_bytes: u64 },
    GitLfsPointer,
    Unreadable(String),
}

impl std::fmt::Display for ModelIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "Model file not found"),
            Self::TooSmall { actual_bytes, minimum_bytes } =>
                write!(f, "File truncated ({} bytes, expected ≥{})", actual_bytes, minimum_bytes),
            Self::GitLfsPointer => write!(f, "File is a Git LFS pointer (not actual model data)"),
            Self::Unreadable(e) => write!(f, "Cannot read file: {}", e),
        }
    }
}

/// Minimum expected sizes for each model variant (conservative — catches obvious truncation)
fn minimum_size(model_id: &str) -> u64 {
    match model_id {
        "tiny" => 30_000_000,
        "base" => 70_000_000,
        "small" => 200_000_000,
        "medium" => 700_000_000,
        "large-v3" => 1_500_000_000,
        _ => 10_000_000,
    }
}

/// Check health of a single model
pub fn check_model(model_id: &str) -> ModelDiagnosis {
    let path = Settings::models_dir().join(format!("ggml-{}.bin", model_id));

    let issue = if !path.exists() {
        Some(ModelIssue::Missing)
    } else {
        match std::fs::metadata(&path) {
            Ok(meta) => {
                let min = minimum_size(model_id);
                if meta.len() < min {
                    // Check if it's a Git LFS pointer (starts with "version https://git-lfs")
                    if meta.len() < 200 {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if content.starts_with("version https://git-lfs") {
                                return ModelDiagnosis {
                                    model_name: model_id.to_string(),
                                    issue: Some(ModelIssue::GitLfsPointer),
                                };
                            }
                        }
                    }
                    Some(ModelIssue::TooSmall { actual_bytes: meta.len(), minimum_bytes: min })
                } else {
                    // Verify file is readable (first 4 bytes should be GGML magic)
                    match std::fs::File::open(&path) {
                        Ok(mut f) => {
                            use std::io::Read;
                            let mut header = [0u8; 4];
                            match f.read_exact(&mut header) {
                                Ok(_) => None, // readable
                                Err(e) => Some(ModelIssue::Unreadable(e.to_string())),
                            }
                        }
                        Err(e) => Some(ModelIssue::Unreadable(e.to_string())),
                    }
                }
            }
            Err(e) => Some(ModelIssue::Unreadable(e.to_string())),
        }
    };

    ModelDiagnosis { model_name: model_id.to_string(), issue }
}

/// Check all downloaded models
pub fn check_all_models() -> Vec<ModelDiagnosis> {
    let models_dir = Settings::models_dir();
    if !models_dir.exists() {
        return Vec::new();
    }

    std::fs::read_dir(models_dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("ggml-") && name.ends_with(".bin") {
                let id = name.trim_start_matches("ggml-").trim_end_matches(".bin").to_string();
                Some(check_model(&id))
            } else {
                None
            }
        })
        .collect()
}

/// Repair a corrupted model by deleting it (next load will re-download)
pub fn repair_model(model_id: &str) -> Result<()> {
    let path = Settings::models_dir().join(format!("ggml-{}.bin", model_id));
    if path.exists() {
        std::fs::remove_file(&path)?;
        tracing::info!("Removed corrupted model '{}' for re-download", model_id);
    }
    Ok(())
}

/// Check and auto-repair the currently configured model. Returns true if repair was needed.
pub fn check_and_repair(model_id: &str) -> bool {
    let diagnosis = check_model(model_id);
    if let Some(issue) = &diagnosis.issue {
        match issue {
            ModelIssue::Missing => false, // not downloaded yet, not an error
            _ => {
                tracing::warn!("Model '{}' is unhealthy: {}. Removing for re-download.", model_id, issue);
                let _ = repair_model(model_id);
                true
            }
        }
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimum_sizes() {
        assert!(minimum_size("tiny") > 0);
        assert!(minimum_size("large-v3") > minimum_size("tiny"));
    }

    #[test]
    fn test_check_nonexistent_model() {
        let d = check_model("nonexistent_test_model_xyz");
        assert!(matches!(d.issue, Some(ModelIssue::Missing)));
        assert!(!d.is_healthy());
    }

    #[test]
    fn test_diagnosis_display() {
        let issue = ModelIssue::TooSmall { actual_bytes: 100, minimum_bytes: 30_000_000 };
        let s = format!("{}", issue);
        assert!(s.contains("truncated"));
        assert!(s.contains("100"));
    }
}
