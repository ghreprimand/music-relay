# Music Relay

Desktop and headless application that proxies Spotify Web API requests from the local machine. The desktop version runs as a system tray service; the headless version runs as a CLI daemon (targeting Raspberry Pi / ARM64). Both authenticate with Spotify via OAuth PKCE and relay commands from a remote server over a Centrifugo WebSocket connection. All Spotify API calls originate from the user's IP.

Built with [Tauri v2](https://v2.tauri.app), React 19, and TypeScript (desktop), or pure Rust (headless).

## Download

Pre-built binaries are available on [GitHub Releases](https://github.com/ghreprimand/music-relay/releases).

### Desktop

| Platform | Format |
|----------|--------|
| Linux | `.AppImage`, `.deb` |
| Windows | `.msi` |

### Headless

| Architecture | Binary |
|--------------|--------|
| x86_64 Linux | `relay-headless-x86_64-unknown-linux-gnu` |
| ARM64 Linux | `relay-headless-aarch64-unknown-linux-gnu` |

## Architecture

```
Remote Server
    |
    | 1. GET /api/connector/token (Bearer API key)
    | 2. Centrifugo WebSocket (JWT from step 1)
    |
Music Relay (desktop or headless)
    |
    | HTTPS (OAuth PKCE + REST)
    |
Spotify Web API
```

The relay fetches a short-lived Centrifugo JWT, channel name, and WebSocket URL from the server's token endpoint, connects to the Centrifugo WebSocket, and subscribes to its relay channel. When a command arrives (e.g. search, add to queue, manage playlists), the relay executes it against the Spotify API and publishes the result back to the same channel. The relay also proactively broadcasts now-playing state whenever the track changes.

### Project Structure (Cargo Workspace)

```
music-relay/
  Cargo.toml                    # workspace root
  crates/
    relay-core/                 # shared library (no Tauri dependency)
    relay-headless/             # CLI binary for headless/Pi deployment
  src-tauri/                    # Tauri desktop application
  src/                          # React frontend (desktop only)
```

### relay-core (shared library)

| Module | Responsibility |
|--------|---------------|
| `config.rs` | `RelayConfig` struct: server URL, API key, Spotify client ID, poll interval |
| `token.rs` | Fetch Centrifugo JWT, channel, and WebSocket URL from server token endpoint; decode JWT `exp` claim for proactive refresh |
| `relay.rs` | `RelayPlatform` trait for platform abstraction; background orchestrator: authenticates Spotify, fetches Centrifugo token, connects WebSocket, runs poll loop, dispatches commands, retries with backoff, proactive token refresh before expiry |
| `oauth.rs` | Spotify PKCE flow: code verifier/challenge generation, localhost callback listener (port 18974), token exchange, token refresh. Platform-agnostic via `present_url` callback |
| `spotify.rs` | Spotify Web API client (GET/POST/PUT/DELETE) with typed request/response structs, automatic token refresh, 401 retry, 15s request timeout, URI batching for playlist operations |
| `centrifugo.rs` | Centrifugo JSON protocol client: connect, subscribe, publish, ping/pong, reconnect with exponential backoff |
| `state.rs` | `AppState` with connection statuses and now-playing info |

### src-tauri (desktop)

| Module | Responsibility |
|--------|---------------|
| `lib.rs` | App entry, tray setup, window management, Tauri command registration |
| `config.rs` | `TauriAppConfig` wrapping `RelayConfig` plus `close_to_tray`; loads from Tauri store |
| `platform.rs` | `TauriPlatform` implementing `RelayPlatform`: store-based token persistence, event emission, desktop notifications, browser-based OAuth |

### relay-headless (CLI)

| Module | Responsibility |
|--------|---------------|
| `main.rs` | Entry point, tokio runtime, signal handling (SIGINT/SIGTERM) |
| `config.rs` | `HeadlessConfig`: JSON file persistence at `~/.config/music-relay/config.json`, interactive first-run setup |
| `platform.rs` | `HeadlessPlatform` implementing `RelayPlatform`: file-based token persistence, log-based status output, prints OAuth URL to stdout |

### Frontend Components (TypeScript/React, desktop only)

| Component | Responsibility |
|-----------|---------------|
| `App.tsx` | View router: redirects to Settings if unconfigured, otherwise shows Status |
| `Settings.tsx` | Three-card settings form: Spotify setup guide, server connection fields (URL + API key), preferences. Cancel button when returning from status view. Only restarts relay if config actually changed |
| `Status.tsx` | Live connection status display, now-playing info, error banner, reconnect button |

## Spotify Integration

### OAuth PKCE Flow

1. Generates a 128-character code verifier and SHA-256 code challenge
2. Presents the authorization URL (desktop: opens browser; headless: prints to stdout)
3. Listens on `http://127.0.0.1:18974/callback` for the redirect (120s timeout)
4. Exchanges the authorization code for access + refresh tokens
5. Stores the refresh token via the platform (desktop: Tauri store; headless: JSON config file)
6. On subsequent launches, refreshes silently without user interaction

### OAuth Scopes

The relay requests these scopes during authorization:

- `user-read-currently-playing` -- now-playing polling and broadcast
- `user-read-playback-state` -- queue and full playback state
- `user-modify-playback-state` -- add to queue
- `playlist-read-private` -- read private playlists
- `playlist-read-collaborative` -- read collaborative playlists
- `playlist-modify-public` -- create/modify public playlists
- `playlist-modify-private` -- create/modify private playlists

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

- Token acquisition and channel derivation
- Command schemas (`get_now_playing`, `get_queue`, `search`, `add_to_queue`, `get_playback_state`, `get_playlist_tracks`, `add_to_playlist`, `remove_from_playlist`, `replace_playlist`, `create_playlist`)
- Response format (result/error with correlation IDs)
- Now-playing broadcast format (published on track change)
- Centrifugo publish wrapper
- Connection lifecycle, reconnection behavior, and proactive token refresh

## Configuration

### Desktop (Tauri Store)

| Field | Store Key | Type | Required | Description |
|-------|-----------|------|----------|-------------|
| Client ID | `spotify_client_id` | string | Yes | 32-character hex Spotify app client ID |
| Server URL | `server_url` | string | Yes | Base URL of the server (e.g. `https://sq.example.com`) |
| API Key | `api_key` | string | Yes | API key for authenticating with the server's token endpoint |
| Poll Interval | `poll_interval_secs` | number | No | Seconds between now-playing polls (default: 5, range: 1-60) |
| Minimize to tray | `close_to_tray` | boolean | No | Hide window on close instead of quitting (default: true) |
| Launch at startup | (OS-level) | boolean | No | Register with OS autostart (default: false) |

The app considers itself "configured" when `spotify_client_id`, `server_url`, and `api_key` are all non-empty. If `server_url` or `api_key` are empty, the relay runs in poll-only mode (Spotify polling without server connection).

Internal keys (not user-editable):
- `spotify_refresh_token` -- persisted Spotify OAuth refresh token

Settings are persisted as JSON via `tauri-plugin-store`:

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/com.musicrelay.app/config.json` |
| Windows | `%APPDATA%\com.musicrelay.app\config.json` |

### Headless (JSON Config File)

Config file location: `~/.config/music-relay/config.json`

```json
{
  "server_url": "https://sq.example.com",
  "api_key": "your-api-key",
  "spotify_client_id": "your-32-char-hex-client-id",
  "poll_interval_secs": 5,
  "refresh_token": null
}
```

On first run, the headless binary prompts interactively for `server_url`, `api_key`, `spotify_client_id`, and `poll_interval_secs`. The `refresh_token` field is managed automatically.

## Server Token Flow

The relay no longer uses a static Centrifugo JWT. Instead, it dynamically fetches a short-lived token from the server on every connection and reconnection:

1. `GET {server_url}/api/connector/token` with `Authorization: Bearer {api_key}`
2. Server responds with `{ "token": "eyJ...", "channel": "...", "websocket_url": "wss://..." }` (JWT typically 24h TTL, plus the channel and WebSocket URL)
3. Relay connects to the returned WebSocket URL with the fresh token and subscribes to the returned channel

### Proactive Token Refresh

To avoid a brief disconnect when the token expires server-side, the relay decodes the JWT `exp` claim and schedules a proactive reconnect 1 hour before expiry (at approximately the 23-hour mark for a 24h token). When the timer fires, the relay cleanly disconnects, fetches a fresh token, and reconnects seamlessly. If the timer does not fire for any reason (missing `exp` claim, etc.), the server-side disconnect triggers the standard reconnect loop as a fallback.

## Relay Behavior

### Startup Sequence

1. Load config from store (desktop) or JSON file (headless)
2. If configured, spawn relay background task:
   a. Check for stored refresh token and attempt silent Spotify token refresh
   b. If no stored token (first run), present OAuth URL for authorization
   c. Fetch Centrifugo token, channel, and WebSocket URL from server (if server configured)
   d. Connect to Centrifugo and subscribe to the relay channel
   f. Begin polling Spotify at the configured interval
3. Desktop: emit `status-changed` events to frontend on each state transition
   Headless: log status transitions to stdout

### Reconnection

**WebSocket level:** disconnect, ping timeout, or proactive token refresh triggers automatic reconnection with exponential backoff (2s, 4s, 8s, 16s, 30s cap). A fresh Centrifugo token is fetched on every reconnect. Backoff resets on successful connection.

**Relay level:** if the relay task itself fails (e.g. Spotify auth error, network outage), it retries up to 5 times with the same exponential backoff. The retry counter resets if the relay had been running successfully before failing. If all retries are exhausted:
- A notification alerts the user (desktop: system notification; headless: log warning)
- The stored refresh token is cleared so the next restart triggers a fresh OAuth flow
- Desktop: the error is shown in the Status UI with a Reconnect button

### Dual Mode

- **Full mode:** Spotify polling + Centrifugo command dispatch (when server URL and API key are configured)
- **Poll-only mode:** Spotify polling without server connection (when server fields are empty)

### Events Emitted to Frontend (desktop only)

| Event | Payload | Description |
|-------|---------|-------------|
| `status-changed` | `{ spotify, websocket, now_playing, last_error }` | Emitted on any state change |

`spotify` and `websocket` are string enums: `"Disconnected"`, `"Connecting"`, `"Connected"`.

### Tauri Commands (desktop only)

| Command | Parameters | Returns | Description |
|---------|-----------|---------|-------------|
| `get_status` | none | `{ spotify, websocket, now_playing, last_error }` | Current relay state |
| `get_config_status` | none | `boolean` | Whether config is complete |
| `get_close_to_tray` | none | `boolean` | Current close-to-tray preference |
| `reload_config` | none | `boolean` | Reload config from store, restart relay, return configured status |
| `restart_relay` | none | `()` | Stop and restart the relay (for manual reconnection) |

## System Tray (desktop only)

- Left-click: show window
- Menu items: Show, Status (read-only, updates dynamically), Quit
- Tooltip updates with current track: "Music Relay - Artist - Track"
- Close-to-tray: window hides on close instead of quitting (configurable)

## First-Run Setup

### Desktop

1. Open [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard), create an app, set redirect URI to `http://127.0.0.1:18974/callback`
2. Launch Music Relay. The settings window opens automatically
3. Enter the Spotify Client ID, Server URL, and API Key. Click Save
4. The app authenticates with Spotify (opens browser), connects to the server, and minimizes to the system tray

### Headless

1. Create a Spotify app as above
2. Run `relay-headless`. It prompts for Server URL, API Key, Spotify Client ID, and poll interval
3. Config is saved to `~/.config/music-relay/config.json`
4. The relay prints a Spotify authorization URL. Open it in a browser on any device that can reach `127.0.0.1:18974`
5. After authorization, the relay connects and begins operating

### Headless Deployment (Raspberry Pi)

```sh
# Copy the ARM64 binary
scp relay-headless-aarch64-unknown-linux-gnu pi@raspberrypi:/usr/local/bin/relay-headless
chmod +x /usr/local/bin/relay-headless

# Run interactive setup (first time only)
relay-headless

# Install as a systemd service
sudo cp deploy/music-relay.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now music-relay
```

The systemd unit file is at `deploy/music-relay.service`. Edit it to set the correct user or environment variables as needed.

## Building from Source

### Prerequisites

- Node.js 22+ (desktop only)
- Rust stable (1.77+)
- **Linux (desktop):** `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`
- **Windows (desktop):** WebView2 (included in Windows 10/11)

### Commands

```sh
# Desktop
npm install
cargo tauri dev            # development mode
cargo tauri build          # production build

# Headless
cargo run -p relay-headless              # development mode
cargo build --release -p relay-headless  # production build

# Cross-compile headless for ARM64
cargo install cross
cross build --release --target aarch64-unknown-linux-gnu -p relay-headless
```

### CI

GitHub Actions workflow (`.github/workflows/build.yml`) builds:
- **Desktop:** `ubuntu-22.04` and `windows-latest` via `tauri-action`
- **Headless:** `x86_64-unknown-linux-gnu` (native) and `aarch64-unknown-linux-gnu` (via `cross`)

Draft releases are created automatically on tag push (`v*`), including all desktop and headless binaries.

## Dependencies

### relay-core

| Crate | Purpose |
|-------|---------|
| `reqwest` | HTTP client for Spotify API and server token endpoint |
| `tokio-tungstenite` | WebSocket client for Centrifugo |
| `futures-util` | Stream utilities for WebSocket messages |
| `sha2` | PKCE code challenge (SHA-256) |
| `base64` | Base64url encoding/decoding for PKCE and JWT |
| `rand` | Random string generation |
| `serde` / `serde_json` | Serialization |
| `tokio` | Async runtime |
| `thiserror` | Error types |
| `log` | Logging facade |

### src-tauri (desktop)

| Crate | Purpose |
|-------|---------|
| `relay-core` | Shared relay logic |
| `tauri` | App framework (tray, windows, commands) |
| `tauri-plugin-shell` | Open URLs in browser |
| `tauri-plugin-store` | JSON config persistence |
| `tauri-plugin-autostart` | OS-level autostart registration |
| `tauri-plugin-notification` | System notifications for relay failures |
| `open` | Open browser for OAuth (non-Linux) |
| `env_logger` | Log output |

### relay-headless

| Crate | Purpose |
|-------|---------|
| `relay-core` | Shared relay logic |
| `dirs` | Platform config directory resolution |
| `ctrlc` | Signal handling (SIGINT/SIGTERM) |
| `env_logger` | Log output |

### Frontend (npm, desktop only)

| Package | Purpose |
|---------|---------|
| `react` / `react-dom` | UI framework |
| `@tauri-apps/api` | Tauri IPC (invoke, events) |
| `@tauri-apps/plugin-shell` | Open URLs from frontend |
| `@tauri-apps/plugin-store` | Read/write config from frontend |
| `@tauri-apps/plugin-autostart` | Toggle autostart from frontend |

## Migrating from 1.2.x

Version 1.3.0 replaces the static WebSocket configuration (URL, token, channel) with a server-based token flow (server URL, API key). After upgrading:

- The settings UI will show empty Server URL and API Key fields
- The relay will not start until you enter the new values
- Your Spotify refresh token is preserved (no re-authorization needed)

## License

[MIT](LICENSE)
