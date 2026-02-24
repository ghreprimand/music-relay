use serde::Serialize;

#[derive(Debug, Default, Serialize)]
pub struct AppState {
    pub spotify_status: ConnectionStatus,
    pub websocket_status: ConnectionStatus,
    pub now_playing: Option<String>,
}

#[derive(Debug, Default, Serialize)]
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
