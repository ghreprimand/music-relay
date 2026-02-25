# Changelog

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
