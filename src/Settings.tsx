import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { load } from "@tauri-apps/plugin-store";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";

const REDIRECT_URI = "http://127.0.0.1:18974/callback";

interface SettingsProps {
  onSaved: () => void;
  onCancel?: () => void;
}

interface StoredConfig {
  serverUrl: string;
  apiKey: string;
  spotifyClientId: string;
  pollInterval: number;
  closeToTray: boolean;
}

function validate(field: string, value: string): string | null {
  switch (field) {
    case "serverUrl":
      if (!value.trim()) return null;
      if (!/^https?:\/\/.+/.test(value.trim()))
        return "Must start with https:// (or http:// for local dev)";
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

export default function Settings({ onSaved, onCancel }: SettingsProps) {
  const [serverUrl, setServerUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [spotifyClientId, setSpotifyClientId] = useState("");
  const [pollInterval, setPollInterval] = useState(5);
  const [saving, setSaving] = useState(false);
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const [autostart, setAutostart] = useState(false);
  const [closeToTray, setCloseToTray] = useState(true);
  const [copyLabel, setCopyLabel] = useState("Copy");
  const [saveError, setSaveError] = useState<string | null>(null);
  const originalConfig = useRef<StoredConfig | null>(null);

  useEffect(() => {
    load("config.json").then(async (store) => {
      const url = await store.get<string>("server_url");
      const key = await store.get<string>("api_key");
      const clientId = await store.get<string>("spotify_client_id");
      const interval = await store.get<number>("poll_interval_secs");
      const ctt = await store.get<boolean>("close_to_tray");
      if (url) setServerUrl(url);
      if (key) setApiKey(key);
      if (clientId) setSpotifyClientId(clientId);
      if (interval) setPollInterval(interval);
      if (ctt !== null && ctt !== undefined) setCloseToTray(ctt);
      originalConfig.current = {
        serverUrl: url || "",
        apiKey: key || "",
        spotifyClientId: clientId || "",
        pollInterval: interval || 5,
        closeToTray: ctt !== null && ctt !== undefined ? ctt : true,
      };
    });
    isEnabled().then(setAutostart);
  }, []);

  function markTouched(field: string) {
    setTouched((prev) => ({ ...prev, [field]: true }));
  }

  const serverUrlError = touched.serverUrl ? validate("serverUrl", serverUrl) : null;
  const clientIdError = touched.spotifyClientId ? validate("spotifyClientId", spotifyClientId) : null;

  const canSave =
    serverUrl.trim() !== "" &&
    apiKey.trim() !== "" &&
    spotifyClientId.trim() !== "" &&
    !validate("serverUrl", serverUrl) &&
    !validate("spotifyClientId", spotifyClientId);

  function configChanged(): boolean {
    const orig = originalConfig.current;
    if (!orig) return true;
    return (
      serverUrl.trim() !== orig.serverUrl ||
      apiKey.trim() !== orig.apiKey ||
      spotifyClientId.trim() !== orig.spotifyClientId ||
      Math.max(1, Math.min(60, pollInterval)) !== orig.pollInterval ||
      closeToTray !== orig.closeToTray
    );
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSave) return;
    setSaving(true);
    setSaveError(null);
    try {
      const store = await load("config.json");
      await store.set("server_url", serverUrl.trim());
      await store.set("api_key", apiKey.trim());
      await store.set("spotify_client_id", spotifyClientId.trim());
      await store.set("poll_interval_secs", Math.max(1, Math.min(60, pollInterval)));
      await store.set("close_to_tray", closeToTray);
      await store.save();
      if (configChanged()) {
        await invoke("reload_config");
      }
      onSaved();
    } catch (err) {
      setSaveError(`Failed to save: ${err}`);
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
                <button
                  type="button"
                  className="field-link"
                  onClick={() => {
                    invoke("open_url", { url: "https://developer.spotify.com/dashboard" })
                      .catch(() => window.open("https://developer.spotify.com/dashboard", "_blank"));
                  }}
                >
                  Create a Spotify app
                </button>
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
            <label htmlFor="server-url">Server URL</label>
            <input
              id="server-url"
              type="text"
              className={serverUrlError ? "input-error" : undefined}
              value={serverUrl}
              onChange={(e) => setServerUrl(e.target.value)}
              onBlur={() => markTouched("serverUrl")}
              placeholder="https://your-server.com"
            />
            {serverUrlError ? (
              <div className="field-error">{serverUrlError}</div>
            ) : (
              <div className="field-hint">
                The base URL of your server (e.g., https://relay.example.com)
              </div>
            )}
          </div>
          <div className="field">
            <label htmlFor="api-key">API Key</label>
            <input
              id="api-key"
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="Your connector API key"
              spellCheck={false}
              autoComplete="off"
            />
            <div className="field-hint">
              API key for authenticating with the server
            </div>
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
          <div className="field">
            <div className="toggle-row">
              <div>
                <div className="toggle-row-label">Minimize to tray on close</div>
                <div className="toggle-row-hint">
                  Keep running in the background when you close the window
                </div>
              </div>
              <button
                type="button"
                role="switch"
                aria-checked={closeToTray}
                className={`toggle${closeToTray ? " toggle-on" : ""}`}
                onClick={() => setCloseToTray(!closeToTray)}
              >
                <span className="toggle-knob" />
              </button>
            </div>
          </div>
        </div>

        {saveError && (
          <div className="card error-card">
            <div className="error-text">{saveError}</div>
          </div>
        )}

        <div style={{ marginTop: 14, display: "flex", justifyContent: "flex-end", gap: 8 }}>
          {onCancel && (
            <button
              type="button"
              className="btn btn-secondary"
              onClick={onCancel}
              disabled={saving}
            >
              Cancel
            </button>
          )}
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
