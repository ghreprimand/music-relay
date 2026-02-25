# WebSocket Protocol

Music Relay connects to a [Centrifugo](https://centrifugal.dev/) server using the JSON protocol over WebSocket. All communication happens through a single channel.

## Transport

- **Protocol:** Centrifugo JSON (line-delimited)
- **Connection:** WebSocket to the configured URL (e.g. `wss://example.com/connection/websocket`)
- **Authentication:** JWT connection token provided in the `connect` command
- **Keep-alive:** Application-level ping/pong (empty `{}` frames)

## Message Flow

1. Client connects to WebSocket
2. Client sends `connect` command with JWT token
3. Client sends `subscribe` command for the configured channel
4. Server publishes commands to the channel as Centrifugo publications
5. Client executes commands against the Spotify API
6. Client publishes responses back to the same channel
7. Client proactively publishes now-playing updates when the track changes

## Server Commands

Commands are delivered as Centrifugo publication data. Each command has a `command` field for routing and an `id` field for correlating responses.

### `get_now_playing`

```json
{
  "command": "get_now_playing",
  "id": "req-001"
}
```

**Response result:**

```json
{
  "is_playing": true,
  "progress_ms": 45000,
  "item": {
    "id": "4iV5W9uYEdYUVa79Axb7Rh",
    "name": "Song Name",
    "uri": "spotify:track:4iV5W9uYEdYUVa79Axb7Rh",
    "duration_ms": 240000,
    "artists": [
      { "id": "0oSGxfWSnnOXhD2fKuz2Gy", "name": "Artist Name" }
    ],
    "album": {
      "id": "6dVIqQ8qmQ5GBnJ9shOYGE",
      "name": "Album Name",
      "images": [
        { "url": "https://i.scdn.co/image/...", "height": 640, "width": 640 }
      ]
    }
  }
}
```

Returns `null` if nothing is playing (HTTP 204 from Spotify).

### `get_queue`

```json
{
  "command": "get_queue",
  "id": "req-002"
}
```

**Response result:**

```json
{
  "currently_playing": { ... },
  "queue": [
    {
      "id": "...",
      "name": "Next Track",
      "uri": "spotify:track:...",
      "duration_ms": 200000,
      "artists": [ ... ],
      "album": { ... }
    }
  ]
}
```

`currently_playing` may be `null`. `queue` may be empty.

### `search`

```json
{
  "command": "search",
  "id": "req-003",
  "query": "bohemian rhapsody"
}
```

**Response result:**

```json
{
  "tracks": {
    "items": [
      {
        "id": "7tFiyTwD0nx5a1eklYtX2J",
        "name": "Bohemian Rhapsody",
        "uri": "spotify:track:7tFiyTwD0nx5a1eklYtX2J",
        "duration_ms": 354947,
        "artists": [ { "id": "...", "name": "Queen" } ],
        "album": { "id": "...", "name": "A Night at the Opera", "images": [ ... ] }
      }
    ],
    "total": 500
  }
}
```

Returns up to 20 results per request. Search is type `track` only.

### `add_to_queue`

```json
{
  "command": "add_to_queue",
  "id": "req-004",
  "track_uri": "spotify:track:4iV5W9uYEdYUVa79Axb7Rh"
}
```

**Response result:**

```json
{
  "success": true
}
```

Requires Spotify Premium on the DJ's account.

## Client Responses

Every response is published to the channel as Centrifugo publication data. Responses include the original command `id` and either a `result` or `error` field.

**Success:**

```json
{
  "id": "req-001",
  "result": { ... }
}
```

**Error:**

```json
{
  "id": "req-001",
  "error": {
    "code": "spotify_error",
    "message": "Not authenticated"
  }
}
```

### Error Codes

| Code | Meaning |
|------|---------|
| `spotify_error` | Spotify API returned an error (message contains details) |

## Now-Playing Broadcast

When the currently playing track changes, the client publishes an unsolicited update to the channel. These have an empty `id` field.

```json
{
  "id": "",
  "result": {
    "type": "now_playing",
    "data": {
      "track_name": "Song Name",
      "artist_name": "Artist Name",
      "album_name": "Album Name",
      "album_art_url": "https://i.scdn.co/image/...",
      "is_playing": true,
      "progress_ms": 45000,
      "duration_ms": 240000,
      "track_uri": "spotify:track:4iV5W9uYEdYUVa79Axb7Rh"
    }
  }
}
```

Broadcasts are only sent when the track URI changes, not on every poll tick.

## Centrifugo Wire Format

All messages are wrapped in Centrifugo's publish command on the wire:

```json
{
  "id": 5,
  "publish": {
    "channel": "relay:your-channel",
    "data": { ... }
  }
}
```

The `data` field contains either a server command (inbound) or a client response/broadcast (outbound) as described above.

## Spotify API Scopes

The relay requests these OAuth scopes:

| Scope | Used By |
|-------|---------|
| `user-read-currently-playing` | `get_now_playing`, `get_queue`, now-playing broadcast |
| `user-read-playback-state` | `get_queue` |
| `user-modify-playback-state` | `add_to_queue` |

## Connection Lifecycle

- Reconnects automatically on WebSocket disconnect or ping timeout
- Exponential backoff: 2s, 4s, 8s, 16s, 30s (capped)
- Backoff resets on successful connection
- Spotify token refreshes automatically before expiry (60s buffer)
