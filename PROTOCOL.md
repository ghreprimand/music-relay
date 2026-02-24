# WebSocket Protocol

Music Relay communicates with the server using a WebSocket connection (Centrifugo-compatible). All messages are JSON.

## Connection

The client connects to the configured WebSocket URL and authenticates using a connection token provided during setup.

## Server Commands

Commands sent from the server to the relay:

| Command | Description |
|---------|-------------|
| `get_now_playing` | Return the current playback state |
| `get_queue` | Return the current playback queue |
| `search` | Search Spotify for tracks matching a query |
| `add_to_queue` | Add a track URI to the playback queue |

## Client Responses

Each response includes the original command ID and either a `result` or `error` field.

```json
{
  "id": "cmd_abc123",
  "result": { ... }
}
```

```json
{
  "id": "cmd_abc123",
  "error": {
    "code": "spotify_error",
    "message": "Track not found"
  }
}
```

## Status Reports

The client periodically publishes now-playing state at the configured poll interval.

```json
{
  "type": "now_playing",
  "data": {
    "track_id": "...",
    "track_name": "...",
    "artist": "...",
    "progress_ms": 12345,
    "is_playing": true
  }
}
```

---

This document will be expanded as the protocol stabilizes.
