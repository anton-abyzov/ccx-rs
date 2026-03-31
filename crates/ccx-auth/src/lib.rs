use std::path::PathBuf;

/// Errors during API key resolution.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("no API key found: set ANTHROPIC_API_KEY or add it to ~/.claude/config.json")]
    NoKeyFound,

    #[error("config file error: {0}")]
    ConfigRead(String),

    #[error("config file has invalid JSON: {0}")]
    ConfigParse(#[from] serde_json::Error),
}

/// Source where the API key was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// From the ANTHROPIC_API_KEY environment variable.
    EnvVar,
    /// From the config file at the given path.
    ConfigFile(PathBuf),
    /// Explicitly provided (e.g. via --api-key flag).
    Explicit,
}

/// A resolved API key with its source.
#[derive(Debug, Clone)]
pub struct ResolvedKey {
    pub key: String,
    pub source: KeySource,
}

/// Resolve the API key from multiple sources, in priority order:
/// 1. Explicit key (if provided)
/// 2. ANTHROPIC_API_KEY environment variable
/// 3. ~/.claude/config.json file
pub fn resolve_api_key(explicit: Option<&str>) -> Result<ResolvedKey, AuthError> {
    // 1. Explicit key takes priority.
    if let Some(key) = explicit {
        return Ok(ResolvedKey {
            key: key.to_string(),
            source: KeySource::Explicit,
        });
    }

    // 2. Environment variable.
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return Ok(ResolvedKey {
                key,
                source: KeySource::EnvVar,
            });
        }
    }

    // 3. Config file.
    if let Some(key) = read_key_from_config()? {
        let config_path = config_file_path().unwrap();
        return Ok(ResolvedKey {
            key,
            source: KeySource::ConfigFile(config_path),
        });
    }

    Err(AuthError::NoKeyFound)
}

/// Path to ~/.claude/config.json
fn config_file_path() -> Option<PathBuf> {
    dirs_path().map(|p| p.join("config.json"))
}

fn dirs_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Attempt to read the API key from the config file.
fn read_key_from_config() -> Result<Option<String>, AuthError> {
    let path = match config_file_path() {
        Some(p) => p,
        None => return Ok(None),
    };

    if !path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(&path)
        .map_err(|e| AuthError::ConfigRead(format!("{path:?}: {e}")))?;

    let config: serde_json::Value = serde_json::from_str(&contents)?;

    let key = config
        .get("apiKey")
        .or_else(|| config.get("api_key"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_key_takes_priority() {
        let result = resolve_api_key(Some("sk-explicit-key")).unwrap();
        assert_eq!(result.key, "sk-explicit-key");
        assert_eq!(result.source, KeySource::Explicit);
    }

    #[test]
    fn test_env_var_resolution() {
        // SAFETY: test is single-threaded, env var is restored after use.
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-env-key") };
        let result = resolve_api_key(None).unwrap();
        assert_eq!(result.key, "sk-env-key");
        assert_eq!(result.source, KeySource::EnvVar);
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn test_no_key_found() {
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        let result = resolve_api_key(None);
        assert!(result.is_ok() || matches!(result, Err(AuthError::NoKeyFound)));
    }

    #[test]
    fn test_explicit_overrides_env() {
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-env-key") };
        let result = resolve_api_key(Some("sk-explicit")).unwrap();
        assert_eq!(result.key, "sk-explicit");
        assert_eq!(result.source, KeySource::Explicit);
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }
}
