use std::sync::atomic::{AtomicU32, Ordering};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const MAX_BACKOFF_SECS: u64 = 30;

#[derive(Debug, Error)]
pub enum CentrifugoError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Connect rejected: {0}")]
    ConnectRejected(String),
    #[error("Subscribe rejected: {0}")]
    SubscribeRejected(String),
    #[error("Connection closed")]
    Closed,
}

// -- Wire protocol types --

#[derive(Debug, Serialize)]
struct Command {
    id: u32,
    #[serde(flatten)]
    method: CommandMethod,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum CommandMethod {
    Connect(ConnectRequest),
    Subscribe(SubscribeRequest),
    Publish(PublishRequest),
}

#[derive(Debug, Serialize)]
struct ConnectRequest {
    token: String,
    name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct SubscribeRequest {
    channel: String,
}

#[derive(Debug, Serialize)]
struct PublishRequest {
    channel: String,
    data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Reply {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    connect: Option<ConnectResult>,
    #[serde(default)]
    subscribe: Option<serde_json::Value>,
    #[serde(default)]
    publish: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<ProtoError>,
    #[serde(default)]
    push: Option<Push>,
}

#[derive(Debug, Deserialize)]
struct ConnectResult {
    #[serde(default)]
    client: String,
    #[serde(default)]
    ping: Option<u32>,
    #[serde(default)]
    pong: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProtoError {
    #[serde(default)]
    code: u32,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Push {
    #[serde(default)]
    channel: Option<String>,
    #[serde(default, rename = "pub")]
    publication: Option<Publication>,
}

#[derive(Debug, Deserialize)]
struct Publication {
    data: serde_json::Value,
}

// -- Application-level command/response types --

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "command")]
pub enum ServerCommand {
    #[serde(rename = "get_now_playing")]
    GetNowPlaying { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "get_queue")]
    GetQueue { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "search")]
    Search { id: String, query: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "add_to_queue")]
    AddToQueue { id: String, track_uri: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "get_playback_state")]
    GetPlaybackState { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "get_playlist_tracks")]
    GetPlaylistTracks {
        id: String,
        playlist_id: String,
        offset: Option<u32>,
        limit: Option<u32>,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "add_to_playlist")]
    AddToPlaylist {
        id: String,
        playlist_id: String,
        uris: Vec<String>,
        position: Option<u32>,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "remove_from_playlist")]
    RemoveFromPlaylist {
        id: String,
        playlist_id: String,
        uris: Vec<String>,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "replace_playlist")]
    ReplacePlaylist {
        id: String,
        playlist_id: String,
        uris: Vec<String>,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "create_playlist")]
    CreatePlaylist {
        id: String,
        name: String,
        description: Option<String>,
        public: Option<bool>,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "get_artists")]
    GetArtists {
        id: String,
        artist_ids: Vec<String>,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "get_playlist_details")]
    GetPlaylistDetails {
        id: String,
        playlist_id: String,
        #[serde(default)]
        nonce: Option<String>,
    },
    #[serde(rename = "get_current_user")]
    GetCurrentUser { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "pause")]
    Pause { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "resume")]
    Resume { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "skip_next")]
    SkipNext { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "skip_previous")]
    SkipPrevious { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "set_volume")]
    SetVolume { id: String, volume_percent: u32, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "fade_skip")]
    FadeSkip { id: String, #[serde(default)] nonce: Option<String> },
    #[serde(rename = "fade_pause")]
    FadePause { id: String, #[serde(default)] nonce: Option<String> },
}

impl ServerCommand {
    /// Returns the command id.
    pub fn id(&self) -> &str {
        match self {
            ServerCommand::GetNowPlaying { id, .. }
            | ServerCommand::GetQueue { id, .. }
            | ServerCommand::Search { id, .. }
            | ServerCommand::AddToQueue { id, .. }
            | ServerCommand::GetPlaybackState { id, .. }
            | ServerCommand::GetPlaylistTracks { id, .. }
            | ServerCommand::AddToPlaylist { id, .. }
            | ServerCommand::RemoveFromPlaylist { id, .. }
            | ServerCommand::ReplacePlaylist { id, .. }
            | ServerCommand::CreatePlaylist { id, .. }
            | ServerCommand::GetArtists { id, .. }
            | ServerCommand::GetPlaylistDetails { id, .. }
            | ServerCommand::GetCurrentUser { id, .. }
            | ServerCommand::Pause { id, .. }
            | ServerCommand::Resume { id, .. }
            | ServerCommand::SkipNext { id, .. }
            | ServerCommand::SkipPrevious { id, .. }
            | ServerCommand::SetVolume { id, .. }
            | ServerCommand::FadeSkip { id, .. }
            | ServerCommand::FadePause { id, .. } => id,
        }
    }

    /// Returns the nonce if present.
    pub fn nonce(&self) -> Option<&str> {
        match self {
            ServerCommand::GetNowPlaying { nonce, .. }
            | ServerCommand::GetQueue { nonce, .. }
            | ServerCommand::Search { nonce, .. }
            | ServerCommand::AddToQueue { nonce, .. }
            | ServerCommand::GetPlaybackState { nonce, .. }
            | ServerCommand::GetPlaylistTracks { nonce, .. }
            | ServerCommand::AddToPlaylist { nonce, .. }
            | ServerCommand::RemoveFromPlaylist { nonce, .. }
            | ServerCommand::ReplacePlaylist { nonce, .. }
            | ServerCommand::CreatePlaylist { nonce, .. }
            | ServerCommand::GetArtists { nonce, .. }
            | ServerCommand::GetPlaylistDetails { nonce, .. }
            | ServerCommand::GetCurrentUser { nonce, .. }
            | ServerCommand::Pause { nonce, .. }
            | ServerCommand::Resume { nonce, .. }
            | ServerCommand::SkipNext { nonce, .. }
            | ServerCommand::SkipPrevious { nonce, .. }
            | ServerCommand::SetVolume { nonce, .. }
            | ServerCommand::FadeSkip { nonce, .. }
            | ServerCommand::FadePause { nonce, .. } => nonce.as_deref(),
        }
    }

    /// Returns true for commands that mutate Spotify state (playback, queue, playlists).
    pub fn is_mutating(&self) -> bool {
        matches!(
            self,
            ServerCommand::AddToQueue { .. }
                | ServerCommand::AddToPlaylist { .. }
                | ServerCommand::RemoveFromPlaylist { .. }
                | ServerCommand::ReplacePlaylist { .. }
                | ServerCommand::CreatePlaylist { .. }
                | ServerCommand::Pause { .. }
                | ServerCommand::Resume { .. }
                | ServerCommand::SkipNext { .. }
                | ServerCommand::SkipPrevious { .. }
                | ServerCommand::SetVolume { .. }
                | ServerCommand::FadeSkip { .. }
                | ServerCommand::FadePause { .. }
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

// -- Client --

pub struct CentrifugoClient {
    url: String,
    token: String,
    channel: String,
    next_id: AtomicU32,
}

impl CentrifugoClient {
    pub fn new(url: String, token: String, channel: String) -> Self {
        Self {
            url,
            token,
            channel,
            next_id: AtomicU32::new(1),
        }
    }

    fn next_id(&self) -> u32 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Connect and run the message loop. Incoming server commands are sent
    /// through `command_tx`. Responses to publish back are read from `response_rx`.
    /// Returns on connection loss (caller should reconnect).
    pub async fn connect_and_run(
        &self,
        command_tx: mpsc::Sender<ServerCommand>,
        mut response_rx: mpsc::Receiver<CommandResponse>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), CentrifugoError> {
        log::info!("Connecting to Centrifugo at {}", self.url);

        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.url)
            .await
            .map_err(|e| CentrifugoError::ConnectionFailed(e.to_string()))?;

        let (mut write, mut read) = ws_stream.split();

        // Send connect command
        let connect_cmd = Command {
            id: self.next_id(),
            method: CommandMethod::Connect(ConnectRequest {
                token: self.token.clone(),
                name: "music-relay".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }),
        };
        let connect_json = serde_json::to_string(&connect_cmd)?;
        write.send(Message::Text(connect_json)).await?;

        // Read connect reply
        let connect_reply = read_reply(&mut read).await?;
        if let Some(err) = connect_reply.error {
            return Err(CentrifugoError::ConnectRejected(format!(
                "code {}: {}",
                err.code, err.message
            )));
        }

        let ping_interval = connect_reply
            .connect
            .as_ref()
            .and_then(|c| c.ping)
            .unwrap_or(25);
        let send_pong = connect_reply
            .connect
            .as_ref()
            .and_then(|c| c.pong)
            .unwrap_or(false);

        log::info!(
            "Connected to Centrifugo (client: {})",
            connect_reply
                .connect
                .as_ref()
                .map(|c| c.client.as_str())
                .unwrap_or("unknown")
        );

        // Subscribe to channel
        let sub_cmd = Command {
            id: self.next_id(),
            method: CommandMethod::Subscribe(SubscribeRequest {
                channel: self.channel.clone(),
            }),
        };
        let sub_json = serde_json::to_string(&sub_cmd)?;
        write.send(Message::Text(sub_json)).await?;

        let sub_reply = read_reply(&mut read).await?;
        if let Some(err) = sub_reply.error {
            return Err(CentrifugoError::SubscribeRejected(format!(
                "code {}: {}",
                err.code, err.message
            )));
        }

        log::info!("Subscribed to channel: {}", self.channel);

        // Message loop
        let ping_timeout = std::time::Duration::from_secs((ping_interval as u64) * 2);
        let mut ping_deadline = tokio::time::Instant::now() + ping_timeout;

        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            ping_deadline = tokio::time::Instant::now() + ping_timeout;

                            // Handle ping (empty JSON object)
                            let trimmed = text.trim();
                            if trimmed == "{}" {
                                if send_pong {
                                    write.send(Message::Text("{}".to_string())).await?;
                                }
                                continue;
                            }

                            // Parse as reply
                            if let Ok(reply) = serde_json::from_str::<Reply>(trimmed) {
                                if let Some(push) = reply.push {
                                    if let Some(pub_data) = push.publication {
                                        // Try to parse as a server command
                                        match serde_json::from_value::<ServerCommand>(pub_data.data.clone()) {
                                            Ok(cmd) => {
                                                if command_tx.send(cmd).await.is_err() {
                                                    log::warn!("Command channel closed");
                                                    return Ok(());
                                                }
                                            }
                                            Err(e) => {
                                                log::warn!("Failed to parse server command: {} - data: {}", e, pub_data.data);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            log::info!("WebSocket closed by server");
                            return Err(CentrifugoError::Closed);
                        }
                        Some(Ok(Message::Ping(data))) => {
                            write.send(Message::Pong(data)).await?;
                            ping_deadline = tokio::time::Instant::now() + ping_timeout;
                        }
                        Some(Err(e)) => {
                            return Err(CentrifugoError::WebSocket(e));
                        }
                        None => {
                            return Err(CentrifugoError::Closed);
                        }
                        _ => {}
                    }
                }
                resp = response_rx.recv() => {
                    if let Some(resp) = resp {
                        let publish_cmd = Command {
                            id: self.next_id(),
                            method: CommandMethod::Publish(PublishRequest {
                                channel: self.channel.clone(),
                                data: serde_json::to_value(&resp)?,
                            }),
                        };
                        let json = serde_json::to_string(&publish_cmd)?;
                        write.send(Message::Text(json)).await?;
                    }
                }
                _ = tokio::time::sleep_until(ping_deadline) => {
                    log::warn!("Ping timeout, disconnecting");
                    return Err(CentrifugoError::Closed);
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        log::info!("Centrifugo shutdown requested");
                        let _ = write.close().await;
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn read_reply<S>(read: &mut S) -> Result<Reply, CentrifugoError>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => {
                let reply: Reply = serde_json::from_str(&text)?;
                return Ok(reply);
            }
            Some(Ok(Message::Ping(_))) => continue,
            Some(Ok(Message::Close(_))) | None => {
                return Err(CentrifugoError::Closed);
            }
            Some(Err(e)) => return Err(CentrifugoError::WebSocket(e)),
            _ => continue,
        }
    }
}

/// Calculate exponential backoff delay for reconnection attempts.
pub fn backoff_delay(attempt: u32) -> std::time::Duration {
    let secs = (2u64.pow(attempt.min(5))).min(MAX_BACKOFF_SECS);
    std::time::Duration::from_secs(secs)
}
