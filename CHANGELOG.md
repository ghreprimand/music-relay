# Changelog

## 1.4.0

### Features

- New playback control commands: `pause`, `resume`, `skip_next`, `skip_previous`, `set_volume`
- New fade commands: `fade_skip` (fade out, skip, restore volume) and `fade_pause` (fade out, pause, restore volume)
- Specific error codes for playback failures: `forbidden` (403), `no_device` (404), `rate_limited` (429)
- `Device` type now includes `volume_percent` field

## 1.3.3

- Add macOS (.dmg) build to release workflow

## 1.3.2

### Features

- New command: `get_artists` -- fetch artist details (genres, popularity) for up to 50 artist IDs
- New command: `get_playlist_details` -- fetch playlist metadata (name, owner, track count, URL)
- New command: `get_current_user` -- fetch the authenticated user's Spotify ID and display name
- `UserProfile` now includes `display_name`

## 1.3.1

- Include `popularity` field (0-100) in track objects returned by search results

## 1.3.0

### Breaking Changes

- Replaced static `websocket_token` and `websocket_channel` config fields with `server_url` and `api_key`
- Existing installations must reconfigure with the new server URL and API key after upgrading
- Spotify refresh token is preserved; no re-authentication required

### Features

- **Headless binary:** Standalone `relay-headless` CLI for running on servers and Raspberry Pi without a desktop environment
  - Interactive first-run setup prompts for all configuration
  - JSON config file at `~/.config/music-relay/config.json`
  - ARM64 (aarch64) cross-compiled builds for Raspberry Pi
  - Systemd service unit included in `deploy/music-relay.service`
- **Dynamic token acquisition:** Centrifugo connection tokens are now fetched from the server on every connect, replacing the static JWT
- **Server-provided connection params:** Channel name and WebSocket URL are now returned by the token endpoint alongside the JWT, rather than derived client-side
- **Proactive token refresh:** JWT `exp` claim is decoded on connect; the relay schedules a clean reconnect 1 hour before token expiry to avoid any disruption from server-side disconnects
- **Smarter error handling:** Revoked Spotify tokens and failed OAuth flows stop immediately instead of retrying. Only transient errors (network issues) trigger retries
- **Improved error messages:** User-facing messages are now actionable (e.g. "Spotify session expired. Click Reconnect to sign in again.")

### Architecture

- Restructured into a Cargo workspace with three crates:
  - `relay-core` -- shared library (Centrifugo, Spotify, OAuth, token handling, relay orchestration)
  - `music-relay` (src-tauri) -- Tauri desktop application
  - `relay-headless` -- headless CLI binary
- Introduced `RelayPlatform` trait to decouple relay logic from Tauri APIs
- Moved `centrifugo.rs`, `spotify.rs`, `oauth.rs`, `state.rs`, `relay.rs` into `relay-core`
- `oauth.rs` now accepts a `present_url` callback instead of directly opening a browser
- `state.rs` simplified: removed config, shutdown handle, and refresh token fields (now managed by platform implementations)
- `start_relay()` returns a future instead of spawning internally, allowing callers to use the appropriate async runtime
- `RelayError` enum distinguishes transient failures (retryable) from auth failures (immediate stop)

### CI

- Split build workflow into `build-tauri` and `build-headless` jobs
- Added ARM64 cross-compilation using `cross` for headless builds
- Headless binaries included as release assets alongside Tauri bundles

## 1.2.0

### Reliability

- Relay retries up to 5 times with exponential backoff on startup and auth failures
- Retry counter resets automatically if the relay had been running before failing
- System notification when the relay gives up: "Spotify song requests are no longer being relayed"
- Stale refresh token cleared on permanent failure so the next launch triggers a fresh OAuth flow
- Spotify refresh token now persisted after command-triggered refreshes (not just during polling)
- Spotify refresh token now persisted in poll-only mode
- 15-second HTTP timeout on all Spotify API requests
- Fixed `add_to_queue` not retrying on 401 (was catching the error but not refreshing)

### Settings

- Cancel button when navigating to settings from the status view
- Saving unchanged settings no longer restarts the relay or triggers re-authentication

### Internal

- Command execution logging for debugging
- Removed unused `url` crate dependency
- Added `tauri-plugin-notification` for system notifications

## 1.1.0

### Features

- Playlist management commands: `get_playlist_tracks`, `add_to_playlist`, `remove_from_playlist`, `replace_playlist`, `create_playlist`
- New OAuth scopes for playlist access

### Fixes

- Fixed browser open failing in AppImage builds on Linux
- Added fallback when `open_url` command fails

## 1.0.0

Initial release.

- System tray application with Spotify OAuth PKCE authentication
- Centrifugo WebSocket client with automatic reconnection
- Commands: `get_now_playing`, `get_queue`, `search`, `add_to_queue`, `get_playback_state`
- Now-playing broadcast on track change
- Settings UI with guided Spotify app setup
- Autostart and close-to-tray preferences
- Linux (.AppImage, .deb) and Windows (.msi) builds
