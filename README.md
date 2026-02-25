# Music Relay

Desktop application that proxies Spotify Web API requests from the local machine. Runs as a system tray service, authenticates with Spotify via OAuth PKCE, and relays commands from a remote server over a Centrifugo WebSocket connection. All Spotify API calls originate from the user's IP.

Built with [Tauri v2](https://v2.tauri.app), React 19, and TypeScript.

## Download

Pre-built binaries are available on [GitHub Releases](https://github.com/ghreprimand/music-relay/releases).

| Platform | Format |
|----------|--------|
| Linux | `.AppImage`, `.deb` |
| Windows | `.msi` |

## Architecture

```
Remote Server
    |
    | Centrifugo WebSocket (JSON protocol)
    |
Music Relay (this app)
    |
    | HTTPS (OAuth PKCE + REST)
    |
Spotify Web API
```

The relay subscribes to a Centrifugo channel and listens for commands. When a command arrives (e.g. search, add to queue, manage playlists), the relay executes it against the Spotify API and publishes the result back to the same channel. The relay also proactively broadcasts now-playing state whenever the track changes.

### Backend Modules (Rust)

| Module | Responsibility |
|--------|---------------|
| `lib.rs` | App entry, tray setup, window management, Tauri command registration |
| `config.rs` | `AppConfig` struct, store-based persistence |
| `state.rs` | `AppState` with connection statuses, now-playing info, relay lifecycle |
| `oauth.rs` | Spotify PKCE flow: code verifier/challenge generation, localhost callback listener (port 18974), token exchange, token refresh |
| `spotify.rs` | Spotify Web API client (GET/POST/PUT/DELETE) with typed request/response structs, automatic token refresh, 401 retry, URI batching for playlist operations |
| `centrifugo.rs` | Centrifugo JSON protocol client: connect, subscribe, publish, ping/pong, reconnect with exponential backoff |
| `relay.rs` | Background orchestrator: authenticates Spotify, connects Centrifugo, runs poll loop, dispatches commands (playback, queue, search, playlists), emits status events to frontend |

### Frontend Components (TypeScript/React)

| Component | Responsibility |
|-----------|---------------|
| `App.tsx` | View router: redirects to Settings if unconfigured, otherwise shows Status |
| `Settings.tsx` | Three-card settings form: Spotify setup guide, server connection fields, preferences |
| `Status.tsx` | Live connection status display, now-playing info, error banner, reconnect button |

## Spotify Integration

### OAuth PKCE Flow

1. Generates a 128-character code verifier and SHA-256 code challenge
2. Opens the user's browser to `https://accounts.spotify.com/authorize` with PKCE parameters
3. Listens on `http://127.0.0.1:18974/callback` for the redirect (120s timeout)
4. Exchanges the authorization code for access + refresh tokens
5. Stores the refresh token in the local config store
6. On subsequent launches, refreshes silently without opening the browser

### OAuth Scopes

The relay requests these scopes during authorization:

- `user-read-currently-playing` -- now-playing polling and broadcast
- `user-read-playback-state` -- queue and full playback state
- `user-modify-playback-state` -- add to queue
- `playlist-read-private` -- read private playlists
- `playlist-read-collaborative` -- read collaborative playlists
- `playlist-modify-public` -- create/modify public playlists
- `playlist-modify-private` -- create/modify private playlists

**Upgrading from versions before 1.1.0:** The stored refresh token from earlier versions does not include playlist scopes. After upgrading, delete the stored `spotify_refresh_token` from the config store (or delete the config file) to trigger a fresh OAuth flow with the full scope set.

### API Endpoints Used

| Endpoint | Method | Scope Required |
|----------|--------|---------------|
| `/v1/me/player/currently-playing` | GET | `user-read-currently-playing` |
| `/v1/me/player/queue` | GET | `user-read-currently-playing`, `user-read-playback-state` |
| `/v1/search?q=...&type=track` | GET | (none beyond valid token) |
| `/v1/me/player/queue?uri=...` | POST | `user-modify-playback-state` |
| `/v1/me/player` | GET | `user-read-playback-state` |
| `/v1/me` | GET | (none beyond valid token) |
| `/v1/playlists/{id}/tracks` | GET | `playlist-read-private`, `playlist-read-collaborative` |
| `/v1/playlists/{id}/tracks` | POST | `playlist-modify-public`, `playlist-modify-private` |
| `/v1/playlists/{id}/tracks` | PUT | `playlist-modify-public`, `playlist-modify-private` |
| `/v1/playlists/{id}/tracks` | DELETE | `playlist-modify-public`, `playlist-modify-private` |
| `/v1/users/{id}/playlists` | POST | `playlist-modify-public`, `playlist-modify-private` |

Token refresh happens automatically 60 seconds before expiry. On a 401 response, the client forces a refresh and retries once.

### Response Types

The relay uses typed Rust structs for all Spotify responses. Key types:

- `Track` -- `id`, `name`, `uri`, `duration_ms`, `artists: Vec<Artist>`, `album: Album`
- `Artist` -- `id`, `name`
- `Album` -- `id`, `name`, `images: Vec<Image>`
- `Image` -- `url`, `height`, `width`
- `NowPlaying` -- `is_playing`, `progress_ms`, `item: Option<Track>`
- `QueueResponse` -- `currently_playing: Option<Track>`, `queue: Vec<Track>`
- `SearchResponse` -- `tracks: { items: Vec<Track>, total: u32 }`
- `PlaybackState` -- `is_playing`, `progress_ms`, `item: Option<Track>`, `context: Option<PlaybackContext>`, `shuffle_state`, `device: Option<Device>`
- `PlaybackContext` -- `type`, `uri`
- `Device` -- `id: Option<String>`, `name`, `is_active`
- `PlaylistTracksResponse` -- `items: Vec<PlaylistItem>`, `total: u32`
- `PlaylistItem` -- `track: Option<Track>`
- `CreatePlaylistResponse` -- `id`, `external_urls: ExternalUrls`
- `ExternalUrls` -- `spotify`
- `UserProfile` -- `id`

## WebSocket Protocol

See [PROTOCOL.md](PROTOCOL.md) for the full wire format, including:

- Command schemas (`get_now_playing`, `get_queue`, `search`, `add_to_queue`, `get_playback_state`, `get_playlist_tracks`, `add_to_playlist`, `remove_from_playlist`, `replace_playlist`, `create_playlist`)
- Response format (result/error with correlation IDs)
- Now-playing broadcast format (published on track change)
- Centrifugo publish wrapper
- Connection lifecycle and reconnection behavior

## Configuration

### Settings Fields

| Field | Store Key | Type | Required | Description |
|-------|-----------|------|----------|-------------|
| Client ID | `spotify_client_id` | string | Yes | 32-character hex Spotify app client ID |
| WebSocket URL | `websocket_url` | string | Yes | Centrifugo endpoint, e.g. `wss://example.com/connection/websocket` |
| Connection Token | `websocket_token` | string | No | JWT for Centrifugo authentication |
| Channel | `websocket_channel` | string | No | Centrifugo channel to subscribe to |
| Poll Interval | `poll_interval_secs` | number | No | Seconds between now-playing polls (default: 5, range: 1-60) |
| Minimize to tray | `close_to_tray` | boolean | No | Hide window on close instead of quitting (default: true) |
| Launch at startup | (OS-level) | boolean | No | Register with OS autostart (default: false) |

The app considers itself "configured" when both `spotify_client_id` and `websocket_url` are non-empty. If `websocket_token` or `websocket_channel` are empty, the relay runs in poll-only mode (Spotify polling without server connection).

Internal keys (not user-editable):
- `spotify_refresh_token` -- persisted Spotify OAuth refresh token

### Store Location

Settings are persisted as JSON via `tauri-plugin-store`:

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/com.musicrelay.app/config.json` |
| Windows | `%APPDATA%\com.musicrelay.app\config.json` |

## Relay Behavior

### Startup Sequence

1. Load config from store
2. If configured, spawn relay background task:
   a. Check for stored `spotify_refresh_token` and attempt silent refresh
   b. If no token or refresh fails, open browser for full OAuth flow
   c. Connect to Centrifugo (if token + channel configured)
   d. Begin polling Spotify at the configured interval
3. Emit `status-changed` events to frontend on each state transition

### Reconnection

- WebSocket disconnect or ping timeout triggers automatic reconnection
- Exponential backoff: 2s, 4s, 8s, 16s, 30s (capped at 30s)
- Backoff resets on successful connection
- Spotify auth errors surface in the UI with a Reconnect button

### Dual Mode

- **Full mode:** Spotify polling + Centrifugo command dispatch (when all server fields configured)
- **Poll-only mode:** Spotify polling without server connection (when token or channel empty)

### Events Emitted to Frontend

| Event | Payload | Description |
|-------|---------|-------------|
| `status-changed` | `{ spotify, websocket, now_playing, last_error }` | Emitted on any state change |

`spotify` and `websocket` are string enums: `"Disconnected"`, `"Connecting"`, `"Connected"`.

### Tauri Commands

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `get_status` | none | `{ spotify, websocket, now_playing, last_error }` | Current relay state |
| `get_config_status` | none | `boolean` | Whether config is complete |
| `get_close_to_tray` | none | `boolean` | Current close-to-tray preference |
| `reload_config` | none | `boolean` | Reload config from store, restart relay, return configured status |
| `restart_relay` | none | `()` | Stop and restart the relay (for manual reconnection) |

## System Tray

- Left-click: show window
- Menu items: Show, Status (read-only, updates dynamically), Quit
- Tooltip updates with current track: "Music Relay - Artist - Track"
- Close-to-tray: window hides on close instead of quitting (configurable)

## First-Run Setup

### 1. Create a Spotify App

1. Open [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard)
2. Create an app
3. Set the redirect URI to `http://127.0.0.1:18974/callback`
4. Copy the Client ID (32-character hex string)

### 2. Configure Music Relay

Launch the app. The settings window opens automatically on first run. Enter the Client ID, WebSocket URL, connection token, and channel. Click Save.

The app authenticates with Spotify (opens browser on first run), connects to the server, and minimizes to the system tray.

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

### CI

GitHub Actions workflow (`.github/workflows/build.yml`) builds on `ubuntu-22.04` and `windows-latest`. Draft releases are created automatically on tag push (`v*`).

## Dependencies

### Rust (src-tauri)

| Crate | Purpose |
|-------|---------|
| `tauri` | App framework (tray, windows, commands) |
| `tauri-plugin-shell` | Open URLs in browser |
| `tauri-plugin-store` | JSON config persistence |
| `tauri-plugin-autostart` | OS-level autostart registration |
| `reqwest` | HTTP client for Spotify API |
| `tokio-tungstenite` | WebSocket client for Centrifugo |
| `futures-util` | Stream utilities for WebSocket messages |
| `sha2` | PKCE code challenge (SHA-256) |
| `base64` | Base64url encoding for PKCE |
| `rand` | Random string generation |
| `open` | Open browser for OAuth |
| `serde` / `serde_json` | Serialization |
| `tokio` | Async runtime |
| `thiserror` | Error types |
| `log` / `env_logger` | Logging |

### Frontend (npm)

| Package | Purpose |
|---------|---------|
| `react` / `react-dom` | UI framework |
| `@tauri-apps/api` | Tauri IPC (invoke, events) |
| `@tauri-apps/plugin-shell` | Open URLs from frontend |
| `@tauri-apps/plugin-store` | Read/write config from frontend |
| `@tauri-apps/plugin-autostart` | Toggle autostart from frontend |

## License

[MIT](LICENSE)
