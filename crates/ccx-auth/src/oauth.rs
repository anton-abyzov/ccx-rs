use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTH_URL: &str = "https://claude.com/cai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const API_KEY_URL: &str = "https://api.anthropic.com/api/oauth/claude_cli/create_api_key";
const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
const MANUAL_REDIRECT_URL: &str = "https://platform.claude.com/oauth/code/callback";
const SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

/// OAuth tokens returned after a successful login.
#[derive(Debug)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub api_key: Option<String>,
    pub subscription_type: Option<String>,
}

/// Run the full OAuth Authorization Code + PKCE flow.
/// Opens a browser, waits for callback, exchanges code for tokens, saves credentials.
pub async fn login() -> Result<OAuthTokens, Box<dyn std::error::Error>> {
    if should_use_local_callback_oauth() {
        return login_with_local_callback().await;
    }
    login_with_manual_callback().await
}

async fn login_with_local_callback() -> Result<OAuthTokens, Box<dyn std::error::Error>> {
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();

    // Optional localhost callback flow for environments that support it.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{port}/callback");
    let auth_url = build_auth_url(&redirect_uri, &code_challenge, &state);

    println!("Opening browser for authentication...");
    println!("If the browser doesn't open, visit:\n  {auth_url}");
    if let Err(e) = open::that(&auth_url) {
        eprintln!("Failed to open browser: {e}");
    }

    let (mut stream, _) = listener.accept()?;
    let code = extract_code_from_request(&mut stream, &state)?;

    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body style=\"font-family:system-ui;text-align:center;padding:60px\">\
        <h1>Authentication successful!</h1>\
        <p>You can close this tab and return to the terminal.</p>\
        </body></html>";
    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    finish_login(&code, &state, &code_verifier, &redirect_uri).await
}

async fn login_with_manual_callback() -> Result<OAuthTokens, Box<dyn std::error::Error>> {
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();
    let auth_url = build_auth_url(MANUAL_REDIRECT_URL, &code_challenge, &state);

    println!("Opening browser for authentication...");
    println!("If the browser doesn't open, visit:\n  {auth_url}");
    if let Err(e) = open::that(&auth_url) {
        eprintln!("Failed to open browser: {e}");
    }

    println!();
    println!("Paste the callback URL or authorization code here:");
    print!("> ");
    std::io::stdout().flush()?;

    let mut input = String::new();
    BufReader::new(std::io::stdin()).read_line(&mut input)?;
    let code = extract_code_from_manual_input(input.trim(), &state)?;

    finish_login(&code, &state, &code_verifier, MANUAL_REDIRECT_URL).await
}

async fn finish_login(
    code: &str,
    state: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens, Box<dyn std::error::Error>> {
    let tokens = exchange_code_for_tokens(code, state, code_verifier, redirect_uri).await?;
    let subscription_type = fetch_subscription_type(&tokens.access_token).await;
    let api_key = match create_cli_api_key(&tokens.access_token).await {
        Ok(api_key) => Some(api_key),
        Err(err) => {
            eprintln!("Warning: failed to mint Claude CLI API key: {err}");
            None
        }
    };

    save_credentials(
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
        api_key.as_deref(),
        subscription_type.as_deref(),
        tokens.expires_in.unwrap_or(3600),
    )?;

    println!("\x1b[32m✓ Logged in successfully!\x1b[0m");

    Ok(OAuthTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        api_key,
        subscription_type,
    })
}

fn build_auth_url(redirect_uri: &str, code_challenge: &str, state: &str) -> String {
    format!(
        "{}?code=true&client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        AUTH_URL,
        CLIENT_ID,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(SCOPES),
        code_challenge,
        state,
    )
}

fn should_use_local_callback_oauth() -> bool {
    matches!(
        std::env::var("CCX_OAUTH_LOCALHOST").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
}

async fn exchange_code_for_tokens(
    code: &str,
    state: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<TokenExchangeResponse, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let token_response = client
        .post(TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "grant_type": "authorization_code",
            "client_id": CLIENT_ID,
            "code": code,
            "redirect_uri": redirect_uri,
            "code_verifier": code_verifier,
            "state": state,
        }))
        .send()
        .await?;

    if !token_response.status().is_success() {
        let status = token_response.status();
        let body = token_response.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed ({status}): {body}").into());
    }

    let tokens: TokenExchangeResponse = token_response.json().await?;
    if tokens.access_token.is_empty() {
        return Err("No access_token in response".into());
    }
    Ok(tokens)
}

/// Generate a random PKCE code verifier (43-128 chars, unreserved characters).
fn generate_code_verifier() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.r#gen()).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Compute the S256 code challenge from the verifier.
fn generate_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate a random state parameter.
fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.r#gen()).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

async fn create_cli_api_key(access_token: &str) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(API_KEY_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API key creation failed ({status}): {body}").into());
    }

    let body: serde_json::Value = response.json().await?;
    let api_key = body
        .get("raw_key")
        .and_then(|value| value.as_str())
        .ok_or("No raw_key in API key response")?;
    Ok(api_key.to_string())
}

async fn fetch_subscription_type(access_token: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let response = client
        .get(PROFILE_URL)
        .header("Authorization", format!("Bearer {access_token}"))
        .header("Content-Type", "application/json")
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let body: serde_json::Value = response.json().await.ok()?;
    match body
        .get("organization")
        .and_then(|org| org.get("organization_type"))
        .and_then(|kind| kind.as_str())
    {
        Some("claude_max") => Some("max".to_string()),
        Some("claude_pro") => Some("pro".to_string()),
        Some("claude_team") => Some("team".to_string()),
        Some("claude_enterprise") => Some("enterprise".to_string()),
        _ => None,
    }
}

