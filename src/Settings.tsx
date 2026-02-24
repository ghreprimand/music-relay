import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { load } from "@tauri-apps/plugin-store";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";

const REDIRECT_URI = "http://127.0.0.1:18974/callback";

interface SettingsProps {
  onSaved: () => void;
}

function validate(field: string, value: string): string | null {
  switch (field) {
    case "websocketUrl":
      if (!value.trim()) return null;
      if (!/^wss?:\/\/.+/.test(value.trim()))
        return "Must start with wss:// (or ws:// for local dev)";
      return null;
    case "spotifyClientId":
      if (!value.trim()) return null;
      if (!/^[a-f0-9]{32}$/i.test(value.trim()))
        return "Should be 32 hex characters — find it in your Spotify app dashboard";
      return null;
    default:
      return null;
  }
}

export default function Settings({ onSaved }: SettingsProps) {
  const [websocketUrl, setWebsocketUrl] = useState("");
  const [spotifyClientId, setSpotifyClientId] = useState("");
  const [pollInterval, setPollInterval] = useState(5);
  const [saving, setSaving] = useState(false);
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const [autostart, setAutostart] = useState(false);
  const [copyLabel, setCopyLabel] = useState("Copy");

  useEffect(() => {
    load("config.json").then(async (store) => {
      const url = await store.get<string>("websocket_url");
      const clientId = await store.get<string>("spotify_client_id");
      const interval = await store.get<number>("poll_interval_secs");
      if (url) setWebsocketUrl(url);
      if (clientId) setSpotifyClientId(clientId);
      if (interval) setPollInterval(interval);
    });
    isEnabled().then(setAutostart);
  }, []);

  function markTouched(field: string) {
    setTouched((prev) => ({ ...prev, [field]: true }));
  }

  const wsError = touched.websocketUrl ? validate("websocketUrl", websocketUrl) : null;
  const clientIdError = touched.spotifyClientId ? validate("spotifyClientId", spotifyClientId) : null;

  const canSave =
    websocketUrl.trim() !== "" &&
    spotifyClientId.trim() !== "" &&
    !validate("websocketUrl", websocketUrl) &&
    !validate("spotifyClientId", spotifyClientId);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSave) return;
    setSaving(true);
    try {
      const store = await load("config.json");
      await store.set("websocket_url", websocketUrl.trim());
      await store.set("spotify_client_id", spotifyClientId.trim());
      await store.set("poll_interval_secs", Math.max(1, Math.min(60, pollInterval)));
      await store.save();
      await invoke("reload_config");
      onSaved();
    } finally {
      setSaving(false);
    }
  }

  function handleCopy() {
    navigator.clipboard.writeText(REDIRECT_URI).then(() => {
      setCopyLabel("Copied");
      setTimeout(() => setCopyLabel("Copy"), 1500);
    });
  }

  async function handleAutostart(enabled: boolean) {
    setAutostart(enabled);
    if (enabled) {
      await enable();
    } else {
      await disable();
    }
  }

  return (
    <div className="container">
      <div className="header">
        <h1>Settings</h1>
      </div>
      <form onSubmit={handleSubmit} className="settings-form">
        {/* Spotify */}
        <div className="card">
          <div className="card-title">Spotify</div>
          <div className="setup-steps">
            <div className="setup-step">
              <span className="step-number">1</span>
              <span>
                <a
                  href="#"
                  className="field-link"
                  onClick={(e) => {
                    e.preventDefault();
                    open("https://developer.spotify.com/dashboard");
                  }}
                >
                  Create a Spotify app
                </a>
                {" "}on the developer dashboard
              </span>
            </div>
            <div className="setup-step">
              <span className="step-number">2</span>
              <span>
                Set the redirect URI to:
                <div className="redirect-uri-row">
                  <code className="redirect-uri">{REDIRECT_URI}</code>
                  <button type="button" className="btn-copy" onClick={handleCopy}>
                    {copyLabel}
                  </button>
                </div>
              </span>
            </div>
            <div className="setup-step">
              <span className="step-number">3</span>
              <span>Paste your Client ID below</span>
            </div>
          </div>
          <div className="field">
            <label htmlFor="client-id">Client ID</label>
            <input
              id="client-id"
              type="text"
              className={clientIdError ? "input-error" : undefined}
              value={spotifyClientId}
              onChange={(e) => setSpotifyClientId(e.target.value)}
              onBlur={() => markTouched("spotifyClientId")}
              placeholder="32-character hex string"
              spellCheck={false}
              autoComplete="off"
            />
            {clientIdError && <div className="field-error">{clientIdError}</div>}
          </div>
        </div>

        {/* Server */}
        <div className="card">
          <div className="card-title">Server</div>
          <div className="field">
            <label htmlFor="ws-url">WebSocket URL</label>
            <input
              id="ws-url"
              type="text"
              className={wsError ? "input-error" : undefined}
              value={websocketUrl}
              onChange={(e) => setWebsocketUrl(e.target.value)}
              onBlur={() => markTouched("websocketUrl")}
              placeholder="wss://your-server.com/connection/websocket"
            />
            {wsError ? (
              <div className="field-error">{wsError}</div>
            ) : (
              <div className="field-hint">
                The Centrifugo WebSocket endpoint provided by your server admin
              </div>
            )}
          </div>
        </div>

        {/* Preferences */}
        <div className="card">
          <div className="card-title">Preferences</div>
          <div className="field">
            <label htmlFor="poll-interval">Poll Interval</label>
            <div className="input-with-suffix">
              <input
                id="poll-interval"
                type="number"
                min={1}
                max={60}
                value={pollInterval}
                onChange={(e) => setPollInterval(Number(e.target.value))}
              />
              <span className="input-suffix">seconds</span>
            </div>
            <div className="field-hint">
              How often to check Spotify for track updates (1-60)
            </div>
          </div>
          <div className="field">
            <div className="toggle-row">
              <div>
                <div className="toggle-row-label">Launch at startup</div>
                <div className="toggle-row-hint">
                  Start Music Relay automatically when you log in
                </div>
              </div>
              <button
                type="button"
                role="switch"
                aria-checked={autostart}
                className={`toggle${autostart ? " toggle-on" : ""}`}
                onClick={() => handleAutostart(!autostart)}
              >
                <span className="toggle-knob" />
              </button>
            </div>
          </div>
        </div>

        <div style={{ marginTop: 14, display: "flex", justifyContent: "flex-end" }}>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={saving || !canSave}
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </form>
      <div className="tray-note">
        Music Relay runs in the system tray.
      </div>
    </div>
  );
}
