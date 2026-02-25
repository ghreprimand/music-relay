use std::sync::Mutex;

use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::mpsc;

use crate::centrifugo::{self, CentrifugoClient, CommandError, CommandResponse, ServerCommand};
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
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
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

    persist_refresh_token(&app, &tokens.refresh_token);

    update_state(&app, |state| {
        state.spotify_status = ConnectionStatus::Connected;
        state.spotify_refresh_token = Some(tokens.refresh_token.clone());
    });
    emit_status(&app);
    log::info!("Spotify authenticated");

    // Step 2: Connect to Centrifugo (if configured)
    let has_websocket = !config.websocket_url.is_empty()
        && !config.websocket_token.is_empty()
        && !config.websocket_channel.is_empty();

    if has_websocket {
        run_with_centrifugo(app, config, &mut spotify, shutdown_rx).await
    } else {
        run_poll_only(app, config, &mut spotify, shutdown_rx).await
    }
}

/// Full mode: Spotify polling + Centrifugo command dispatch.
async fn run_with_centrifugo(
    app: tauri::AppHandle,
    config: AppConfig,
    spotify: &mut SpotifyClient,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut reconnect_attempt: u32 = 0;

    loop {
        // Check shutdown before each connection attempt
        if *shutdown_rx.borrow() {
            return Ok(());
        }

        update_state(&app, |state| {
            state.websocket_status = ConnectionStatus::Connecting;
        });
        emit_status(&app);

        let client = CentrifugoClient::new(
            config.websocket_url.clone(),
            config.websocket_token.clone(),
            config.websocket_channel.clone(),
        );

        let (command_tx, mut command_rx) = mpsc::channel::<ServerCommand>(32);
        let (response_tx, response_rx) = mpsc::channel::<CommandResponse>(32);

        let ws_shutdown = shutdown_rx.clone();
        let ws_handle = tauri::async_runtime::spawn(async move {
            client.connect_and_run(command_tx, response_rx, ws_shutdown).await
        });

        // Give the connection a moment to establish or fail fast
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Check if command_rx is already closed (connection failed immediately)
        // We do this by trying a non-blocking recv
        match command_rx.try_recv() {
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // The WebSocket task ended - get the error
                let result = ws_handle.await.map_err(|e| e.to_string())?;
                if let Err(e) = result {
                    log::warn!("Centrifugo connection failed: {}", e);
                    update_state(&app, |state| {
                        state.websocket_status = ConnectionStatus::Disconnected;
                        state.last_error = Some(format!("WebSocket: {}", e));
                    });
                    emit_status(&app);

                    let delay = centrifugo::backoff_delay(reconnect_attempt);
                    reconnect_attempt += 1;
                    log::info!("Reconnecting in {:?}", delay);

                    tokio::select! {
                        _ = tokio::time::sleep(delay) => continue,
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { return Ok(()); }
                        }
                    }
                    continue;
                }
                // Ended without error (shouldn't happen this fast, but handle it)
                continue;
            }
            _ => {
                // Channel still open or has a message - connection is alive
            }
        }

        reconnect_attempt = 0;
        update_state(&app, |state| {
            state.websocket_status = ConnectionStatus::Connected;
            state.last_error = None;
        });
        emit_status(&app);
        log::info!("Centrifugo connected");

        // Run the combined poll + command loop
        let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.max(1));
        let mut ticker = tokio::time::interval(poll_interval);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    poll_now_playing(&app, spotify).await;
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(cmd) => {
                            let resp = handle_command(spotify, cmd).await;
                            if response_tx.send(resp).await.is_err() {
                                log::warn!("Response channel closed, WebSocket likely disconnected");
                                break;
                            }
                        }
                        None => {
                            // command_tx dropped = WebSocket task ended
                            log::info!("Command channel closed, WebSocket disconnected");
                            break;
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        update_state(&app, |state| {
                            state.spotify_status = ConnectionStatus::Disconnected;
                            state.websocket_status = ConnectionStatus::Disconnected;
                        });
                        emit_status(&app);
                        return Ok(());
                    }
                }
            }
        }

        // If we get here, the WebSocket disconnected. Clean up and reconnect.
        update_state(&app, |state| {
            state.websocket_status = ConnectionStatus::Disconnected;
        });
        emit_status(&app);

        let delay = centrifugo::backoff_delay(reconnect_attempt);
        reconnect_attempt += 1;
        log::info!("WebSocket lost, reconnecting in {:?}", delay);

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { return Ok(()); }
            }
        }
    }
}

/// Spotify-only mode: just poll now-playing without a WebSocket connection.
async fn run_poll_only(
    app: tauri::AppHandle,
    config: AppConfig,
    spotify: &mut SpotifyClient,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Running in poll-only mode (no WebSocket configured)");

    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.max(1));
    let mut interval = tokio::time::interval(poll_interval);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                poll_now_playing(&app, spotify).await;
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    log::info!("Relay shutdown requested");
                    update_state(&app, |state| {
                        state.spotify_status = ConnectionStatus::Disconnected;
                    });
                    emit_status(&app);
                    return Ok(());
                }
            }
        }
    }
}

async fn poll_now_playing(app: &tauri::AppHandle, spotify: &mut SpotifyClient) {
    match spotify.get_now_playing().await {
        Ok(Some(np)) => {
            let info = np.item.as_ref().map(|track| NowPlayingInfo {
                track_name: track.name.clone(),
                artist_name: track
                    .artists
                    .iter()
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
            update_state(app, |state| {
                state.now_playing = info;
            });
            emit_status(app);

            // Persist refreshed token if applicable
            if let Ok(Some(new_tokens)) = spotify.ensure_token().await {
                persist_refresh_token(app, &new_tokens.refresh_token);
            }
        }
        Ok(None) => {
            update_state(app, |state| {
                state.now_playing = None;
            });
            emit_status(app);
        }
        Err(e) => {
            log::warn!("Failed to get now playing: {}", e);
        }
    }
}

async fn handle_command(spotify: &mut SpotifyClient, cmd: ServerCommand) -> CommandResponse {
    match cmd {
        ServerCommand::GetNowPlaying { id } => {
            match spotify.get_now_playing().await {
                Ok(np) => CommandResponse {
                    id,
                    result: Some(serde_json::to_value(&np).unwrap_or_default()),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::GetQueue { id } => {
            match spotify.get_queue().await {
                Ok(queue) => CommandResponse {
                    id,
                    result: Some(serde_json::to_value(&queue).unwrap_or_default()),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::Search { id, query } => {
            match spotify.search(&query, 20).await {
                Ok(results) => CommandResponse {
                    id,
                    result: Some(serde_json::to_value(&results).unwrap_or_default()),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::AddToQueue { id, track_uri } => {
            match spotify.add_to_queue(&track_uri).await {
                Ok(()) => CommandResponse {
                    id,
                    result: Some(serde_json::json!({"success": true})),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
    }
}

fn error_response(id: String, code: &str, message: &str) -> CommandResponse {
    CommandResponse {
        id,
        result: None,
        error: Some(CommandError {
            code: code.to_string(),
            message: message.to_string(),
        }),
    }
}

async fn authenticate_spotify(
    app: &tauri::AppHandle,
    config: &AppConfig,
) -> Result<oauth::OAuthTokens, Box<dyn std::error::Error + Send + Sync>> {
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

    let tokens = oauth::start_oauth_flow(&config.spotify_client_id, &config.redirect_uri)
        .await
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
