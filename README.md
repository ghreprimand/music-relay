# Music Relay

Desktop application that proxies Spotify Web API requests from your local machine. Runs in the system tray, authenticates with Spotify via OAuth PKCE, and relays commands from a remote server over WebSocket -- so all Spotify API calls originate from the DJ's own IP.

## Download

Grab the latest release from [GitHub Releases](https://github.com/ghreprimand/music-relay/releases).

| Platform | Format |
|----------|--------|
| Linux | `.AppImage`, `.deb` |
| Windows | `.msi` |

## First-Run Setup

### 1. Create a Spotify App

1. Open [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard)
2. Click **Create App**
3. Set the **Redirect URI** to:
   ```
   http://127.0.0.1:18974/callback
   ```
4. Save the app, then copy the **Client ID** (32-character hex string)

### 2. Configure Music Relay

Launch the app. It opens a settings window on first run.

| Field | Description |
|-------|-------------|
| Client ID | The Spotify Client ID from step 1 |
| WebSocket URL | Server endpoint (`wss://...`) provided by your server admin |
| Poll Interval | How often to report now-playing state, in seconds (default: 5) |

Click **Save**. The app minimizes to the system tray and begins connecting.

## Autostart

Enable **Launch at startup** in Settings to start Music Relay automatically when you log in.

- **Linux:** creates a `.desktop` entry in `~/.config/autostart/`
- **Windows:** adds a registry entry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`

## Configuration

Settings are stored as JSON via the Tauri store plugin:

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/com.musicrelay.app/config.json` |
| Windows | `%APPDATA%\com.musicrelay.app\config.json` |

## Building from Source

### Prerequisites

- Node.js 22+
- Rust stable (1.77+)
- **Linux:** `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`
- **Windows:** WebView2 (included in Windows 10/11)

### Commands

```sh
npm install
cargo tauri dev      # development mode
cargo tauri build    # production build
```

## Protocol

See [PROTOCOL.md](PROTOCOL.md) for the WebSocket message format.

## License

[MIT](LICENSE)
