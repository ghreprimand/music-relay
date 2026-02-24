use thiserror::Error;

#[derive(Debug, Error)]
pub enum OAuthError {
    #[error("PKCE flow failed: {0}")]
    PkceFlowFailed(String),
    #[error("Token refresh failed: {0}")]
    TokenRefreshFailed(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
}

/// Start the Spotify OAuth PKCE flow.
/// Opens a browser to the Spotify authorization page and listens on localhost
/// for the redirect callback.
pub async fn start_oauth_flow(
    _client_id: &str,
    _redirect_uri: &str,
) -> Result<OAuthTokens, OAuthError> {
    todo!("implement PKCE flow")
}

/// Refresh an expired access token using the stored refresh token.
pub async fn refresh_access_token(
    _client_id: &str,
    _refresh_token: &str,
) -> Result<OAuthTokens, OAuthError> {
    todo!("implement token refresh")
}
