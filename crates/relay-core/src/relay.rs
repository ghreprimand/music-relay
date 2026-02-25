use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::centrifugo::{self, CentrifugoClient, CommandError, CommandResponse, ServerCommand};
use crate::config::RelayConfig;
use crate::oauth;
use crate::spotify::SpotifyClient;
use crate::state::{AppState, ConnectionStatus, NowPlayingInfo};
use crate::token;

const MAX_RELAY_RETRIES: u32 = 5;

/// Platform abstraction that decouples relay logic from Tauri or any specific runtime.
pub trait RelayPlatform: Send + Sync + 'static {
    fn persist_refresh_token(&self, token: &str);
    fn get_refresh_token(&self) -> Option<String>;
    fn clear_refresh_token(&self);
    fn update_state<F: FnOnce(&mut AppState) + Send>(&self, f: F);
    fn emit_status(&self);
    fn notify(&self, title: &str, body: &str);
    fn present_auth_url(&self, url: &str);
}

/// Create the relay task. Returns a shutdown sender and a future to spawn.
/// The caller is responsible for spawning the future on an appropriate runtime
/// (e.g. `tokio::spawn` for headless, `tauri::async_runtime::spawn` for Tauri).
pub fn start_relay<P: RelayPlatform>(
    platform: Arc<P>,
    config: RelayConfig,
) -> (
    tokio::sync::watch::Sender<bool>,
    impl std::future::Future<Output = ()> + Send + 'static,
) {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let future = async move {
        let mut attempt: u32 = 0;
        let mut shutdown_rx = shutdown_rx;
        let connected = Arc::new(AtomicBool::new(false));

        loop {
            if *shutdown_rx.borrow() {
                return;
            }

            connected.store(false, Ordering::Relaxed);

            match run_relay(&*platform, &config, shutdown_rx.clone(), connected.clone()).await {
                Ok(()) => return,
                Err(e) => {
                    // Reset retry counter if the relay had been running successfully
                    if connected.load(Ordering::Relaxed) {
                        attempt = 0;
                    }
                    attempt += 1;
                    log::error!(
                        "Relay failed (attempt {}/{}): {}",
                        attempt,
                        MAX_RELAY_RETRIES,
                        e
                    );

                    platform.update_state(|state| {
                        state.last_error = Some(e.to_string());
                        state.spotify_status = ConnectionStatus::Disconnected;
                        state.websocket_status = ConnectionStatus::Disconnected;
                    });
                    platform.emit_status();

                    if attempt >= MAX_RELAY_RETRIES {
                        log::error!("Relay exceeded max retries, giving up");
                        // Clear the bad refresh token so next launch triggers
                        // a fresh OAuth flow instead of repeating the same failure
                        platform.clear_refresh_token();
                        platform.notify(
                            "Music Relay -- Connection Failed",
                            "Spotify song requests are no longer being relayed. Open Music Relay to reconnect.",
                        );
                        return;
                    }

                    let delay = centrifugo::backoff_delay(attempt - 1);
                    log::info!("Retrying relay in {:?}", delay);

                    tokio::select! {
                        _ = tokio::time::sleep(delay) => {}
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { return; }
                        }
                    }
                }
            }
        }
    };

    (shutdown_tx, future)
}

async fn run_relay<P: RelayPlatform>(
    platform: &P,
    config: &RelayConfig,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    connected: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Authenticate with Spotify
    platform.update_state(|state| {
        state.spotify_status = ConnectionStatus::Connecting;
        state.last_error = None;
    });
    platform.emit_status();

    let tokens = authenticate_spotify(platform, config).await?;

    let mut spotify = SpotifyClient::new(config.spotify_client_id.clone());
    spotify.set_tokens(&tokens);

    platform.persist_refresh_token(&tokens.refresh_token);

    platform.update_state(|state| {
        state.spotify_status = ConnectionStatus::Connected;
    });
    platform.emit_status();
    log::info!("Spotify authenticated");

    // Signal that we got past startup -- retry counter will reset on failure
    connected.store(true, Ordering::Relaxed);

    // Step 2: Connect to Centrifugo (if configured)
    let has_server = !config.server_url.is_empty() && !config.api_key.is_empty();

    if has_server {
        run_with_centrifugo(platform, config, &mut spotify, shutdown_rx).await
    } else {
        run_poll_only(platform, config, &mut spotify, shutdown_rx).await
    }
}

