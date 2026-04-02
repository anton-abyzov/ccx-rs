pub mod oauth;

use std::io::Write;
use std::path::PathBuf;

/// Errors during API key resolution.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("no API key found: set ANTHROPIC_API_KEY or add it to ~/.claude/config.json")]
    NoKeyFound,

    #[error("no credentials found: set ANTHROPIC_API_KEY or log in with Claude Code")]
    NoCredentials,

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

/// Authentication method resolved from available credentials.
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Traditional API key authentication.
    ApiKey(ResolvedKey),
    /// OAuth token from Claude Code subscription (Max, Pro, Team).
    OAuthToken {
        access_token: String,
        api_key: Option<String>,
        subscription_type: String,
    },
    /// Not authenticated yet — user needs to /login or set an API key.
    None,
}

impl AuthMethod {
    /// Human-readable label for the auth source (for welcome panel display).
    pub fn display_label(&self) -> &str {
        match self {
            AuthMethod::ApiKey(_) => "API Key",
            AuthMethod::OAuthToken {
                subscription_type, ..
            } => match subscription_type.as_str() {
                "max" => "Claude Max",
                "pro" => "Claude Pro",
                "team" => "Claude Team",
                _ => "Claude Subscription",
            },
            AuthMethod::None => "Not authenticated",
        }
    }

    /// Returns the OAuth access token if this is an OAuth auth method.
    pub fn oauth_token(&self) -> Option<&str> {
        match self {
            AuthMethod::OAuthToken { access_token, .. } => Some(access_token),
            _ => None,
        }
    }

    /// Returns true if no credentials are available.
    pub fn is_none(&self) -> bool {
        matches!(self, AuthMethod::None)
    }
}

/// Fetch the account email from the Anthropic OAuth profile endpoint.
pub async fn fetch_oauth_email(access_token: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/api/oauth/profile")
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    body.get("email")
        .and_then(|e| e.as_str())
        .map(|s| s.to_string())
}

/// Resolve authentication from all available sources, in priority order:
/// 1. Explicit API key (if provided)
/// 2. ANTHROPIC_API_KEY environment variable
/// 3. Claude Code OAuth token from macOS Keychain
/// 4. Claude Code OAuth token from ~/.claude/.credentials.json
/// 5. API key from ~/.claude/config.json
pub fn resolve_auth(explicit: Option<&str>) -> Result<AuthMethod, AuthError> {
    // 1. Explicit key takes priority.
    if let Some(key) = explicit {
        return Ok(AuthMethod::ApiKey(ResolvedKey {
            key: key.to_string(),
            source: KeySource::Explicit,
        }));
    }

    // 2. Environment variable.
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY")
        && !key.is_empty()
    {
        return Ok(AuthMethod::ApiKey(ResolvedKey {
            key,
            source: KeySource::EnvVar,
        }));
    }

    // 3. macOS Keychain OAuth token.
    if cfg!(target_os = "macos")
        && let Some(oauth) = read_keychain_token()
    {
        return Ok(oauth);
    }

    // 4. Credentials file OAuth token.
    if let Some(oauth) = read_credentials_file() {
        return Ok(oauth);
    }

    // 5. Config file API key (~/.claude/config.json).
    if let Some(key) = read_key_from_config()? {
        let config_path = config_file_path().unwrap();
        return Ok(AuthMethod::ApiKey(ResolvedKey {
            key,
            source: KeySource::ConfigFile(config_path),
        }));
    }

    // 6. CCX saved credentials (~/.ccx/config.json).
    if let Some((provider, key)) = read_ccx_config_credential() {
        match provider.as_str() {
            "anthropic" => {
                return Ok(AuthMethod::ApiKey(ResolvedKey {
                    key,
                    source: KeySource::ConfigFile(ccx_config_path().unwrap()),
                }));
            }
            // For non-Anthropic providers, set the env var so downstream
            // provider detection picks it up.
            "openrouter" => {
                // SAFETY: single-threaded startup path.
                unsafe { std::env::set_var("OPENROUTER_API_KEY", &key) };
            }
            "openai" => {
                unsafe { std::env::set_var("OPENAI_API_KEY", &key) };
            }
            _ => {}
        }
        // Fall through so run_chat() auto-detects the env var provider.
    }

    Err(AuthError::NoCredentials)
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
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY")
        && !key.is_empty()
    {
        return Ok(ResolvedKey {
            key,
            source: KeySource::EnvVar,
        });
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

/// Read OAuth token from macOS Keychain (Claude Code stores it there).
fn read_keychain_token() -> Option<AuthMethod> {
    let user = std::env::var("USER").ok()?;
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-a",
            &user,
            "-w",
            "-s",
            "Claude Code-credentials",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json_str = String::from_utf8(output.stdout).ok()?.trim().to_string();
    parse_oauth_json(&json_str)
}

/// Read OAuth token from ~/.claude/.credentials.json fallback file.
fn read_credentials_file() -> Option<AuthMethod> {
    let path = home_dir()?.join(".claude/.credentials.json");
    let content = std::fs::read_to_string(path).ok()?;
    parse_oauth_json(&content)
}

/// Parse Claude Code OAuth JSON and return an AuthMethod if valid and not expired.
fn parse_oauth_json(json: &str) -> Option<AuthMethod> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let oauth = v.get("claudeAiOauth").or_else(|| v.get("claudeAiOAuth"));
    let access_token = oauth
        .and_then(|value| value.get("accessToken"))
        .or_else(|| v.get("accessToken"))?
        .as_str()?
        .to_string();
    let api_key = oauth
        .and_then(|value| value.get("apiKey"))
        .or_else(|| v.get("apiKey"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let subscription_type = oauth
        .and_then(|value| value.get("subscriptionType"))
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Check if token is expired.
    if let Some(expires) = oauth
        .and_then(|value| value.get("expiresAt"))
        .or_else(|| v.get("expiresAt"))
        .and_then(|value| value.as_i64())
    {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if now_ms > expires {
            return None;
        }
    }

    if access_token.is_empty() {
        return None;
    }
    Some(AuthMethod::OAuthToken {
        access_token,
        api_key,
        subscription_type,
    })
}

/// Path to ~/.claude/config.json
fn config_file_path() -> Option<PathBuf> {
    dirs_path().map(|p| p.join("config.json"))
}

fn dirs_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".claude"))
}

fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
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

/// Path to ~/.ccx/config.json (CCX-specific config).
pub fn ccx_config_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".ccx/config.json"))
}

/// Read the default provider and its key from ~/.ccx/config.json.
fn read_ccx_config_credential() -> Option<(String, String)> {
    let path = ccx_config_path()?;
    let content = std::fs::read_to_string(path).ok()?;
    let config: serde_json::Value = serde_json::from_str(&content).ok()?;

    let provider = config.get("default_provider")?.as_str()?.to_string();
    let creds = config.get("credentials")?;

    let key_field = match provider.as_str() {
        "anthropic" => "anthropic_api_key",
        "openrouter" => "openrouter_api_key",
        "openai" => "openai_api_key",
        _ => return None,
    };

    let key = creds.get(key_field)?.as_str()?.to_string();
    if key.is_empty() {
        return None;
    }
    Some((provider, key))
}

/// Save a credential to ~/.ccx/config.json, merging with existing data.
pub fn save_ccx_credential(provider: &str, key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = ccx_config_path().ok_or("Cannot determine home directory")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Load existing config or start fresh.
    let mut config: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    config["default_provider"] = serde_json::json!(provider);

    let key_field = match provider {
        "anthropic" => "anthropic_api_key",
        "openrouter" => "openrouter_api_key",
        "openai" => "openai_api_key",
        _ => return Err(format!("Unknown provider: {provider}").into()),
    };

    if config.get("credentials").is_none() {
        config["credentials"] = serde_json::json!({});
    }
    config["credentials"][key_field] = serde_json::json!(key);

    // Write with restricted permissions on Unix.
    let json_str = serde_json::to_string_pretty(&config)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?
            .write_all(json_str.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, &json_str)?;
    }

    Ok(())
}

/// Validate an API key by making a lightweight test call. Returns Ok(()) if valid.
pub async fn validate_api_key(provider: &str, key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    match provider {
        "anthropic" => {
            let resp = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .body(r#"{"model":"claude-haiku-4-5-20241022","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#)
                .send()
                .await?;
            if resp.status() == 401 {
                return Err("Invalid API key".into());
            }
            Ok(())
        }
        "openrouter" => {
            let resp = client
                .get("https://openrouter.ai/api/v1/models")
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .await?;
            if resp.status() == 401 {
                return Err("Invalid API key".into());
            }
            Ok(())
        }
        "openai" => {
            let resp = client
                .get("https://api.openai.com/v1/models")
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .await?;
            if resp.status() == 401 {
                return Err("Invalid API key".into());
            }
            Ok(())
        }
        _ => Err(format!("Unknown provider: {provider}").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_explicit_key_takes_priority() {
        let result = resolve_api_key(Some("sk-explicit-key")).unwrap();
        assert_eq!(result.key, "sk-explicit-key");
        assert_eq!(result.source, KeySource::Explicit);
    }

    #[test]
    fn test_env_var_resolution() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-env-key") };
        let result = resolve_api_key(None).unwrap();
        assert_eq!(result.key, "sk-env-key");
        assert_eq!(result.source, KeySource::EnvVar);
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn test_no_key_found() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        let result = resolve_api_key(None);
        assert!(result.is_ok() || matches!(result, Err(AuthError::NoKeyFound)));
    }

    #[test]
    fn test_explicit_overrides_env() {
        let _guard = env_lock().lock().unwrap();
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-env-key") };
        let result = resolve_api_key(Some("sk-explicit")).unwrap();
        assert_eq!(result.key, "sk-explicit");
        assert_eq!(result.source, KeySource::Explicit);
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn test_parse_oauth_json_accepts_alt_casing_and_api_key() {
        let json = r#"{
            "claudeAiOAuth": {
                "accessToken": "oauth-token",
                "apiKey": "sk-ant-oauth-derived",
                "subscriptionType": "max",
                "expiresAt": 4102444800000
            }
        }"#;

        let parsed = parse_oauth_json(json);
        match parsed {
            Some(AuthMethod::OAuthToken {
                access_token,
                api_key,
                subscription_type,
            }) => {
                assert_eq!(access_token, "oauth-token");
                assert_eq!(api_key.as_deref(), Some("sk-ant-oauth-derived"));
                assert_eq!(subscription_type, "max");
            }
            _ => panic!("expected OAuthToken"),
        }
    }
}
