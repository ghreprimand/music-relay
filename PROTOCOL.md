# WebSocket Protocol

Music Relay connects to a [Centrifugo](https://centrifugal.dev/) server using the JSON protocol over WebSocket. All communication happens through a single channel.

## Transport

- **Protocol:** Centrifugo JSON (line-delimited)
- **Connection:** WebSocket URL returned by the token endpoint
- **Authentication:** JWT connection token fetched from `GET {server_url}/api/connector/token` with `Authorization: Bearer {api_key}`
- **Channel:** Returned by the token endpoint alongside the JWT
- **Keep-alive:** Application-level ping/pong (empty `{}` frames)

## Token Lifecycle

1. On startup and every reconnect, the client fetches a fresh JWT, channel, and WebSocket URL from the token endpoint
2. The token endpoint returns `{ "token": "eyJ...", "channel": "...", "websocket_url": "wss://..." }`
3. The client decodes the JWT `exp` claim (without signature verification) to determine token expiry
5. If the `exp` claim is present, the client schedules a proactive reconnect 1 hour before token expiry to avoid disruption from server-side disconnects
6. If no `exp` claim is found or the token is already within the refresh window, the client logs a warning and relies on the server to disconnect at expiry

## Message Flow

1. Client fetches a JWT connection token, channel, and WebSocket URL from the server
2. Client connects to the returned WebSocket URL
3. Client sends `connect` command with the JWT token
4. Client sends `subscribe` command for the returned channel
5. Server publishes commands to the channel as Centrifugo publications
6. Client executes commands against the Spotify API
7. Client publishes responses back to the same channel
8. Client proactively publishes now-playing updates when the track changes
9. Before token expiry, the client disconnects and reconnects with a fresh token

## Server Commands

Commands are delivered as Centrifugo publication data. Each command has a `command` field for routing and an `id` field for correlating responses. Mutating commands may include an optional `nonce` field (UUID string) for deduplication -- see [Command Deduplication](#command-deduplication) below.

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

### `get_artists`

```json
{
  "command": "get_artists",
  "id": "req-011",
  "artist_ids": ["0oSGxfWSnnOXhD2fKuz2Gy", "6sFIWsNpZYqbRiJfEyKSzF"]
}
```

Accepts up to 50 artist IDs.

**Response result:**

```json
{
  "artists": [
    {
      "id": "0oSGxfWSnnOXhD2fKuz2Gy",
      "name": "Artist Name",
      "genres": ["pop", "dance pop"],
      "popularity": 82
    }
  ]
}
```

Elements in the `artists` array may be `null` if an ID was not found.

### `get_playlist_details`

```json
{
  "command": "get_playlist_details",
  "id": "req-012",
  "playlist_id": "37i9dQZF1DXcBWIGoYBM5M"
}
```

**Response result:**

```json
{
  "id": "37i9dQZF1DXcBWIGoYBM5M",
  "name": "Today's Top Hits",
  "owner": {
    "id": "spotify",
    "display_name": "Spotify"
  },
  "tracks": {
    "total": 50
  },
  "external_urls": {
    "spotify": "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M"
  }
}
```

Uses Spotify's `fields` parameter to return only the listed fields.

### `get_current_user`

```json
{
  "command": "get_current_user",
  "id": "req-013"
}
```

**Response result:**

```json
{
  "id": "user123",
  "display_name": "DJ Name"
}
```

The `display_name` field may be `null`. The result is cached for the lifetime of the relay session.

### `pause`

```json
{
  "command": "pause",
  "id": "req-014"
}
```

**Response result:**

```json
{}
```

Pauses playback on the active device. Requires Spotify Premium.

### `resume`

```json
{
  "command": "resume",
  "id": "req-015"
}
```

**Response result:**

```json
{}
```

Resumes playback on the active device. Requires Spotify Premium.

### `skip_next`

```json
{
  "command": "skip_next",
  "id": "req-016"
}
```

**Response result:**

```json
{}
```

Skips to the next track. Requires Spotify Premium.

### `skip_previous`

```json
{
  "command": "skip_previous",
  "id": "req-017"
}
```

**Response result:**

```json
{}
```

Skips to the previous track. Requires Spotify Premium.

### `set_volume`

```json
{
  "command": "set_volume",
  "id": "req-018",
  "volume_percent": 75
}
```

**Response result:**

```json
{}
```

Sets playback volume (0-100). Clamped to 100 if higher. Requires Spotify Premium.

### `fade_skip`

```json
{
  "command": "fade_skip",
  "id": "req-019"
}
```

**Response result:**

```json
{}
```

Or with warning:

```json
{
  "warning": "Could not read volume"
}
```

Gradually reduces volume to zero (5 steps, ~200ms apart), skips to the next track, waits 500ms, then restores the original volume. If the current volume cannot be read, skips without fading and returns a warning. Requires Spotify Premium.

### `fade_pause`

```json
{
  "command": "fade_pause",
  "id": "req-020"
}
```

**Response result:**

```json
{}
```

Or with warning:

```json
{
  "warning": "Could not read volume"
}
```

Gradually reduces volume to zero (5 steps, ~200ms apart), pauses playback, then restores the original volume (so it is correct when playback is resumed). If the current volume cannot be read, pauses without fading and returns a warning. Requires Spotify Premium.

## Command Deduplication

When multiple relay instances are subscribed to the same channel, mutating commands would be executed by each instance. To prevent this, the server can include a `nonce` field (UUID string) on mutating commands. The relay uses this nonce to claim exclusive execution rights before proceeding.

### Nonce Field

Any command may include an optional `nonce` field:

```json
{
  "command": "add_to_queue",
  "id": "req-004",
  "track_uri": "spotify:track:4iV5W9uYEdYUVa79Axb7Rh",
  "nonce": "550e8400-e29b-41d4-a716-446655440000"
}
```

The `nonce` is ignored by the relay for read-only commands (`get_now_playing`, `get_queue`, `search`, `get_playback_state`, `get_playlist_tracks`, `get_playlist_details`, `get_artists`, `get_current_user`). For mutating commands (`add_to_queue`, `add_to_playlist`, `remove_from_playlist`, `replace_playlist`, `create_playlist`, `pause`, `resume`, `skip_next`, `skip_previous`, `set_volume`, `fade_skip`, `fade_pause`), the relay performs a claim check before executing.

### Claim Endpoint

```
POST {server_url}/api/connector/claim-command
Authorization: Bearer {api_key}
Content-Type: application/json

{ "nonce": "550e8400-e29b-41d4-a716-446655440000" }
```

| Status | Meaning |
|--------|---------|
| 200 OK | This relay claimed the command; proceed with execution |
| 409 Conflict | Another relay already claimed it; skip execution |

The claim request uses a 3-second timeout. If the request fails for any reason (network error, timeout, non-200/409 status), the relay executes the command anyway (fail-open). When a command is skipped, the relay still returns a success response so the server does not interpret it as a failure.

If no `nonce` is present (backwards compatibility), the claim check is skipped and the command executes normally.

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
| `forbidden` | Spotify returned 403 (e.g. Premium required) |
| `no_device` | No active Spotify device (Spotify returned 404) |
| `rate_limited` | Spotify rate limit exceeded (429) |

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
| `user-read-playback-state` | `get_queue`, `get_playback_state`, `fade_skip`, `fade_pause` |
| `user-modify-playback-state` | `add_to_queue`, `pause`, `resume`, `skip_next`, `skip_previous`, `set_volume`, `fade_skip`, `fade_pause` |
| `playlist-read-private` | `get_playlist_tracks`, `get_playlist_details` |
| `playlist-read-collaborative` | `get_playlist_tracks`, `get_playlist_details` |
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