/// Full mode: Spotify polling + Centrifugo command dispatch.
async fn run_with_centrifugo<P: RelayPlatform>(
    platform: &P,
    config: &RelayConfig,
    spotify: &mut SpotifyClient,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut reconnect_attempt: u32 = 0;

    loop {
        // Check shutdown before each connection attempt
        if *shutdown_rx.borrow() {
            return Ok(());
        }

        platform.update_state(|state| {
            state.websocket_status = ConnectionStatus::Connecting;
        });
        platform.emit_status();

        // Fetch fresh token and derive connection params on every connect/reconnect
        let (ws_url, centrifugo_token, channel) = match token::fetch_connection_params(
            &config.server_url,
            &config.api_key,
        )
        .await
        {
            Ok(params) => params,
            Err(e) => {
                log::warn!("Token fetch failed: {}", e);
                platform.update_state(|state| {
                    state.websocket_status = ConnectionStatus::Disconnected;
                    state.last_error = Some(format!("Token: {}", e));
                });
                platform.emit_status();

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
        };

        // Schedule a proactive reconnect 1 hour before the token expires.
        // This avoids the brief disconnect when Centrifugo drops us at expiry.
        let token_refresh_deadline = match token::token_expiry(&centrifugo_token) {
            Some(exp) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let refresh_at = exp.saturating_sub(3600);
                if refresh_at > now {
                    let secs = refresh_at - now;
                    log::info!("Token expires in {}h, will refresh in {}h", (exp - now) / 3600, secs / 3600);
                    Some(tokio::time::Instant::now() + std::time::Duration::from_secs(secs))
                } else {
                    log::warn!("Token expiry is already within the refresh window, no proactive refresh scheduled");
                    None
                }
            }
            None => {
                log::warn!("Could not determine token expiry, proactive refresh disabled");
                None
            }
        };

        let client = CentrifugoClient::new(ws_url, centrifugo_token, channel);

        let (command_tx, mut command_rx) = mpsc::channel::<ServerCommand>(32);
        let (response_tx, response_rx) = mpsc::channel::<CommandResponse>(32);

        let ws_shutdown = shutdown_rx.clone();
        let ws_handle = tokio::spawn(async move {
            client.connect_and_run(command_tx, response_rx, ws_shutdown).await
        });

        // Give the connection a moment to establish or fail fast
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Check if command_rx is already closed (connection failed immediately)
        match command_rx.try_recv() {
            Err(mpsc::error::TryRecvError::Disconnected) => {
                // The WebSocket task ended - get the error
                let result = ws_handle.await.map_err(|e| e.to_string())?;
                if let Err(e) = result {
                    log::warn!("Centrifugo connection failed: {}", e);
                    platform.update_state(|state| {
                        state.websocket_status = ConnectionStatus::Disconnected;
                        state.last_error = Some(format!("WebSocket: {}", e));
                    });
                    platform.emit_status();

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
        platform.update_state(|state| {
            state.websocket_status = ConnectionStatus::Connected;
            state.last_error = None;
        });
        platform.emit_status();
        log::info!("Centrifugo connected");

        // Run the combined poll + command loop
        let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.max(1));
        let mut ticker = tokio::time::interval(poll_interval);
        let mut last_track_uri: Option<String> = None;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    let broadcast = poll_now_playing(platform, spotify, &mut last_track_uri).await;
                    if let Some(data) = broadcast {
                        let msg = CommandResponse {
                            id: String::new(),
                            result: Some(serde_json::json!({
                                "type": "now_playing",
                                "data": data
                            })),
                            error: None,
                        };
                        if response_tx.send(msg).await.is_err() {
                            log::warn!("Response channel closed during broadcast");
                            break;
                        }
                    }
                }
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(cmd) => {
                            let resp = handle_command(spotify, cmd).await;
                            persist_if_refreshed(platform, spotify);
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
                _ = async {
                    match token_refresh_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {
                    log::info!("Token approaching expiry, proactively reconnecting");
                    break;
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        platform.update_state(|state| {
                            state.spotify_status = ConnectionStatus::Disconnected;
                            state.websocket_status = ConnectionStatus::Disconnected;
                        });
                        platform.emit_status();
                        return Ok(());
                    }
                }
            }
        }

        // If we get here, the WebSocket disconnected (or token refresh triggered).
        // Clean up and reconnect.
        platform.update_state(|state| {
            state.websocket_status = ConnectionStatus::Disconnected;
        });
        platform.emit_status();

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
async fn run_poll_only<P: RelayPlatform>(
    platform: &P,
    config: &RelayConfig,
    spotify: &mut SpotifyClient,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("Running in poll-only mode (no server configured)");

    let poll_interval = std::time::Duration::from_secs(config.poll_interval_secs.max(1));
    let mut interval = tokio::time::interval(poll_interval);
    let mut last_track_uri: Option<String> = None;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                poll_now_playing(platform, spotify, &mut last_track_uri).await;
                persist_if_refreshed(platform, spotify);
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    log::info!("Relay shutdown requested");
                    platform.update_state(|state| {
                        state.spotify_status = ConnectionStatus::Disconnected;
                    });
                    platform.emit_status();
                    return Ok(());
                }
            }
        }
    }
}

