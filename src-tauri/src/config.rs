use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub websocket_url: String,
    pub spotify_client_id: String,
    pub redirect_uri: String,
    pub poll_interval_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            websocket_url: String::new(),
            spotify_client_id: String::new(),
            redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
            poll_interval_secs: 5,
        }
    }
}
