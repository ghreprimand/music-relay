import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Status {
  spotify: string;
  websocket: string;
  now_playing: string | null;
}

function App() {
  const [status, setStatus] = useState<Status | null>(null);

  useEffect(() => {
    invoke<Status>("get_status").then(setStatus);
  }, []);

  return (
    <div style={{ padding: "1rem", fontFamily: "system-ui, sans-serif" }}>
      <h2>Music Relay</h2>
      {status ? (
        <table>
          <tbody>
            <tr>
              <td>Spotify</td>
              <td>{status.spotify}</td>
            </tr>
            <tr>
              <td>WebSocket</td>
              <td>{status.websocket}</td>
            </tr>
            <tr>
              <td>Now Playing</td>
              <td>{status.now_playing ?? "Nothing"}</td>
            </tr>
          </tbody>
        </table>
      ) : (
        <p>Loading...</p>
      )}
      <p style={{ marginTop: "1rem", color: "#888", fontSize: "0.85rem" }}>
        This window can be closed. Music Relay continues running in the system
        tray.
      </p>
    </div>
  );
}

export default App;
