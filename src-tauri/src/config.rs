use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::Runtime;
use tauri_plugin_store::Store;

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

impl AppConfig {
    pub fn from_store<R: Runtime>(store: &Arc<Store<R>>) -> Self {
        let websocket_url = store
            .get("websocket_url")
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

        Self {
            websocket_url,
            spotify_client_id,
            redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
            poll_interval_secs,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.websocket_url.is_empty() && !self.spotify_client_id.is_empty()
    }
}
