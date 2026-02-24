use thiserror::Error;

#[derive(Debug, Error)]
pub enum SpotifyError {
    #[error("Not authenticated")]
    NotAuthenticated,
    #[error("Spotify API error: {status} {message}")]
    Api { status: u16, message: String },
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub struct SpotifyClient {
    http: reqwest::Client,
    access_token: Option<String>,
}

impl SpotifyClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
            access_token: None,
        }
    }

    pub fn set_access_token(&mut self, token: String) {
        self.access_token = Some(token);
    }

    pub async fn get_now_playing(&self) -> Result<serde_json::Value, SpotifyError> {
        let _token = self.access_token.as_ref().ok_or(SpotifyError::NotAuthenticated)?;
        todo!("GET /v1/me/player/currently-playing")
    }

    pub async fn get_queue(&self) -> Result<serde_json::Value, SpotifyError> {
        let _token = self.access_token.as_ref().ok_or(SpotifyError::NotAuthenticated)?;
        todo!("GET /v1/me/player/queue")
    }

    pub async fn search(&self, _query: &str) -> Result<serde_json::Value, SpotifyError> {
        let _token = self.access_token.as_ref().ok_or(SpotifyError::NotAuthenticated)?;
        todo!("GET /v1/search")
    }

    pub async fn add_to_queue(&self, _track_uri: &str) -> Result<(), SpotifyError> {
        let _token = self.access_token.as_ref().ok_or(SpotifyError::NotAuthenticated)?;
        todo!("POST /v1/me/player/queue")
    }
}
