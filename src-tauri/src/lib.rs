mod centrifugo;
mod config;
mod oauth;
mod spotify;
mod state;

use config::AppConfig;
use state::AppState;
use std::sync::Mutex;
use tauri::{
    image::Image,
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_store::StoreExt;

#[tauri::command]
fn get_status(state: tauri::State<'_, Mutex<AppState>>) -> Result<serde_json::Value, String> {
    let state = state.lock().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "spotify": state.spotify_status,
        "websocket": state.websocket_status,
        "now_playing": state.now_playing,
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
    let mut state = state.lock().map_err(|e| e.to_string())?;
    state.config = config;
    Ok(configured)
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItemBuilder::with_id("show", "Show").build(app)?;
    let status = MenuItemBuilder::with_id("status", "Status: disconnected")
        .enabled(false)
        .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&status)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(Image::from_bytes(include_bytes!("../icons/icon.png")).expect("failed to decode tray icon"))
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

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let store = app.store("config.json")?;
            let config = AppConfig::from_store(&store);
            let state = AppState {
                config,
                ..Default::default()
            };
            app.manage(Mutex::new(state));

            setup_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_config_status,
            reload_config,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run application");
}
