import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface NowPlayingInfo {
  track_name: string;
  artist_name: string;
  album_name: string;
  album_art_url: string | null;
  is_playing: boolean;
  progress_ms: number | null;
  duration_ms: number;
  track_uri: string;
}

interface StatusData {
  spotify: string;
  websocket: string;
  now_playing: NowPlayingInfo | null;
  last_error: string | null;
}

interface StatusProps {
  onOpenSettings: () => void;
}

function StatusDot({ status }: { status: string }) {
  const cls =
    status === "Connected"
      ? "connected"
      : status === "Connecting"
        ? "connecting"
        : "disconnected";
  return <span className={`status-dot ${cls}`} />;
}

function formatStatus(status: string): string {
  switch (status) {
    case "Connected":
      return "Connected";
    case "Connecting":
      return "Connecting...";
    default:
      return "Disconnected";
  }
}

function formatNowPlaying(np: NowPlayingInfo | null): string {
  if (!np) return "Nothing playing";
  if (!np.is_playing) return "Paused";
  return `${np.artist_name} - ${np.track_name}`;
}

export default function Status({ onOpenSettings }: StatusProps) {
  const [status, setStatus] = useState<StatusData | null>(null);
  const [reconnecting, setReconnecting] = useState(false);

  useEffect(() => {
    invoke<StatusData>("get_status").then(setStatus);

    const unlisten = listen<StatusData>("status-changed", (event) => {
      setStatus(event.payload);
      setReconnecting(false);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  function handleReconnect() {
    setReconnecting(true);
    invoke("restart_relay").catch(() => setReconnecting(false));
  }

  const showReconnect =
    status &&
    !reconnecting &&
    (status.last_error ||
      (status.spotify === "Disconnected" && status.websocket === "Disconnected"));

  return (
    <div className="container">
      <div className="header">
        <h1>Music Relay</h1>
        <button
          className="btn-icon"
          onClick={onOpenSettings}
          title="Settings"
          aria-label="Settings"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="8" cy="8" r="2.5" />
            <path d="M13.5 8a5.5 5.5 0 0 0-.1-.8l1.3-1-.7-1.2-1.5.5a5.5 5.5 0 0 0-1.2-.7L11 3.3H9.6l-.3 1.5a5.5 5.5 0 0 0-1.2.7L6.6 5l-.7 1.2 1.3 1a5.5 5.5 0 0 0 0 1.6l-1.3 1 .7 1.2 1.5-.5a5.5 5.5 0 0 0 1.2.7l.3 1.5H11l.3-1.5a5.5 5.5 0 0 0 1.2-.7l1.5.5.7-1.2-1.3-1a5.5 5.5 0 0 0 .1-.8z" />
          </svg>
        </button>
      </div>

      {status ? (
        <>
          <div className="card">
            <div className="status-row">
              <span className="status-label">Spotify</span>
              <span className="status-value">
                <StatusDot status={status.spotify} />
                {formatStatus(status.spotify)}
              </span>
            </div>
            <div className="status-row">
              <span className="status-label">WebSocket</span>
              <span className="status-value">
                <StatusDot status={status.websocket} />
                {formatStatus(status.websocket)}
              </span>
            </div>
            <div className="status-row">
              <span className="status-label">Now Playing</span>
              <span className="status-value muted">
                {formatNowPlaying(status.now_playing)}
              </span>
            </div>
          </div>

          {status.last_error && (
            <div className="card error-card">
              <div className="error-text">{status.last_error}</div>
            </div>
          )}

          {showReconnect && (
            <div style={{ marginTop: 12, display: "flex", justifyContent: "center" }}>
              <button
                className="btn btn-primary"
                onClick={handleReconnect}
              >
                Reconnect
              </button>
            </div>
          )}

          {reconnecting && (
            <div style={{ marginTop: 12, textAlign: "center" }}>
              <span className="muted">Reconnecting...</span>
            </div>
          )}
        </>
      ) : (
        <div className="card">
          <p className="muted">Loading...</p>
        </div>
      )}

      <div className="tray-note">
        This window can be closed. Music Relay continues in the system tray.
      </div>
    </div>
  );
}
