mod centrifugo;
mod config;
mod oauth;
mod relay;
mod spotify;
mod state;

use config::AppConfig;
use state::{AppState, ConnectionStatus};
use std::sync::Mutex;
use tauri::{
    image::Image,
    Listener, Manager,
    menu::{MenuBuilder, MenuItem, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_store::StoreExt;

struct TrayState {
    status_item: MenuItem<tauri::Wry>,
}

#[tauri::command]
fn get_status(state: tauri::State<'_, Mutex<AppState>>) -> Result<serde_json::Value, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "spotify": state.spotify_status,
        "websocket": state.websocket_status,
        "now_playing": state.now_playing,
        "last_error": state.last_error,
    }))
}

#[tauri::command]
fn get_config_status(state: tauri::State<'_, Mutex<AppState>>) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.config.is_configured())
}

#[tauri::command]
fn reload_config(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<bool, String> {
    let store = app.store("config.json").map_err(|e| e.to_string())?;
    let config = AppConfig::from_store(&store);
    let configured = config.is_configured();

    // Stop existing relay if running
    {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        if let Some(tx) = state.relay_shutdown.take() {
            let _ = tx.send(true);
        }
        state.config = config.clone();
    }

    // Start relay if configured
    if configured {
        let shutdown_tx = relay::start_relay(app, config);
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.relay_shutdown = Some(shutdown_tx);
    }

    Ok(configured)
}

#[tauri::command]
fn restart_relay(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let config = {
        let mut state = state.lock().map_err(|e| e.to_string())?;
        // Stop existing relay
        if let Some(tx) = state.relay_shutdown.take() {
            let _ = tx.send(true);
        }
        state.last_error = None;
        state.spotify_status = ConnectionStatus::Disconnected;
        state.websocket_status = ConnectionStatus::Disconnected;
        state.config.clone()
    };

    if config.is_configured() {
        let shutdown_tx = relay::start_relay(app, config);
        let mut state = state.lock().map_err(|e| e.to_string())?;
        state.relay_shutdown = Some(shutdown_tx);
    }

    Ok(())
}

#[tauri::command]
fn get_close_to_tray(state: tauri::State<'_, Mutex<AppState>>) -> Result<bool, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(state.config.close_to_tray)
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
    let status_item = MenuItemBuilder::with_id("status", "Status: disconnected")
        .enabled(false)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&status_item)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/icon.png")).expect("failed to decode tray icon"))
        .tooltip("Music Relay")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click { .. } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    app.manage(TrayState { status_item });

    Ok(())
}

fn setup_tray_updater(app: &tauri::App) {
    let handle = app.handle().clone();
    app.listen("status-changed", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let tray = handle.try_state::<TrayState>();
            if let Some(tray) = tray {
                let spotify = payload["spotify"].as_str().unwrap_or("Disconnected");
                let ws = payload["websocket"].as_str().unwrap_or("Disconnected");

                let status_text = match (spotify, ws) {
                    ("Connected", "Connected") => "Status: connected".to_string(),
                    ("Connected", _) => "Status: Spotify OK, server disconnected".to_string(),
                    ("Connecting", _) => "Status: connecting...".to_string(),
                    _ => "Status: disconnected".to_string(),
                };

                let _ = tray.status_item.set_text(&status_text);

                // Update tooltip with now-playing info
                let tooltip = if let Some(np) = payload["now_playing"].as_object() {
                    let artist = np.get("artist_name").and_then(|v| v.as_str()).unwrap_or("");
                    let track = np.get("track_name").and_then(|v| v.as_str()).unwrap_or("");
                    let playing = np.get("is_playing").and_then(|v| v.as_bool()).unwrap_or(false);
                    if playing && !track.is_empty() {
                        format!("Music Relay - {} - {}", artist, track)
                    } else {
                        "Music Relay - Idle".to_string()
                    }
                } else {
                    "Music Relay".to_string()
                };

                if let Some(tray_icon) = handle.tray_by_id("main") {
                    let _ = tray_icon.set_tooltip(Some(&tooltip));
                }
            }
        }
    });
}

fn setup_close_to_tray(app: &tauri::App) {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => return,
    };

    let app_handle = app.handle().clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            let close_to_tray = app_handle
                .try_state::<Mutex<AppState>>()
                .and_then(|state| state.lock().ok().map(|s| s.config.close_to_tray))
                .unwrap_or(true);

            if close_to_tray {
                api.prevent_close();
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let store = app.store("config.json")?;
            let config = AppConfig::from_store(&store);
            let configured = config.is_configured();

            let state = AppState {
                config: config.clone(),
                ..Default::default()
            };
            app.manage(Mutex::new(state));

            setup_tray(app)?;
            setup_tray_updater(app);
            setup_close_to_tray(app);

            // Auto-start relay if already configured
            if configured {
                let shutdown_tx = relay::start_relay(app.handle().clone(), config);
                if let Some(state) = app.try_state::<Mutex<AppState>>() {
                    if let Ok(mut state) = state.lock() {
                        state.relay_shutdown = Some(shutdown_tx);
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_config_status,
            get_close_to_tray,
            reload_config,
            restart_relay,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run application");
}
