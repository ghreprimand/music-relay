use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const SPOTIFY_AUTH_URL: &str = "https://accounts.spotify.com/authorize";
const SPOTIFY_TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const SCOPES: &str = "user-read-currently-playing user-read-playback-state user-modify-playback-state playlist-read-private playlist-read-collaborative playlist-modify-public playlist-modify-private";
const CALLBACK_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("Authorization failed: {0}")]
    AuthorizationFailed(String),
    #[error("Token exchange failed: {0}")]
    TokenExchangeFailed(String),
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),
    #[error("Spotify authorization is required")]
    InvalidGrant,
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

impl OAuthError {
    pub fn is_invalid_grant(&self) -> bool {
        matches!(self, OAuthError::InvalidGrant)
    }
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
    (0..32)
        .map(|_| chars[rng.gen_range(0..chars.len())] as char)
        .collect()
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

    let first_line = request
        .lines()
        .next()
        .ok_or_else(|| OAuthError::CallbackError("Empty request".to_string()))?;

    let path = first_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| OAuthError::CallbackError("No path in request".to_string()))?;

    let query_str = path
        .split('?')
        .nth(1)
        .ok_or_else(|| OAuthError::CallbackError("No query string in callback".to_string()))?;

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
    refresh_access_token_from_url(SPOTIFY_TOKEN_URL, client_id, refresh_token).await
}

async fn refresh_access_token_from_url(
    token_url: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<OAuthTokens, OAuthError> {
    let http = reqwest::Client::new();
    let resp = http
        .post(token_url)
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
        if spotify_error_code(&body).as_deref() == Some("invalid_grant") {
            return Err(OAuthError::InvalidGrant);
        }
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

fn spotify_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.as_str())
                .map(str::to_string)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn token_server(status: &str, body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let status = status.to_string();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
        });

        format!("http://{}/api/token", addr)
    }

    #[tokio::test]
    async fn refresh_invalid_grant_is_structured() {
        let url = token_server(
            "400 Bad Request",
            r#"{"error":"invalid_grant","error_description":"Refresh token expired"}"#,
        )
        .await;

        let err = refresh_access_token_from_url(&url, "client", "expired")
            .await
            .unwrap_err();

        assert!(err.is_invalid_grant());
    }

    #[tokio::test]
    async fn refresh_transient_failure_is_not_invalid_grant() {
        let url = token_server("500 Internal Server Error", r#"{"error":"server_error"}"#).await;

        let err = refresh_access_token_from_url(&url, "client", "refresh")
            .await
            .unwrap_err();

        assert!(!err.is_invalid_grant());
        assert!(matches!(err, OAuthError::TokenRefreshFailed(_)));
    }

    #[tokio::test]
    async fn refresh_success_uses_returned_refresh_token() {
        let url = token_server(
            "200 OK",
            r#"{"access_token":"access","refresh_token":"fresh-refresh","expires_in":3600}"#,
        )
        .await;

        let tokens = refresh_access_token_from_url(&url, "client", "old-refresh")
            .await
            .unwrap();

        assert_eq!(tokens.access_token, "access");
        assert_eq!(tokens.refresh_token, "fresh-refresh");
    }
}

/// Run the full OAuth PKCE flow: present auth URL, wait for callback, exchange code.
/// The `present_url` callback is called with the authorization URL -- the platform
/// decides how to show it (open browser, print to console, etc.).
pub async fn start_oauth_flow(
    client_id: &str,
    redirect_uri: &str,
    present_url: impl FnOnce(&str) + Send,
) -> Result<OAuthTokens, OAuthError> {
    let (verifier, challenge) = generate_pkce();
    let state = generate_state();
    let auth_url = build_auth_url(client_id, redirect_uri, &challenge, &state);

    log::info!("Presenting Spotify authorization URL");
    present_url(&auth_url);

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
