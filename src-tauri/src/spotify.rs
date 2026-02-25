use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::oauth;

const API_BASE: &str = "https://api.spotify.com/v1";

#[derive(Debug, Error)]
pub enum SpotifyError {
    #[error("Not authenticated")]
    NotAuthenticated,
    #[error("Spotify API error: {status} {message}")]
    Api { status: u16, message: String },
    #[error("Token refresh failed: {0}")]
    RefreshFailed(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub uri: String,
    pub duration_ms: u64,
    pub artists: Vec<Artist>,
    pub album: Album,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub images: Vec<Image>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub url: String,
    pub height: Option<u32>,
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowPlaying {
    pub is_playing: bool,
    pub progress_ms: Option<u64>,
    pub item: Option<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueResponse {
    pub currently_playing: Option<Track>,
    #[serde(default)]
    pub queue: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub tracks: SearchTracks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTracks {
    pub items: Vec<Track>,
    pub total: u32,
}

pub struct SpotifyClient {
    http: reqwest::Client,
    client_id: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: u64,
}

impl SpotifyClient {
    pub fn new(client_id: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            client_id,
            access_token: None,
            refresh_token: None,
            expires_at: 0,
        }
    }

    pub fn set_tokens(&mut self, tokens: &oauth::OAuthTokens) {
        self.access_token = Some(tokens.access_token.clone());
        self.refresh_token = Some(tokens.refresh_token.clone());
        self.expires_at = tokens.expires_at;
    }

    pub fn has_tokens(&self) -> bool {
        self.access_token.is_some()
    }

    fn is_token_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at.saturating_sub(60)
    }

    /// Refresh the access token if it's expired. Returns the new tokens if refreshed.
    pub async fn ensure_token(&mut self) -> Result<Option<oauth::OAuthTokens>, SpotifyError> {
        if !self.is_token_expired() {
            return Ok(None);
        }

        let refresh = self
            .refresh_token
            .as_ref()
            .ok_or(SpotifyError::NotAuthenticated)?
            .clone();

        log::info!("Access token expired, refreshing");
        let tokens = oauth::refresh_access_token(&self.client_id, &refresh)
            .await
            .map_err(|e| SpotifyError::RefreshFailed(e.to_string()))?;

        self.set_tokens(&tokens);
        Ok(Some(tokens))
    }

    fn auth_header(&self) -> Result<String, SpotifyError> {
        let token = self
            .access_token
            .as_ref()
            .ok_or(SpotifyError::NotAuthenticated)?;
        Ok(format!("Bearer {}", token))
    }

    async fn api_get(&mut self, url: &str) -> Result<reqwest::Response, SpotifyError> {
        self.ensure_token().await?;
        let auth = self.auth_header()?;
        let resp = self.http.get(url).header("Authorization", &auth).send().await?;

        // If we get a 401 after a fresh token, try one more refresh
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify, attempting token refresh");
            self.expires_at = 0; // force refresh
            self.ensure_token().await?;
            let auth = self.auth_header()?;
            let resp = self.http.get(url).header("Authorization", &auth).send().await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn check_response(
        &self,
        resp: reqwest::Response,
    ) -> Result<reqwest::Response, SpotifyError> {
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(SpotifyError::Api {
            status,
            message: body,
        })
    }

    pub async fn get_now_playing(&mut self) -> Result<Option<NowPlaying>, SpotifyError> {
        let url = format!("{}/me/player/currently-playing", API_BASE);
        let resp = self.api_get(&url).await?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let body: NowPlaying = resp.json().await?;
        Ok(Some(body))
    }

    pub async fn get_queue(&mut self) -> Result<QueueResponse, SpotifyError> {
        let url = format!("{}/me/player/queue", API_BASE);
        let resp = self.api_get(&url).await?;
        let body: QueueResponse = resp.json().await?;
        Ok(body)
    }

    pub async fn search(
        &mut self,
        query: &str,
        limit: u32,
    ) -> Result<SearchResponse, SpotifyError> {
        let url = format!(
            "{}/search?q={}&type=track&limit={}",
            API_BASE,
            crate::oauth::urlencoding::encode(query),
            limit.min(50)
        );
        let resp = self.api_get(&url).await?;
        let body: SearchResponse = resp.json().await?;
        Ok(body)
    }

    pub async fn add_to_queue(&mut self, track_uri: &str) -> Result<(), SpotifyError> {
        self.ensure_token().await?;
        let auth = self.auth_header()?;
        let url = format!(
            "{}/me/player/queue?uri={}",
            API_BASE,
            crate::oauth::urlencoding::encode(track_uri)
        );
        let resp = self
            .http
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Length", "0")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(());
        }

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 on add_to_queue, refreshing token");
            // Force a retry - but don't recurse, just report the error
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(SpotifyError::Api {
                status,
                message: body,
            });
        }

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(SpotifyError::Api {
                status,
                message: body,
            });
        }

        Ok(())
    }
}
