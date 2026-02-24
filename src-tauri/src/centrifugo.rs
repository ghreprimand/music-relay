use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CentrifugoError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "command")]
pub enum ServerCommand {
    #[serde(rename = "get_now_playing")]
    GetNowPlaying { id: String },
    #[serde(rename = "get_queue")]
    GetQueue { id: String },
    #[serde(rename = "search")]
    Search { id: String, query: String },
    #[serde(rename = "add_to_queue")]
    AddToQueue { id: String, track_uri: String },
}

#[derive(Debug, Serialize)]
pub struct CommandResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

pub struct CentrifugoClient {
    _url: String,
}

impl CentrifugoClient {
    pub fn new(url: String) -> Self {
        Self { _url: url }
    }

    /// Connect to the WebSocket server and process commands in a loop.
    pub async fn connect_and_run(&self) -> Result<(), CentrifugoError> {
        todo!("implement WebSocket connection and command loop")
    }
}
