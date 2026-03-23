#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("Music Relay {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    music_relay_lib::run();
}
