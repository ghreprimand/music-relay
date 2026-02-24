import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface StatusData {
  spotify: string;
  websocket: string;
  now_playing: string | null;
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
      return "Connecting";
    default:
      return "Disconnected";
  }
}

export default function Status({ onOpenSettings }: StatusProps) {
  const [status, setStatus] = useState<StatusData | null>(null);

  useEffect(() => {
    invoke<StatusData>("get_status").then(setStatus);
  }, []);

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
              {status.now_playing ?? "Nothing"}
            </span>
          </div>
        </div>
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
