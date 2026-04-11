use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

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
    #[serde(default)]
    pub volume_percent: Option<u32>,
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

/// Token state is behind a Mutex so API methods can take `&self` and
/// the client can be wrapped in an Arc for concurrent command handling.
struct TokenState {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_at: u64,
    /// Set when a token refresh occurs, cleared by `take_refreshed_token`.
    pending_refresh_token: Option<String>,
}

pub struct SpotifyClient {
    http: reqwest::Client,
    client_id: String,
    tokens: Mutex<TokenState>,
    cached_user: Mutex<Option<UserProfile>>,
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
            tokens: Mutex::new(TokenState {
                access_token: None,
                refresh_token: None,
                expires_at: 0,
                pending_refresh_token: None,
            }),
            cached_user: Mutex::new(None),
        }
    }

    pub async fn set_tokens(&self, tokens: &oauth::OAuthTokens) {
        let mut t = self.tokens.lock().await;
        t.access_token = Some(tokens.access_token.clone());
        t.refresh_token = Some(tokens.refresh_token.clone());
        t.expires_at = tokens.expires_at;
    }

    fn is_expired(expires_at: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now >= expires_at.saturating_sub(60)
    }

    /// Returns a valid access token, refreshing if expired. Holds the token
    /// mutex across the refresh HTTP call so concurrent callers block on a
    /// single refresh -- refreshes are rare (once per ~hour) so contention
    /// is negligible. Fast path (token still valid) releases the lock
    /// immediately, allowing concurrent API calls.
    async fn ensure_token(&self) -> Result<String, SpotifyError> {
        let mut t = self.tokens.lock().await;
        if !Self::is_expired(t.expires_at) {
            return t.access_token.clone().ok_or(SpotifyError::NotAuthenticated);
        }

        let refresh = t
            .refresh_token
            .clone()
            .ok_or(SpotifyError::NotAuthenticated)?;

        log::info!("Access token expired, refreshing");
        let new_tokens = oauth::refresh_access_token(&self.client_id, &refresh)
            .await
            .map_err(|e| SpotifyError::RefreshFailed(e.to_string()))?;

        let access = new_tokens.access_token.clone();
        t.access_token = Some(new_tokens.access_token);
        t.refresh_token = Some(new_tokens.refresh_token.clone());
        t.expires_at = new_tokens.expires_at;
        t.pending_refresh_token = Some(new_tokens.refresh_token);
        Ok(access)
    }

    /// Force a refresh on the next `ensure_token` call (used after a 401).
    async fn invalidate_token(&self) {
        let mut t = self.tokens.lock().await;
        t.expires_at = 0;
    }

    /// Returns the refresh token from the most recent token refresh, if any.
    /// Clears the pending state so the caller can persist it exactly once.
    pub async fn take_refreshed_token(&self) -> Option<String> {
        let mut t = self.tokens.lock().await;
        t.pending_refresh_token.take()
    }

    async fn api_get(&self, url: &str) -> Result<reqwest::Response, SpotifyError> {
        let token = self.ensure_token().await?;
        let resp = self.http.get(url).bearer_auth(&token).send().await?;

        // If we get a 401 after a fresh token, try one more refresh
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify, attempting token refresh");
            self.invalidate_token().await;
            let token = self.ensure_token().await?;
            let resp = self.http.get(url).bearer_auth(&token).send().await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn api_post(&self, url: &str, body: serde_json::Value) -> Result<reqwest::Response, SpotifyError> {
        let token = self.ensure_token().await?;
        let resp = self.http.post(url).bearer_auth(&token).json(&body).send().await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify POST, attempting token refresh");
            self.invalidate_token().await;
            let token = self.ensure_token().await?;
            let resp = self.http.post(url).bearer_auth(&token).json(&body).send().await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn api_put(&self, url: &str, body: serde_json::Value) -> Result<reqwest::Response, SpotifyError> {
        let token = self.ensure_token().await?;
        let resp = self.http.put(url).bearer_auth(&token).json(&body).send().await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify PUT, attempting token refresh");
            self.invalidate_token().await;
            let token = self.ensure_token().await?;
            let resp = self.http.put(url).bearer_auth(&token).json(&body).send().await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn api_delete(&self, url: &str, body: serde_json::Value) -> Result<reqwest::Response, SpotifyError> {
        let token = self.ensure_token().await?;
        let resp = self.http.delete(url).bearer_auth(&token).json(&body).send().await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 from Spotify DELETE, attempting token refresh");
            self.invalidate_token().await;
            let token = self.ensure_token().await?;
            let resp = self.http.delete(url).bearer_auth(&token).json(&body).send().await?;
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

    async fn api_post_empty(&self, url: &str) -> Result<reqwest::Response, SpotifyError> {
        let token = self.ensure_token().await?;
        let resp = self.http.post(url)
            .bearer_auth(&token)
            .header("Content-Length", "0")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 on POST, attempting token refresh");
            self.invalidate_token().await;
            let token = self.ensure_token().await?;
            let resp = self.http.post(url)
                .bearer_auth(&token)
                .header("Content-Length", "0")
                .send()
                .await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    async fn api_put_empty(&self, url: &str) -> Result<reqwest::Response, SpotifyError> {
        let token = self.ensure_token().await?;
        let resp = self.http.put(url)
            .bearer_auth(&token)
            .header("Content-Length", "0")
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            log::warn!("Got 401 on PUT, attempting token refresh");
            self.invalidate_token().await;
            let token = self.ensure_token().await?;
            let resp = self.http.put(url)
                .bearer_auth(&token)
                .header("Content-Length", "0")
                .send()
                .await?;
            return self.check_response(resp).await;
        }

        self.check_response(resp).await
    }

    pub async fn get_now_playing(&self) -> Result<Option<NowPlaying>, SpotifyError> {
        let url = format!("{}/me/player/currently-playing", API_BASE);
        let resp = self.api_get(&url).await?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let body: NowPlaying = resp.json().await?;
        Ok(Some(body))
    }

    pub async fn get_queue(&self) -> Result<QueueResponse, SpotifyError> {
        let url = format!("{}/me/player/queue", API_BASE);
        let resp = self.api_get(&url).await?;
        let body: QueueResponse = resp.json().await?;
        Ok(body)
    }

    pub async fn search(
        &self,
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

    pub async fn add_to_queue(&self, track_uri: &str) -> Result<(), SpotifyError> {
        let url = format!(
            "{}/me/player/queue?uri={}",
            API_BASE,
            crate::oauth::urlencoding::encode(track_uri)
        );
        self.api_post_empty(&url).await?;
        Ok(())
    }

    pub async fn get_playback_state(&self) -> Result<Option<PlaybackState>, SpotifyError> {
        let url = format!("{}/me/player", API_BASE);
        let resp = self.api_get(&url).await?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let body: PlaybackState = resp.json().await?;
        Ok(Some(body))
    }

    pub async fn get_current_user(&self) -> Result<UserProfile, SpotifyError> {
        {
            let cached = self.cached_user.lock().await;
            if let Some(profile) = cached.as_ref() {
                return Ok(profile.clone());
            }
        }

        let url = format!("{}/me", API_BASE);
        let resp = self.api_get(&url).await?;
        let profile: UserProfile = resp.json().await?;
        let mut cached = self.cached_user.lock().await;
        *cached = Some(profile.clone());
        Ok(profile)
    }

    pub async fn get_playlist_tracks(
        &self,
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
        &self,
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
        &self,
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
        &self,
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
        &self,
        artist_ids: &[String],
    ) -> Result<GetArtistsResponse, SpotifyError> {
        let ids = artist_ids.join(",");
        let url = format!("{}/artists?ids={}", API_BASE, ids);
        let resp = self.api_get(&url).await?;
        let body: GetArtistsResponse = resp.json().await?;
        Ok(body)
    }

    pub async fn get_playlist_details(
        &self,
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
        &self,
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

    pub async fn pause(&self) -> Result<(), SpotifyError> {
        let url = format!("{}/me/player/pause", API_BASE);
        self.api_put_empty(&url).await?;
        Ok(())
    }

    pub async fn resume(&self) -> Result<(), SpotifyError> {
        let url = format!("{}/me/player/play", API_BASE);
        self.api_put_empty(&url).await?;
        Ok(())
    }

    pub async fn skip_next(&self) -> Result<(), SpotifyError> {
        let url = format!("{}/me/player/next", API_BASE);
        self.api_post_empty(&url).await?;
        Ok(())
    }

    pub async fn skip_previous(&self) -> Result<(), SpotifyError> {
        let url = format!("{}/me/player/previous", API_BASE);
        self.api_post_empty(&url).await?;
        Ok(())
    }

    pub async fn set_volume(&self, volume_percent: u32) -> Result<(), SpotifyError> {
        let url = format!(
            "{}/me/player/volume?volume_percent={}",
            API_BASE,
            volume_percent.min(100)
        );
        self.api_put_empty(&url).await?;
        Ok(())
    }
}