/// Mint a Claude CLI API key from an OAuth access token.
pub async fn derive_cli_api_key(access_token: &str) -> Result<String, Box<dyn std::error::Error>> {
    create_cli_api_key(access_token).await
}

/// Look up the subscription type for an OAuth access token.
pub async fn resolve_subscription_type(access_token: &str) -> Option<String> {
    fetch_subscription_type(access_token).await
}

fn extract_code_from_manual_input(
    input: &str,
    expected_state: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("No authorization code provided".into());
    }

    if let Some((code, state)) = trimmed.split_once('#')
        && !trimmed.contains("://")
    {
        if state != expected_state {
            return Err("OAuth state mismatch — possible CSRF. Try /login again.".into());
        }
        return Ok(code.to_string());
    }

    if trimmed.contains("://") || trimmed.contains("code=") || trimmed.contains("state=") {
        let query_start = trimmed.find('?').map(|idx| idx + 1).unwrap_or(0);
        let query = &trimmed[query_start..];
        let fragment = query.split('#').next().unwrap_or(query);
        let params: std::collections::HashMap<String, String> = fragment
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .map(|(key, value)| {
                (
                    key.to_string(),
                    urlencoding::decode(value)
                        .map(|value| value.into_owned())
                        .unwrap_or_else(|_| value.to_string()),
                )
            })
            .collect();

        if let Some(error) = params.get("error") {
            let description = params
                .get("error_description")
                .map(String::as_str)
                .unwrap_or("unknown error");
            return Err(format!("OAuth error: {error} - {description}").into());
        }

        if let Some(state) = params.get("state")
            && state != expected_state
        {
            return Err("OAuth state mismatch — possible CSRF. Try /login again.".into());
        }

        if let Some(code) = params.get("code") {
            return Ok(code.clone());
        }
    }

    Ok(trimmed.to_string())
}

/// Extract the authorization code from the HTTP request on the callback.
/// Validates the `state` parameter against the expected value to prevent CSRF attacks.
fn extract_code_from_request(
    stream: &mut std::net::TcpStream,
    expected_state: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    // Parse: GET /callback?code=XYZ&state=ABC HTTP/1.1
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or("Invalid HTTP request")?;

    let query = path
        .split_once('?')
        .map(|(_, q)| q)
        .ok_or("No query parameters in callback")?;

    let params: std::collections::HashMap<&str, &str> = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .collect();

    if let Some(error) = params.get("error") {
        let description = params.get("error_description").unwrap_or(&"unknown error");
        return Err(format!(
            "OAuth error: {} - {}",
            urlencoding::decode(error)?,
            urlencoding::decode(description)?
        )
        .into());
    }

    let received_state = params
        .get("state")
        .ok_or("Missing state parameter in OAuth callback")?;
    if urlencoding::decode(received_state)? != expected_state {
        return Err("OAuth state mismatch — possible CSRF attack".into());
    }

    let code = params
        .get("code")
        .ok_or("No authorization code in callback")?;
    Ok(urlencoding::decode(code)?.into_owned())
}

/// Save OAuth credentials to keychain (macOS) and credentials file.
fn save_credentials(
    access_token: &str,
    refresh_token: Option<&str>,
    api_key: Option<&str>,
    subscription_type: Option<&str>,
    expires_in_secs: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;

    let creds = serde_json::json!({
        "claudeAiOauth": {
            "accessToken": access_token,
            "refreshToken": refresh_token.unwrap_or(""),
            "apiKey": api_key.unwrap_or(""),
            "expiresAt": now_ms + (expires_in_secs * 1000),
            "subscriptionType": subscription_type.unwrap_or("unknown")
        }
    });

    let creds_str = serde_json::to_string(&creds)?;

    if cfg!(target_os = "macos")
        && let Ok(user) = std::env::var("USER")
    {
        let _ = std::process::Command::new("security")
            .args([
                "delete-generic-password",
                "-a",
                &user,
                "-s",
                "Claude Code-credentials",
            ])
            .output();

        std::process::Command::new("security")
            .args([
                "add-generic-password",
                "-a",
                &user,
                "-s",
                "Claude Code-credentials",
                "-w",
                &creds_str,
            ])
            .output()?;
    }

    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let creds_path = home.join(".claude/.credentials.json");
    if let Some(parent) = creds_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&creds_path)?
            .write_all(creds_str.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&creds_path, &creds_str)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_length() {
        let verifier = generate_code_verifier();
        assert!(verifier.len() >= 43);
    }

    #[test]
    fn test_code_challenge_is_base64url() {
        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        assert_eq!(challenge.len(), 43);
        assert!(
            challenge
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        );
    }

    #[test]
    fn test_state_not_empty() {
        let state = generate_state();
        assert!(!state.is_empty());
    }

    #[test]
    fn test_extract_code_from_manual_fragment() {
        let state = "expected-state";
        let code = extract_code_from_manual_input("auth-code#expected-state", state).unwrap();
        assert_eq!(code, "auth-code");
    }

    #[test]
    fn test_extract_code_from_manual_url() {
        let state = "expected-state";
        let input =
            "https://platform.claude.com/oauth/code/callback?code=auth-code&state=expected-state";
        let code = extract_code_from_manual_input(input, state).unwrap();
        assert_eq!(code, "auth-code");
    }
}
