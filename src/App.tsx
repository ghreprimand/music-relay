import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import Settings from "./Settings";
import Status from "./Status";

type View = "loading" | "settings" | "status";

function App() {
  const [view, setView] = useState<View>("loading");
  const wasConfigured = useRef(false);

  useEffect(() => {
    invoke<boolean>("get_config_status")
      .then((configured) => {
        wasConfigured.current = configured;
        setView(configured ? "status" : "settings");
      })
      .catch(() => {
        setView("settings");
      });
  }, []);

  if (view === "loading") {
    return null;
  }

  if (view === "settings") {
    return (
      <Settings
        onSaved={() => {
          wasConfigured.current = true;
          setView("status");
        }}
        onCancel={wasConfigured.current ? () => setView("status") : undefined}
      />
    );
  }

  return <Status onOpenSettings={() => setView("settings")} />;
}

export default App;
