use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("Token fetch failed: {0}")]
    FetchFailed(String),
    #[error("Invalid JWT: {0}")]
    InvalidJwt(String),
    #[error("Invalid server URL: {0}")]
    InvalidUrl(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// Response from the server's token endpoint.
pub struct ConnectionToken {
    pub token: String,
    pub channel: String,
}

/// Fetch a Centrifugo connection token and channel from the server.
pub async fn fetch_centrifugo_token(
    server_url: &str,
    api_key: &str,
) -> Result<ConnectionToken, TokenError> {
    let url = format!(
        "{}/api/connector/token",
        server_url.trim_end_matches('/')
    );
    let http = reqwest::Client::new();
    let resp = http
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(TokenError::FetchFailed(format!("HTTP {} - {}", status, body)));
    }

    let body: serde_json::Value = resp.json().await?;

    let token = body["token"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| TokenError::FetchFailed("Missing token in response".to_string()))?;

    let channel = body["channel"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| TokenError::FetchFailed("Missing channel in response".to_string()))?;

    Ok(ConnectionToken { token, channel })
}

/// Decode the claims (payload) from a JWT without verifying the signature.
fn decode_jwt_claims(token: &str) -> Result<serde_json::Value, TokenError> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(TokenError::InvalidJwt("Expected 3 parts".to_string()));
    }

    // Strip any padding and decode
    let raw = parts[1].trim_end_matches('=');
    let payload = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|e| TokenError::InvalidJwt(format!("Base64 decode failed: {}", e)))?;

    serde_json::from_slice(&payload)
        .map_err(|e| TokenError::InvalidJwt(format!("JSON parse failed: {}", e)))
}

/// Derive the Centrifugo WebSocket URL from the server base URL.
pub fn derive_websocket_url(server_url: &str) -> Result<String, TokenError> {
    let url = server_url.trim_end_matches('/');
    let ws_url = if url.starts_with("https://") {
        url.replacen("https://", "wss://", 1)
    } else if url.starts_with("http://") {
        url.replacen("http://", "ws://", 1)
    } else {
        return Err(TokenError::InvalidUrl(
            "Server URL must start with http:// or https://".to_string(),
        ));
    };
    Ok(format!("{}/connection/websocket", ws_url))
}

/// Extract the `exp` claim from a JWT, returning seconds since UNIX epoch.
/// Returns None if the claim is missing or the token is malformed.
pub fn token_expiry(token: &str) -> Option<u64> {
    let claims = decode_jwt_claims(token).ok()?;
    claims["exp"].as_u64()
}

/// Fetch all connection parameters needed for a Centrifugo connection.
/// Returns (websocket_url, token, channel).
pub async fn fetch_connection_params(
    server_url: &str,
    api_key: &str,
) -> Result<(String, String, String), TokenError> {
    let ws_url = derive_websocket_url(server_url)?;
    let conn = fetch_centrifugo_token(server_url, api_key).await?;
    Ok((ws_url, conn.token, conn.channel))
}
