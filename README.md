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
| **Client ID** | The Spotify Client ID from step 1 |
| **WebSocket URL** | Centrifugo endpoint (`wss://...`) provided by your server admin |
| **Connection Token** | JWT token for authenticating with the WebSocket server |
| **Channel** | Channel to subscribe to for receiving commands |
| **Poll Interval** | How often to report now-playing state, in seconds (default: 5) |

Click **Save**. The app authenticates with Spotify (opens your browser on first run) and connects to the server. Once connected it minimizes to the system tray.

## Preferences

| Setting | Default | Description |
|---------|---------|-------------|
| Launch at startup | Off | Start Music Relay when you log in |
| Minimize to tray on close | On | Hide the window instead of quitting when you close it |

## How It Works

1. Authenticates with Spotify using OAuth PKCE (localhost redirect on port 18974)
2. Polls Spotify for the current track at the configured interval
3. Connects to the remote server via Centrifugo WebSocket
4. Receives commands (now-playing, queue, search, add to queue) and executes them against the Spotify API
5. Publishes results back through the WebSocket channel

The refresh token is stored locally so you only need to authorize once. The app reconnects automatically on connection loss with exponential backoff.

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
