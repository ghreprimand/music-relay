# WebSocket Protocol

Music Relay connects to a [Centrifugo](https://centrifugal.dev/) server using the JSON protocol over WebSocket. All communication happens through a single channel.

## Transport

- **Protocol:** Centrifugo JSON (line-delimited)
- **Connection:** WebSocket derived from the server URL (e.g. `https://example.com` becomes `wss://example.com/connection/websocket`)
- **Authentication:** JWT connection token fetched from `GET {server_url}/api/connector/token` with `Authorization: Bearer {api_key}`
- **Channel:** Returned by the token endpoint alongside the JWT
- **Keep-alive:** Application-level ping/pong (empty `{}` frames)

## Token Lifecycle

1. On startup and every reconnect, the client fetches a fresh JWT and channel from the token endpoint
2. The token endpoint returns `{ "token": "eyJ...", "channel": "..." }`
3. The WebSocket URL is derived from the configured server URL by swapping the scheme (`https` to `wss`) and appending `/connection/websocket`
4. The client decodes the JWT `exp` claim (without signature verification) to determine token expiry
5. If the `exp` claim is present, the client schedules a proactive reconnect 1 hour before token expiry to avoid disruption from server-side disconnects
6. If no `exp` claim is found or the token is already within the refresh window, the client logs a warning and relies on the server to disconnect at expiry

## Message Flow

1. Client fetches a JWT connection token and channel from the server
2. Client derives the WebSocket URL from the configured server URL
3. Client connects to WebSocket
4. Client sends `connect` command with the JWT token
5. Client sends `subscribe` command for the derived channel
6. Server publishes commands to the channel as Centrifugo publications
7. Client executes commands against the Spotify API
8. Client publishes responses back to the same channel
9. Client proactively publishes now-playing updates when the track changes
10. Before token expiry, the client disconnects and reconnects with a fresh token

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

### `get_playback_state`

```json
{
  "command": "get_playback_state",
  "id": "req-005"
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
    "artists": [ { "id": "...", "name": "Artist Name" } ],
    "album": { "id": "...", "name": "Album Name", "images": [ ... ] }
  },
  "context": {
    "type": "playlist",
    "uri": "spotify:playlist:37i9dQZF1DXcBWIGoYBM5M"
  },
  "shuffle_state": false,
  "device": {
    "id": "abc123",
    "name": "My Speaker",
    "is_active": true
  }
}
```

Returns `null` if no active device (HTTP 204 from Spotify). The `context`, `shuffle_state`, and `device` fields may be `null`.

### `get_playlist_tracks`

```json
{
  "command": "get_playlist_tracks",
  "id": "req-006",
  "playlist_id": "37i9dQZF1DXcBWIGoYBM5M",
  "offset": 0,
  "limit": 50
}
```

`offset` defaults to 0, `limit` defaults to 100 (clamped to max 100).

**Response result:**

```json
{
  "items": [
    {
      "track": {
        "id": "...",
        "name": "Track Name",
        "uri": "spotify:track:...",
        "duration_ms": 200000,
        "artists": [ ... ],
        "album": { ... }
      }
    }
  ],
  "total": 250
}
```

`track` may be `null` for local or unavailable tracks.

### `add_to_playlist`

```json
{
  "command": "add_to_playlist",
  "id": "req-007",
  "playlist_id": "37i9dQZF1DXcBWIGoYBM5M",
  "uris": ["spotify:track:4iV5W9uYEdYUVa79Axb7Rh"],
  "position": 0
}
```

`position` is optional (appends to end if omitted). Accepts more than 100 URIs (batched automatically).

**Response result:**

```json
{
  "snapshot_id": "MTcsZjM..."
}
```

### `remove_from_playlist`

```json
{
  "command": "remove_from_playlist",
  "id": "req-008",
  "playlist_id": "37i9dQZF1DXcBWIGoYBM5M",
  "uris": ["spotify:track:4iV5W9uYEdYUVa79Axb7Rh"]
}
```

Removes all occurrences of the given URIs. Accepts more than 100 URIs (batched automatically).

**Response result:**

```json
{
  "snapshot_id": "MTcsZjM..."
}
```

### `replace_playlist`

```json
{
  "command": "replace_playlist",
  "id": "req-009",
  "playlist_id": "37i9dQZF1DXcBWIGoYBM5M",
  "uris": ["spotify:track:4iV5W9uYEdYUVa79Axb7Rh", "spotify:track:7tFiyTwD0nx5a1eklYtX2J"]
}
```

Replaces all tracks in the playlist. Accepts more than 100 URIs (first 100 via PUT, remaining batched via POST).

**Response result:**

```json
{
  "snapshot_id": "MTcsZjM..."
}
```

### `create_playlist`

```json
{
  "command": "create_playlist",
  "id": "req-010",
  "name": "My New Playlist",
  "description": "Optional description",
  "public": false
}
```

`description` and `public` are optional. `public` defaults to `false`.

**Response result:**

```json
{
  "id": "3cEYpjA9oz9GiPac4AsH4n",
  "external_urls": {
    "spotify": "https://open.spotify.com/playlist/3cEYpjA9oz9GiPac4AsH4n"
  }
}
```

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

Broadcasts are sent when the track URI changes, not on every poll tick. A broadcast is also sent when playback stops (all string fields empty, `is_playing: false`).

## Centrifugo Wire Format

All messages are wrapped in Centrifugo's publish command on the wire:

```json
{
  "id": 5,
  "publish": {
    "channel": "prod:relay:abc123",
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
| `user-read-playback-state` | `get_queue`, `get_playback_state` |
| `user-modify-playback-state` | `add_to_queue` |
| `playlist-read-private` | `get_playlist_tracks` |
| `playlist-read-collaborative` | `get_playlist_tracks` |
| `playlist-modify-public` | `add_to_playlist`, `remove_from_playlist`, `replace_playlist`, `create_playlist` |
| `playlist-modify-private` | `add_to_playlist`, `remove_from_playlist`, `replace_playlist`, `create_playlist` |

## Connection Lifecycle

- On every connect/reconnect, a fresh Centrifugo JWT is fetched from the token endpoint
- Reconnects automatically on WebSocket disconnect or ping timeout
- Proactive reconnect scheduled 1 hour before JWT expiry (tokens are valid for 24 hours)
- Exponential backoff: 2s, 4s, 8s, 16s, 30s (capped)
- Backoff resets on successful connection
- Spotify token refreshes automatically before expiry (60s buffer)
- Relay-level failures (auth errors, network outages) retry up to 5 times before giving up
- On permanent failure, a system notification is shown and the stored refresh token is cleared
