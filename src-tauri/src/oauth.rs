use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const SPOTIFY_AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const SPOTIFY_TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const SCOPES: &str = "user-read-currently-playing user-read-playback-state user-modify-playback-state";
const CALLBACK_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),
    #[error("Callback listener error: {0}")]
    CallbackError(String),
    #[error("Callback timed out (user did not complete authorization)")]
    CallbackTimeout,
    #[error("State mismatch (possible CSRF)")]
    StateMismatch,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

pub fn generate_pkce() -> (String, String) {
    let chars: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    let verifier: String = (0..128)
        .map(|_| chars[rng.gen_range(0..chars.len())] as char)
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(hasher.finalize());

    (verifier, challenge)
}

fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    let chars: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    (0..32).map(|_| chars[rng.gen_range(0..chars.len())] as char).collect()
}

pub fn build_auth_url(client_id: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
    format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge_method=S256&code_challenge={}&state={}",
        SPOTIFY_AUTH_URL,
        client_id,
        urlencoding::encode(redirect_uri),
        urlencoding::encode(SCOPES),
        challenge,
        state,
    )
}

pub async fn wait_for_callback(expected_state: &str) -> Result<String, OAuthError> {
    let listener = TcpListener::bind("127.0.0.1:18974").await?;

    let accept_future = listener.accept();
    let (mut stream, _) = tokio::time::timeout(
        std::time::Duration::from_secs(CALLBACK_TIMEOUT_SECS),
        accept_future,
    )
    .await
    .map_err(|_| OAuthError::CallbackTimeout)?
    .map_err(|e| OAuthError::CallbackError(e.to_string()))?;

    let mut buf = vec![0u8; 4096];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let first_line = request.lines().next().ok_or_else(|| {
        OAuthError::CallbackError("Empty request".to_string())
    })?;

    let path = first_line.split_whitespace().nth(1).ok_or_else(|| {
        OAuthError::CallbackError("No path in request".to_string())
    })?;

    let query_str = path.split('?').nth(1).ok_or_else(|| {
        OAuthError::CallbackError("No query string in callback".to_string())
    })?;

    let params: std::collections::HashMap<&str, &str> = query_str
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            Some((parts.next()?, parts.next()?))
        })
        .collect();

    // Check for error response from Spotify
    if let Some(error) = params.get("error") {
        let html = format!(
            "<html><body><h3>Authorization denied.</h3><p>{}</p><p>You may close this window.</p></body></html>",
            error
        );
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(), html
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        return Err(OAuthError::AuthorizationFailed(error.to_string()));
    }

    let code = params
        .get("code")
        .ok_or_else(|| OAuthError::CallbackError("No code parameter".to_string()))?
        .to_string();

    let state = params.get("state").map(|s| *s).unwrap_or("");
    if state != expected_state {
        let html = "<html><body><h3>Authentication failed.</h3><p>State mismatch. Please try again.</p></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            html.len(), html
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        return Err(OAuthError::StateMismatch);
    }

    let html = "<html><body><h3>Authentication complete.</h3><p>You may close this window and return to Music Relay.</p></body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(), html
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;

    Ok(code)
}

pub async fn exchange_code(
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<OAuthTokens, OAuthError> {
    let http = reqwest::Client::new();
    let resp = http
        .post(SPOTIFY_TOKEN_URL)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("code_verifier", code_verifier),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(OAuthError::TokenExchangeFailed(format!(
            "HTTP {} - {}",
            status, body
        )));
    }

    let body: serde_json::Value = resp.json().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Ok(OAuthTokens {
        access_token: body["access_token"]
            .as_str()
            .ok_or_else(|| OAuthError::TokenExchangeFailed("Missing access_token".to_string()))?
            .to_string(),
        refresh_token: body["refresh_token"]
            .as_str()
            .ok_or_else(|| OAuthError::TokenExchangeFailed("Missing refresh_token".to_string()))?
            .to_string(),
        expires_at: now + body["expires_in"].as_u64().unwrap_or(3600),
    })
}

pub async fn refresh_access_token(
    client_id: &str,
    refresh_token: &str,
) -> Result<OAuthTokens, OAuthError> {
    let http = reqwest::Client::new();
    let resp = http
        .post(SPOTIFY_TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(OAuthError::TokenRefreshFailed(format!(
            "HTTP {} - {}",
            status, body
        )));
    }

    let body: serde_json::Value = resp.json().await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Spotify may or may not return a new refresh_token
    let new_refresh = body["refresh_token"]
        .as_str()
        .unwrap_or(refresh_token)
        .to_string();

    Ok(OAuthTokens {
        access_token: body["access_token"]
            .as_str()
            .ok_or_else(|| OAuthError::TokenRefreshFailed("Missing access_token".to_string()))?
            .to_string(),
        refresh_token: new_refresh,
        expires_at: now + body["expires_in"].as_u64().unwrap_or(3600),
    })
}

/// Run the full OAuth PKCE flow: open browser, wait for callback, exchange code.
pub async fn start_oauth_flow(
    client_id: &str,
    redirect_uri: &str,
) -> Result<OAuthTokens, OAuthError> {
    let (verifier, challenge) = generate_pkce();
    let state = generate_state();
    let auth_url = build_auth_url(client_id, redirect_uri, &challenge, &state);

    log::info!("Opening browser for Spotify authorization");
    open::that(&auth_url).map_err(|e| {
        OAuthError::AuthorizationFailed(format!("Failed to open browser: {}", e))
    })?;

    log::info!("Waiting for OAuth callback on 127.0.0.1:18974");
    let code = wait_for_callback(&state).await?;

    log::info!("Exchanging authorization code for tokens");
    let tokens = exchange_code(client_id, &code, redirect_uri, &verifier).await?;

    Ok(tokens)
}

pub mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut result = String::with_capacity(input.len() * 3);
        for byte in input.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", byte));
                }
            }
        }
        result
    }
}
