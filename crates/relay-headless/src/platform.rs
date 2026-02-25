use std::path::PathBuf;
use std::sync::Mutex;

use relay_core::state::AppState;
use relay_core::RelayPlatform;

use crate::config::HeadlessConfig;

pub struct HeadlessPlatform {
    state: Mutex<AppState>,
    config_path: PathBuf,
}

impl HeadlessPlatform {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            state: Mutex::new(AppState::default()),
            config_path,
        }
    }
}

impl RelayPlatform for HeadlessPlatform {
    fn persist_refresh_token(&self, token: &str) {
        if let Ok(mut config) = HeadlessConfig::load(&self.config_path) {
            config.refresh_token = Some(token.to_string());
            let _ = config.save(&self.config_path);
        }
    }

    fn get_refresh_token(&self) -> Option<String> {
        HeadlessConfig::load(&self.config_path)
            .ok()
            .and_then(|c| c.refresh_token)
    }

    fn clear_refresh_token(&self) {
        if let Ok(mut config) = HeadlessConfig::load(&self.config_path) {
            config.refresh_token = None;
            let _ = config.save(&self.config_path);
        }
    }

    fn update_state<F: FnOnce(&mut AppState) + Send>(&self, f: F) {
        if let Ok(mut state) = self.state.lock() {
            f(&mut state);
        }
    }

    fn emit_status(&self) {
        if let Ok(state) = self.state.lock() {
            let spotify = &state.spotify_status;
            let ws = &state.websocket_status;

            if let Some(ref np) = state.now_playing {
                if np.is_playing && !np.track_name.is_empty() {
                    log::info!(
                        "[{}|{}] Now playing: {} - {}",
                        spotify, ws, np.artist_name, np.track_name
                    );
                    return;
                }
            }

            if let Some(ref err) = state.last_error {
                log::warn!("[{}|{}] Error: {}", spotify, ws, err);
            } else {
                log::info!("[{}|{}]", spotify, ws);
            }
        }
    }

    fn notify(&self, title: &str, body: &str) {
        log::warn!("{}: {}", title, body);
    }

    fn present_auth_url(&self, url: &str) {
        println!("\nOpen this URL in your browser to authorize Spotify:\n");
        println!("  {}\n", url);
        println!("Waiting for authorization...");
    }
}