/// Poll Spotify for now-playing. Returns Some(info) when the track changes
/// (for broadcasting over WebSocket).
async fn poll_now_playing<P: RelayPlatform>(
    platform: &P,
    spotify: &mut SpotifyClient,
    last_track_uri: &mut Option<String>,
) -> Option<NowPlayingInfo> {
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

            // Detect track change
            let current_uri = np.item.as_ref().map(|t| t.uri.clone());
            let changed = current_uri != *last_track_uri;
            *last_track_uri = current_uri;

            platform.update_state(|state| {
                state.now_playing = info.clone();
            });
            platform.emit_status();

            if let Some(token) = spotify.take_refreshed_token() {
                platform.persist_refresh_token(&token);
            }

            if changed { info } else { None }
        }
        Ok(None) => {
            let changed = last_track_uri.is_some();
            *last_track_uri = None;
            platform.update_state(|state| {
                state.now_playing = None;
            });
            platform.emit_status();
            if changed {
                // Broadcast that playback stopped
                Some(NowPlayingInfo {
                    track_name: String::new(),
                    artist_name: String::new(),
                    album_name: String::new(),
                    album_art_url: None,
                    is_playing: false,
                    progress_ms: None,
                    duration_ms: 0,
                    track_uri: String::new(),
                })
            } else {
                None
            }
        }
        Err(e) => {
            log::warn!("Failed to get now playing: {}", e);
            platform.update_state(|state| {
                state.last_error = Some(format!("Spotify: {}", e));
            });
            platform.emit_status();
            None
        }
    }
}

fn command_name(cmd: &ServerCommand) -> &'static str {
    match cmd {
        ServerCommand::GetNowPlaying { .. } => "get_now_playing",
        ServerCommand::GetQueue { .. } => "get_queue",
        ServerCommand::Search { .. } => "search",
        ServerCommand::AddToQueue { .. } => "add_to_queue",
        ServerCommand::GetPlaybackState { .. } => "get_playback_state",
        ServerCommand::GetPlaylistTracks { .. } => "get_playlist_tracks",
        ServerCommand::AddToPlaylist { .. } => "add_to_playlist",
        ServerCommand::RemoveFromPlaylist { .. } => "remove_from_playlist",
        ServerCommand::ReplacePlaylist { .. } => "replace_playlist",
        ServerCommand::CreatePlaylist { .. } => "create_playlist",
    }
}

async fn handle_command(spotify: &mut SpotifyClient, cmd: ServerCommand) -> CommandResponse {
    log::info!("Handling command: {}", command_name(&cmd));
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
        ServerCommand::GetPlaybackState { id } => {
            match spotify.get_playback_state().await {
                Ok(state) => CommandResponse {
                    id,
                    result: Some(serde_json::to_value(&state).unwrap_or_default()),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::GetPlaylistTracks { id, playlist_id, offset, limit } => {
            match spotify.get_playlist_tracks(&playlist_id, offset.unwrap_or(0), limit.unwrap_or(100)).await {
                Ok(tracks) => CommandResponse {
                    id,
                    result: Some(serde_json::to_value(&tracks).unwrap_or_default()),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::AddToPlaylist { id, playlist_id, uris, position } => {
            match spotify.add_to_playlist(&playlist_id, uris, position).await {
                Ok(snapshot_id) => CommandResponse {
                    id,
                    result: Some(serde_json::json!({"snapshot_id": snapshot_id})),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::RemoveFromPlaylist { id, playlist_id, uris } => {
            match spotify.remove_from_playlist(&playlist_id, uris).await {
                Ok(snapshot_id) => CommandResponse {
                    id,
                    result: Some(serde_json::json!({"snapshot_id": snapshot_id})),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::ReplacePlaylist { id, playlist_id, uris } => {
            match spotify.replace_playlist_tracks(&playlist_id, uris).await {
                Ok(snapshot_id) => CommandResponse {
                    id,
                    result: Some(serde_json::json!({"snapshot_id": snapshot_id})),
                    error: None,
                },
                Err(e) => error_response(id, "spotify_error", &e.to_string()),
            }
        }
        ServerCommand::CreatePlaylist { id, name, description, public } => {
            match spotify.create_playlist(&name, description.as_deref(), public.unwrap_or(false)).await {
                Ok(playlist) => CommandResponse {
                    id,
                    result: Some(serde_json::to_value(&playlist).unwrap_or_default()),
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

async fn authenticate_spotify<P: RelayPlatform>(
    platform: &P,
    config: &RelayConfig,
) -> Result<oauth::OAuthTokens, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(refresh_token) = platform.get_refresh_token() {
        log::info!("Found existing refresh token, attempting refresh");
        match oauth::refresh_access_token(&config.spotify_client_id, &refresh_token).await {
            Ok(tokens) => {
                log::info!("Token refresh successful");
                return Ok(tokens);
            }
            Err(e) => {
                // Return the error instead of falling back to browser OAuth.
                // The retry loop will handle transient failures, and if the
                // refresh token is truly revoked the user will see the error.
                return Err(format!("Token refresh failed: {}", e).into());
            }
        }
    }

    // No stored refresh token -- first-time setup, requires browser interaction
    let tokens = oauth::start_oauth_flow(
        &config.spotify_client_id,
        &config.redirect_uri,
        |url| platform.present_auth_url(url),
    )
    .await
    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    Ok(tokens)
}

fn persist_if_refreshed<P: RelayPlatform>(platform: &P, spotify: &mut SpotifyClient) {
    if let Some(token) = spotify.take_refreshed_token() {
        platform.persist_refresh_token(&token);
    }
}
