use std::sync::Mutex;

use relay_core::state::AppState;
use relay_core::RelayPlatform;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_store::StoreExt;

pub struct TauriPlatform {
    app: tauri::AppHandle,
}

impl TauriPlatform {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl RelayPlatform for TauriPlatform {
    fn persist_refresh_token(&self, token: &str) {
        if let Ok(store) = self.app.store("config.json") {
            let _ = store.set("spotify_refresh_token", serde_json::json!(token));
            let _ = store.save();
        }
    }

    fn get_refresh_token(&self) -> Option<String> {
        self.app
            .store("config.json")
            .ok()
            .and_then(|store| {
                store
                    .get("spotify_refresh_token")
                    .and_then(|v| v.as_str().map(String::from))
            })
    }

    fn clear_refresh_token(&self) {
        if let Ok(store) = self.app.store("config.json") {
            let _ = store.delete("spotify_refresh_token");
            let _ = store.save();
        }
    }

    fn update_state<F: FnOnce(&mut AppState) + Send>(&self, f: F) {
        if let Some(state) = self.app.try_state::<Mutex<AppState>>() {
            if let Ok(mut state) = state.lock() {
                f(&mut state);
            }
        }
    }

    fn emit_status(&self) {
        if let Some(state) = self.app.try_state::<Mutex<AppState>>() {
            if let Ok(state) = state.lock() {
                let payload = serde_json::json!({
                    "spotify": state.spotify_status,
                    "websocket": state.websocket_status,
                    "now_playing": state.now_playing,
                    "last_error": state.last_error,
                });
                let _ = self.app.emit("status-changed", payload);
            }
        }
    }

    fn notify(&self, title: &str, body: &str) {
        let _ = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }

    fn present_auth_url(&self, url: &str) {
        #[cfg(target_os = "linux")]
        {
            let _ = std::process::Command::new("xdg-open")
                .arg(url)
                .env_remove("LD_LIBRARY_PATH")
                .env_remove("GIO_LAUNCHED_DESKTOP_FILE")
                .spawn();
            return;
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = open::that(url);
        }
    }
}
