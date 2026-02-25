use std::sync::Mutex;

use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

use crate::config::AppConfig;
use crate::oauth;
use crate::spotify::SpotifyClient;
use crate::state::{AppState, ConnectionStatus, NowPlayingInfo};

/// Start the background relay task. Returns a shutdown sender to stop it.
pub fn start_relay(
    app: tauri::AppHandle,
    config: AppConfig,
) -> tokio::sync::watch::Sender<bool> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_relay(app.clone(), config, shutdown_rx).await {
            log::error!("Relay task failed: {}", e);
            update_state(&app, |state| {
                state.last_error = Some(e.to_string());
                state.spotify_status = ConnectionStatus::Disconnected;
                state.websocket_status = ConnectionStatus::Disconnected;
            });
            emit_status(&app);
        }
    });

    shutdown_tx
}

async fn run_relay(
    app: tauri::AppHandle,
    config: AppConfig,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Authenticate with Spotify
    update_state(&app, |state| {
        state.spotify_status = ConnectionStatus::Connecting;
        state.last_error = None;
    });
    emit_status(&app);

    let tokens = authenticate_spotify(&app, &config).await?;

    let mut spotify = SpotifyClient::new(config.spotify_client_id.clone());
    spotify.set_tokens(&tokens);

    // Persist the refresh token
    persist_refresh_token(&app, &tokens.refresh_token);

    update_state(&app, |state| {
        state.spotify_status = ConnectionStatus::Connected;
        state.spotify_refresh_token = Some(tokens.refresh_token.clone());
    });
    emit_status(&app);

    log::info!("Spotify authenticated, starting poll loop");

    // Step 2: Poll loop
    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.max(1));
    let mut interval = tokio::time::interval(poll_interval);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                match spotify.get_now_playing().await {
                    Ok(Some(np)) => {
                        let info = np.item.as_ref().map(|track| NowPlayingInfo {
                            track_name: track.name.clone(),
                            artist_name: track.artists.iter()
                                .map(|a| a.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                            album_name: track.album.name.clone(),
                            album_art_url: track.album.images.first().map(|i| i.url.clone()),
                            is_playing: np.is_playing,
                            progress_ms: np.progress_ms,
                            duration_ms: track.duration_ms,
                            track_uri: track.uri.clone(),
                        });
                        update_state(&app, |state| {
                            state.now_playing = info;
                        });
                        emit_status(&app);

                        // If token was refreshed during the call, persist it
                        if let Ok(Some(new_tokens)) = spotify.ensure_token().await {
                            persist_refresh_token(&app, &new_tokens.refresh_token);
                        }
                    }
                    Ok(None) => {
                        update_state(&app, |state| {
                            state.now_playing = None;
                        });
                        emit_status(&app);
                    }
                    Err(e) => {
                        log::warn!("Failed to get now playing: {}", e);
                        // Don't crash the loop on transient errors
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    log::info!("Relay shutdown requested");
                    break;
                }
            }
        }
    }

    update_state(&app, |state| {
        state.spotify_status = ConnectionStatus::Disconnected;
        state.websocket_status = ConnectionStatus::Disconnected;
    });
    emit_status(&app);

    Ok(())
}

async fn authenticate_spotify(
    app: &tauri::AppHandle,
    config: &AppConfig,
) -> Result<oauth::OAuthTokens, Box<dyn std::error::Error + Send + Sync>> {
    // Try to refresh existing token first
    let existing_refresh = {
        let store = app.store("config.json").map_err(|e| e.to_string())?;
        store
            .get("spotify_refresh_token")
            .and_then(|v| v.as_str().map(String::from))
    };

    if let Some(refresh_token) = existing_refresh {
        log::info!("Found existing refresh token, attempting refresh");
        match oauth::refresh_access_token(&config.spotify_client_id, &refresh_token).await {
            Ok(tokens) => {
                log::info!("Token refresh successful");
                return Ok(tokens);
            }
            Err(e) => {
                log::warn!("Token refresh failed ({}), starting fresh OAuth flow", e);
            }
        }
    }

    // No existing token or refresh failed -- start full OAuth flow
    let tokens = oauth::start_oauth_flow(&config.spotify_client_id, &config.redirect_uri).await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    Ok(tokens)
}

fn persist_refresh_token(app: &tauri::AppHandle, refresh_token: &str) {
    if let Ok(store) = app.store("config.json") {
        let _ = store.set("spotify_refresh_token", serde_json::json!(refresh_token));
        let _ = store.save();
    }
}

fn update_state<F: FnOnce(&mut AppState)>(app: &tauri::AppHandle, f: F) {
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        if let Ok(mut state) = state.lock() {
            f(&mut state);
        }
    }
}

fn emit_status(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        if let Ok(state) = state.lock() {
            let payload = serde_json::json!({
                "spotify": state.spotify_status,
                "websocket": state.websocket_status,
                "now_playing": state.now_playing,
                "last_error": state.last_error,
            });
            let _ = app.emit("status-changed", payload);
        }
    }
}
