mod config;
mod platform;

use std::sync::Arc;

use config::HeadlessConfig;
use platform::HeadlessPlatform;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_path = HeadlessConfig::config_path();

    let config = if config_path.exists() {
        match HeadlessConfig::load(&config_path) {
            Ok(c) => c,
            Err(e) => {
                log::error!("Failed to load config from {}: {}", config_path.display(), e);
                std::process::exit(1);
            }
        }
    } else {
        match HeadlessConfig::interactive_setup() {
            Ok(c) => c,
            Err(e) => {
                log::error!("Setup failed: {}", e);
                std::process::exit(1);
            }
        }
    };

    if !config.is_configured() {
        log::error!(
            "Incomplete configuration. Edit {} or delete it to re-run setup.",
            config_path.display()
        );
        std::process::exit(1);
    }

    let relay_config = config.to_relay_config();
    let platform = Arc::new(HeadlessPlatform::new(config_path));

    log::info!("Starting relay (server: {})", relay_config.server_url);
    let shutdown_tx = relay_core::start_relay(platform, relay_config);

    // Wait for SIGINT / SIGTERM
    let tx = shutdown_tx.clone();
    ctrlc::set_handler(move || {
        log::info!("Shutdown signal received");
        let _ = tx.send(true);
    })
    .expect("failed to set signal handler");

    // Block until shutdown is triggered
    let mut rx = shutdown_tx.subscribe();
    loop {
        if *rx.borrow() {
            break;
        }
        if rx.changed().await.is_err() {
            break;
        }
    }

    // Give tasks a moment to clean up
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    log::info!("Relay stopped");
}
