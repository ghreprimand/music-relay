# Architecture Audit

Here is an audit of the `music-relay` codebase highlighting areas for architectural improvement and potential code smells. The codebase is generally well-structured—particularly the use of a `RelayPlatform` trait to separate core logic from Tauri/headless host implementations—but there are several areas that could be refined:

## 1. Code Duplication in `SpotifyClient` (`crates/relay-core/src/spotify.rs`)
The HTTP method helpers (`api_get`, `api_post`, `api_put`, `api_delete`) share nearly identical boilerplate for token validation, header injection, and retry logic (specifically handling 401 Unauthorized responses). 
* **Recommendation:** Consolidate these into a single, private generic `request` method that takes the HTTP method and payload as arguments to DRY up the client.

## 2. Excessive Complexity in `relay.rs`
The `run_with_centrifugo` function does too much. It handles token refresh timing, WebSocket lifecycle events, polling intervals, and command dispatching all in one large loop.
* **Recommendation:** Break this loop down into a state-machine or smaller, focused actor components. Separating the Centrifugo WebSocket lifecycle from the Spotify polling logic would make it much easier to maintain and test.

## 3. Manual OAuth / PKCE Implementation (`crates/relay-core/src/oauth.rs`)
The codebase implements its own PKCE challenge generation, custom URL encoding, and a manual TCP listener for the OAuth callback.
* **Recommendation:** Adopt established crates like `oauth2` for the flow and `urlencoding` for safe string handling. This reduces the maintenance burden and prevents subtle security or parsing bugs in hand-rolled auth logic.

## 4. Inconsistent Error Handling
Error handling across the workspace is fragmented. Some modules use `thiserror` for strongly-typed errors (e.g., `spotify.rs`, `centrifugo.rs`), while others rely on `Box<dyn Error>` or custom ad-hoc enums.
* **Recommendation:** Standardize error handling. Using `thiserror` for library crates (`relay-core`) and `anyhow` for the application binaries (`relay-headless`, `src-tauri`) is the standard Rust idiom and would drastically improve debugging and error propagation.

## 5. State Management Fragmentation
The current pattern of passing a callback to update state (`platform.update_state(|state| ...)`) is functional but can lead to fragmented and hard-to-trace state updates as the application grows.
* **Recommendation:** Moving to a more centralized, event-driven architecture (such as message passing via `mpsc` channels) would scale better and prevent potential deadlocks or race conditions as more features are added.