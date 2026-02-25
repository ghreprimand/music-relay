use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tauri_plugin_store::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub websocket_url: String,
    pub websocket_token: String,
    pub websocket_channel: String,
    pub spotify_client_id: String,
    pub redirect_uri: String,
    pub poll_interval_secs: u64,
    pub close_to_tray: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            websocket_url: String::new(),
            websocket_token: String::new(),
            websocket_channel: String::new(),
            spotify_client_id: String::new(),
            redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
            poll_interval_secs: 5,
            close_to_tray: true,
        }
    }
}

impl AppConfig {
    pub fn from_store<R: Runtime>(store: &Arc<Store<R>>) -> Self {
        let websocket_url = store
            .get("websocket_url")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        let websocket_token = store
            .get("websocket_token")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        let websocket_channel = store
            .get("websocket_channel")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        let spotify_client_id = store
            .get("spotify_client_id")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        let poll_interval_secs = store
            .get("poll_interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(5);

        let close_to_tray = store
            .get("close_to_tray")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        Self {
            websocket_url,
            websocket_token,
            websocket_channel,
            spotify_client_id,
            redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
            poll_interval_secs,
            close_to_tray,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.websocket_url.is_empty() && !self.spotify_client_id.is_empty()
    }
}
