use std::sync::Arc;

use relay_core::config::RelayConfig;
use tauri::Runtime;
use tauri_plugin_store::Store;

#[derive(Debug, Clone)]
pub struct TauriAppConfig {
    pub relay: RelayConfig,
    pub close_to_tray: bool,
}

impl TauriAppConfig {
    pub fn from_store<R: Runtime>(store: &Arc<Store<R>>) -> Self {
        // Clean up legacy keys from pre-1.3.0 configs
        for key in &["websocket_url", "websocket_token", "websocket_channel"] {
            if store.has(*key) {
                let _ = store.delete(*key);
            }
        }

        let server_url = store
            .get("server_url")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();

        let api_key = store
            .get("api_key")
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
            relay: RelayConfig {
                server_url,
                api_key,
                spotify_client_id,
                redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
                poll_interval_secs,
            },
            close_to_tray,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.relay.is_configured()
    }
}
