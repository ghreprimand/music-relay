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
    #[serde(default)]
    pub popularity: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub progress_ms: Option<u64>,
    pub item: Option<Track>,
    pub context: Option<PlaybackContext>,
    pub shuffle_state: Option<bool>,
    pub device: Option<Device>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybackContext {
    #[serde(rename = "type")]
    pub context_type: String,
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Option<String>,
    pub name: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistTracksResponse {
    pub items: Vec<PlaylistItem>,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub track: Option<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePlaylistResponse {
    pub id: String,
    pub external_urls: ExternalUrls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalUrls {
    pub spotify: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtistDetail {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub genres: Vec<String>,
    #[serde(default)]
    pub popularity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetArtistsResponse {
    pub artists: Vec<Option<ArtistDetail>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistOwner {
    pub id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistTracksTotal {
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDetails {
    pub id: String,
    pub name: String,
    pub owner: PlaylistOwner,
    pub tracks: PlaylistTracksTotal,
    pub external_urls: ExternalUrls,
}

pub struct SpotifyClient {
    http: reqwest::Client,
    client_id: String,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: u64,
    cached_user: Option<UserProfile>,
    /// Set when a token refresh occurs, cleared by `take_refreshed_token`.
    pending_refresh_token: Option<String>,
}

impl SpotifyClient {
    pub fn new(client_id: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        Self {
            http,
            client_id,
            access_token: None,
            refresh_token: None,
            expires_at: 0,
            cached_user: None,
            pending_refresh_token: None,
        }
    }

    pub fn set_tokens(&mut self, tokens: &oauth::OAuthTokens) {
        self.access_token = Some(tokens.access_token.clone());
        self.refresh_token = Some(tokens.refresh_token.clone());
        self.expires_at = tokens.expires_at;
    }

    fn is_token_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= self.expires_at.saturating_sub(60)
    }

    /// Refresh the access token if it's expired.
    pub async fn ensure_token(&mut self) -> Result<(), SpotifyError> {
        if !self.is_token_expired() {
            return Ok(());
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

        self.pending_refresh_token = Some(tokens.refresh_token.clone());
        self.set_tokens(&tokens);
        Ok(())
    }

    /// Returns the refresh token from the most recent token refresh, if any.
    /// Clears the pending state so the caller can persist it exactly once.
    pub fn take_refreshed_token(&mut self) -> Option<String> {
        self.pending_refresh_token.take()
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

    async fn api_post(&mut self, url: &str, body: serde_json::Value) -> Result<reqwest::Response, SpotifyError> {
        self.ensure_token().await?;
        let auth = self.auth_header()?;
        let resp = self.http.post(url)
            .header("Authorization", &auth)
            .json(&body)
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify POST, attempting token refresh");
            self.expires_at = 0;
            self.ensure_token().await?;
            let auth = self.auth_header()?;
            let resp = self.http.post(url)
                .header("Authorization", &auth)
                .json(&body)
                .send()
                .await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn api_put(&mut self, url: &str, body: serde_json::Value) -> Result<reqwest::Response, SpotifyError> {
        self.ensure_token().await?;
        let auth = self.auth_header()?;
        let resp = self.http.put(url)
            .header("Authorization", &auth)
            .json(&body)
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify PUT, attempting token refresh");
            self.expires_at = 0;
            self.ensure_token().await?;
            let auth = self.auth_header()?;
            let resp = self.http.put(url)
                .header("Authorization", &auth)
                .json(&body)
                .send()
                .await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn api_delete(&mut self, url: &str, body: serde_json::Value) -> Result<reqwest::Response, SpotifyError> {
        self.ensure_token().await?;
        let auth = self.auth_header()?;
        let resp = self.http.delete(url)
            .header("Authorization", &auth)
            .json(&body)
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify DELETE, attempting token refresh");
            self.expires_at = 0;
            self.ensure_token().await?;
            let auth = self.auth_header()?;
            let resp = self.http.delete(url)
                .header("Authorization", &auth)
                .json(&body)
                .send()
                .await?;
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

        let resp = self.http.post(&url)
            .header("Authorization", &auth)
            .header("Content-Length", "0")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 on add_to_queue, attempting token refresh");
            self.expires_at = 0;
            self.ensure_token().await?;
            let auth = self.auth_header()?;
            let resp = self.http.post(&url)
                .header("Authorization", &auth)
                .header("Content-Length", "0")
                .send()
                .await?;
            self.check_response(resp).await?;
            return Ok(());
        }

        self.check_response(resp).await?;
        Ok(())
    }

    pub async fn get_playback_state(&mut self) -> Result<Option<PlaybackState>, SpotifyError> {
        let url = format!("{}/me/player", API_BASE);
        let resp = self.api_get(&url).await?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let body: PlaybackState = resp.json().await?;
        Ok(Some(body))
    }

    pub async fn get_current_user(&mut self) -> Result<UserProfile, SpotifyError> {
        if let Some(ref profile) = self.cached_user {
            return Ok(profile.clone());
        }

        let url = format!("{}/me", API_BASE);
        let resp = self.api_get(&url).await?;
        let profile: UserProfile = resp.json().await?;
        self.cached_user = Some(profile.clone());
        Ok(profile)
    }

    pub async fn get_playlist_tracks(
        &mut self,
        playlist_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<PlaylistTracksResponse, SpotifyError> {
        let limit = limit.min(100);
        let url = format!(
            "{}/playlists/{}/tracks?offset={}&limit={}&fields=items(track(id,name,uri,duration_ms,artists(id,name),album(id,name,images))),total",
            API_BASE, playlist_id, offset, limit
        );
        let resp = self.api_get(&url).await?;
        let body: PlaylistTracksResponse = resp.json().await?;
        Ok(body)
    }

    pub async fn add_to_playlist(
        &mut self,
        playlist_id: &str,
        uris: Vec<String>,
        position: Option<u32>,
    ) -> Result<String, SpotifyError> {
        let url = format!("{}/playlists/{}/tracks", API_BASE, playlist_id);
        let mut snapshot_id = String::new();

        for chunk in uris.chunks(100) {
            let mut body = serde_json::json!({ "uris": chunk });
            if let Some(pos) = position {
                body["position"] = serde_json::json!(pos);
            }
            let resp = self.api_post(&url, body).await?;
            let result: serde_json::Value = resp.json().await?;
            snapshot_id = result["snapshot_id"].as_str().unwrap_or("").to_string();
        }

        Ok(snapshot_id)
    }

    pub async fn remove_from_playlist(
        &mut self,
        playlist_id: &str,
        uris: Vec<String>,
    ) -> Result<String, SpotifyError> {
        let url = format!("{}/playlists/{}/tracks", API_BASE, playlist_id);
        let mut snapshot_id = String::new();

        for chunk in uris.chunks(100) {
            let tracks: Vec<serde_json::Value> = chunk
                .iter()
                .map(|uri| serde_json::json!({ "uri": uri }))
                .collect();
            let body = serde_json::json!({ "tracks": tracks });
            let resp = self.api_delete(&url, body).await?;
            let result: serde_json::Value = resp.json().await?;
            snapshot_id = result["snapshot_id"].as_str().unwrap_or("").to_string();
        }

        Ok(snapshot_id)
    }

    pub async fn replace_playlist_tracks(
        &mut self,
        playlist_id: &str,
        uris: Vec<String>,
    ) -> Result<String, SpotifyError> {
        let url = format!("{}/playlists/{}/tracks", API_BASE, playlist_id);

        // PUT the first 100 (replaces all existing tracks)
        let first_chunk: Vec<&String> = uris.iter().take(100).collect();
        let body = serde_json::json!({ "uris": first_chunk });
        let resp = self.api_put(&url, body).await?;
        let result: serde_json::Value = resp.json().await?;
        let mut snapshot_id = result["snapshot_id"].as_str().unwrap_or("").to_string();

        // POST remaining in batches of 100
        if uris.len() > 100 {
            for chunk in uris[100..].chunks(100) {
                let body = serde_json::json!({ "uris": chunk });
                let resp = self.api_post(&url, body).await?;
                let result: serde_json::Value = resp.json().await?;
                snapshot_id = result["snapshot_id"].as_str().unwrap_or("").to_string();
            }
        }

        Ok(snapshot_id)
    }

    pub async fn get_artists(
        &mut self,
        artist_ids: &[String],
    ) -> Result<GetArtistsResponse, SpotifyError> {
        let ids = artist_ids.join(",");
        let url = format!("{}/artists?ids={}", API_BASE, ids);
        let resp = self.api_get(&url).await?;
        let body: GetArtistsResponse = resp.json().await?;
        Ok(body)
    }

    pub async fn get_playlist_details(
        &mut self,
        playlist_id: &str,
    ) -> Result<PlaylistDetails, SpotifyError> {
        let url = format!(
            "{}/playlists/{}?fields=id,name,owner(id,display_name),tracks(total),external_urls",
            API_BASE, playlist_id
        );
        let resp = self.api_get(&url).await?;
        let body: PlaylistDetails = resp.json().await?;
        Ok(body)
    }

    pub async fn create_playlist(
        &mut self,
        name: &str,
        description: Option<&str>,
        public: bool,
    ) -> Result<CreatePlaylistResponse, SpotifyError> {
        let user = self.get_current_user().await?;
        let url = format!("{}/users/{}/playlists", API_BASE, user.id);

        let mut body = serde_json::json!({
            "name": name,
            "public": public,
        });
        if let Some(desc) = description {
            body["description"] = serde_json::json!(desc);
        }

        let resp = self.api_post(&url, body).await?;
        let playlist: CreatePlaylistResponse = resp.json().await?;
        Ok(playlist)
    }
}
