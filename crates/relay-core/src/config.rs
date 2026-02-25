#[derive(Debug, Clone)]
pub struct RelayConfig {
    pub server_url: String,
    pub api_key: String,
    pub spotify_client_id: String,
    pub redirect_uri: String,
    pub poll_interval_secs: u64,
}

impl RelayConfig {
    pub fn is_configured(&self) -> bool {
        !self.server_url.is_empty()
            && !self.api_key.is_empty()
            && !self.spotify_client_id.is_empty()
    }
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            api_key: String::new(),
            spotify_client_id: String::new(),
            redirect_uri: "http://127.0.0.1:18974/callback".to_string(),
            poll_interval_secs: 5,
        }
    }
}
