use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ccx_permission::PermissionSettings;

/// User settings loaded from ~/.claude/settings.json.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub permissions: PermissionSettings,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("failed to read settings: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid settings JSON: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Load settings from a specific file path.
pub fn load_settings(path: &Path) -> Result<Settings, SettingsError> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let content = fs::read_to_string(path)?;
    let settings: Settings = serde_json::from_str(&content)?;
    Ok(settings)
}

/// Load settings from the default location (~/.claude/settings.json).
pub fn load_default_settings() -> Result<Settings, SettingsError> {
    let path = default_settings_path();
    match path {
        Some(p) => load_settings(&p),
        None => Ok(Settings::default()),
    }
}

/// Path to ~/.claude/settings.json.
pub fn default_settings_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| h.join(".claude").join("settings.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent() {
        let settings = load_settings(Path::new("/nonexistent/settings.json")).unwrap();
        assert!(settings.model.is_none());
    }

    #[test]
    fn test_load_from_string() {
        let dir = std::env::temp_dir().join("ccx_test_settings");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("settings.json");
        fs::write(
            &path,
            r#"{"model": "claude-sonnet-4-6", "permissions": {"mode": "plan"}}"#,
        )
        .unwrap();

        let settings = load_settings(&path).unwrap();
        assert_eq!(settings.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(
            settings.permissions.mode,
            Some(ccx_permission::PermissionMode::Plan)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_serde_roundtrip() {
        let settings = Settings {
            permissions: PermissionSettings {
                mode: Some(ccx_permission::PermissionMode::Auto),
                allow: vec!["Bash(git *)".into()],
                deny: vec!["Bash(rm *)".into()],
            },
            model: Some("test".into()),
            max_tokens: Some(4096),
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, settings.model);
        assert_eq!(parsed.max_tokens, settings.max_tokens);
    }
}
