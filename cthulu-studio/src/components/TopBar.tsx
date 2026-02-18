import { useState, useEffect } from "react";
import * as api from "../api/client";
import type { Flow } from "../types/flow";

interface TopBarProps {
  flow: Flow | null;
  onTrigger: () => void;
  onToggleEnabled: () => void;
  onSettingsClick: () => void;
}

export default function TopBar({
  flow,
  onTrigger,
  onToggleEnabled,
  onSettingsClick,
}: TopBarProps) {
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const check = async () => {
      setConnected(await api.checkConnection());
    };
    check();
    const interval = setInterval(check, 10000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="top-bar">
      <h1>Cthulu Studio</h1>
      {flow && (
        <>
          <span className="flow-name">{flow.name}</span>
          <button className="ghost" onClick={onToggleEnabled}>
            {flow.enabled ? "Enabled" : "Disabled"}
          </button>
        </>
      )}
      <div className="spacer" />
      {flow && (
        <button className="primary" onClick={onTrigger} disabled={!connected}>
          Run
        </button>
      )}
      <div className="connection-status">
        <div
          className={`connection-dot ${connected ? "connected" : "disconnected"}`}
        />
        <span>{connected ? api.getServerUrl() : "Disconnected"}</span>
      </div>
      <button className="ghost" onClick={onSettingsClick}>
        Settings
      </button>
    </div>
  );
}
