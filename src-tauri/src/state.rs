use serde::Serialize;

use crate::config::AppConfig;

#[derive(Debug, Default, Serialize)]
pub struct AppState {
    pub spotify_status: ConnectionStatus,
    pub websocket_status: ConnectionStatus,
    pub now_playing: Option<NowPlayingInfo>,
    pub last_error: Option<String>,
    #[serde(skip)]
    pub config: AppConfig,
    #[serde(skip)]
    pub spotify_refresh_token: Option<String>,
    #[serde(skip)]
    pub relay_shutdown: Option<tokio::sync::watch::Sender<bool>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NowPlayingInfo {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub album_art_url: Option<String>,
    pub is_playing: bool,
    pub progress_ms: Option<u64>,
    pub duration_ms: u64,
    pub track_uri: String,
}

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub enum ConnectionStatus {
    #[default]
    Disconnected,
    Connecting,
    Connected,
}

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionStatus::Disconnected => write!(f, "disconnected"),
            ConnectionStatus::Connecting => write!(f, "connecting"),
            ConnectionStatus::Connected => write!(f, "connected"),
        }
    }
}
