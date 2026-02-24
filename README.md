# Music Relay

Desktop relay application that proxies Spotify Web API requests from your local machine. Runs in the system tray and communicates with a remote server over WebSocket.

Built with [Tauri v2](https://v2.tauri.app), React, and TypeScript.

## How It Works

1. Authenticates with Spotify using OAuth PKCE (localhost redirect)
2. Connects to a remote server via WebSocket
3. Receives commands (get now-playing, search, queue tracks) and executes them against the Spotify API from the local machine's IP
4. Reports results back over the WebSocket connection

## Requirements

- Node.js 22+
- Rust stable (1.77+)
- Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`
- Windows: WebView2 (included in Windows 10/11)

## Development

```sh
npm install
cargo tauri dev
```

## Building

```sh
cargo tauri build
```

Produces `.deb` and `.AppImage` on Linux, `.msi` on Windows.

## Configuration

On first launch the app opens a settings window where you enter:

| Field | Description |
|-------|-------------|
| WebSocket URL | Server endpoint (wss://...) |
| Spotify Client ID | From your Spotify Developer Dashboard |
| Poll Interval | How often to report now-playing state (seconds) |

Credentials are stored in the OS keychain via the `keyring` crate.

## Protocol

See [PROTOCOL.md](PROTOCOL.md) for the WebSocket message format.

## License

[MIT](LICENSE)
