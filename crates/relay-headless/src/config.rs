use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use relay_core::config::RelayConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadlessConfig {
    pub server_url: String,
    pub api_key: String,
    pub spotify_client_id: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

fn default_poll_interval() -> u64 {
    5
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            api_key: String::new(),
            spotify_client_id: String::new(),
            poll_interval_secs: 5,
            refresh_token: None,
        }
    }
}

impl HeadlessConfig {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("music-relay")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load(path: &PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        let config: HeadlessConfig = serde_json::from_str(&data)?;
        Ok(config)
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    pub fn to_relay_config(&self) -> RelayConfig {
        RelayConfig {
            server_url: self.server_url.clone(),
            api_key: self.api_key.clone(),
            spotify_client_id: self.spotify_client_id.clone(),
            redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
            poll_interval_secs: self.poll_interval_secs,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty()
            && !self.api_key.is_empty()
            && !self.spotify_client_id.is_empty()
    }

    /// Run interactive first-time setup, prompting the user for config values.
    pub fn interactive_setup() -> Result<Self, Box<dyn std::error::Error>> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();

        println!("Music Relay -- First-time setup\n");

        let server_url = prompt(&mut reader, "Server URL (e.g., https://relay.example.com): ")?;
        let api_key = prompt(&mut reader, "API Key: ")?;
        let spotify_client_id = prompt(&mut reader, "Spotify Client ID: ")?;
        let poll_str = prompt(&mut reader, "Poll interval in seconds [5]: ")?;
        let poll_interval_secs = if poll_str.is_empty() {
            5
        } else {
            poll_str.parse::<u64>().unwrap_or(5).clamp(1, 60)
        };

        let config = HeadlessConfig {
            server_url,
            api_key,
            spotify_client_id,
            poll_interval_secs,
            refresh_token: None,
        };

        let path = Self::config_path();
        config.save(&path)?;
        println!("\nConfig saved to {}", path.display());

        Ok(config)
    }
}

fn prompt(reader: &mut impl BufRead, message: &str) -> Result<String, Box<dyn std::error::Error>> {
    print!("{}", message);
    io::stdout().flush()?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim().to_string())
}
