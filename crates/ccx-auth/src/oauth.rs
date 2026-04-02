use std::io::{BufRead, BufReader, Write};
// TcpListener removed — using paste-code flow instead of local callback server

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use sha2::{Digest, Sha256};

const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const AUTH_URL: &str = "https://claude.com/cai/oauth/authorize";
const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const SCOPES: &str = "org:create_api_key user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";

/// OAuth tokens returned after a successful login.
#[derive(Debug)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Run the full OAuth Authorization Code + PKCE flow.
/// Opens a browser, waits for callback, exchanges code for tokens, saves credentials.
pub async fn login() -> Result<OAuthTokens, Box<dyn std::error::Error>> {
    // 1. Generate PKCE verifier and challenge.
    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();

    // 2. Use the platform callback URL (registered redirect URI).
    // Claude's OAuth server only accepts this, NOT localhost.
    let manual_redirect = "https://platform.claude.com/oauth/code/callback";

    // 3. Build authorize URL matching Claude Code's EXACT format.
    let auth_url = format!(
        "{}?code=true&client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&state={}",
        AUTH_URL,
        CLIENT_ID,
        urlencoding::encode(manual_redirect),
        urlencoding::encode(SCOPES),
        code_challenge,
        state,
    );

    // 4. Open browser.
    println!("Opening browser for authentication...");
    if let Err(e) = open::that(&auth_url) {
        eprintln!("Failed to open browser: {e}");
    }

    // 5. Show paste code prompt (the platform page shows a code after auth).
    println!();
    println!("After signing in, you'll see a code on the page.");
    println!("Paste the code here:");
    print!("> ");
    std::io::Write::flush(&mut std::io::stdout())?;

    let mut code_input = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(std::io::stdin()), &mut code_input)?;
    let code_input = code_input.trim();

    // The pasted code format is: {authorization_code}#{state}
    let (code, received_state) = if let Some((c, s)) = code_input.split_once('#') {
        (c.to_string(), s.to_string())
    } else {
        // Just the code without state — use as-is
        (code_input.to_string(), state.clone())
    };

    if received_state != state && code_input.contains('#') {
        return Err("OAuth state mismatch — possible CSRF. Try /login again.".into());
    }

    let redirect_uri = manual_redirect.to_string();

    // 6. Exchange code for token.

    // Exchange code for token (JSON body, matching Claude Code's format).
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
        let body = token_response.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {body}").into());
    }

    let tokens: serde_json::Value = token_response.json().await?;

    let access_token = tokens["access_token"]
        .as_str()
        .ok_or("No access_token in response")?
        .to_string();
    let refresh_token = tokens["refresh_token"].as_str().map(|s| s.to_string());

    // 7. Save credentials.
    save_credentials(&access_token, refresh_token.as_deref())?;

    println!("\x1b[32m✓ Logged in successfully!\x1b[0m");

    Ok(OAuthTokens {
        access_token,
        refresh_token,
    })
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

    // Collect query params into a map for easier access.
    let params: std::collections::HashMap<&str, &str> = query
        .split('&')
        .filter_map(|p| p.split_once('='))
        .collect();

    // Check for error parameter first.
    if let Some(error) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(&"unknown error");
        return Err(format!(
            "OAuth error: {} - {}",
            urlencoding::decode(error)?,
            urlencoding::decode(desc)?
        )
        .into());
    }

    // Validate state parameter to prevent CSRF attacks.
    let received_state = params
        .get("state")
        .ok_or("Missing state parameter in OAuth callback")?;
    if urlencoding::decode(received_state)? != expected_state {
        return Err("OAuth state mismatch \u{2014} possible CSRF attack".into());
    }

    // Extract authorization code.
    let code = params
        .get("code")
        .ok_or("No authorization code in callback")?;
    Ok(urlencoding::decode(code)?.into_owned())
}

/// Save OAuth credentials to keychain (macOS) and credentials file.
fn save_credentials(
    access_token: &str,
    refresh_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as i64;

    let creds = serde_json::json!({
        "claudeAiOauth": {
            "accessToken": access_token,
            "refreshToken": refresh_token.unwrap_or(""),
            "expiresAt": now_ms + 3_600_000, // 1 hour
            "subscriptionType": "unknown"
        }
    });

    let creds_str = serde_json::to_string(&creds)?;

    // macOS: save to Keychain.
    if cfg!(target_os = "macos")
        && let Ok(user) = std::env::var("USER")
    {
        // Delete existing entry (ignore errors).
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

    // Also save to file as fallback (restricted permissions on Unix).
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
        let v = generate_code_verifier();
        assert!(v.len() >= 43);
    }

    #[test]
    fn test_code_challenge_is_base64url() {
        let v = generate_code_verifier();
        let c = generate_code_challenge(&v);
        // S256 hash is 32 bytes = 43 base64url chars (no padding).
        assert_eq!(c.len(), 43);
        assert!(
            c.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        );
    }

    #[test]
    fn test_state_not_empty() {
        let s = generate_state();
        assert!(!s.is_empty());
    }
}
