import { useState, useEffect, useRef } from "react";
import * as api from "../api/client";
import { log } from "../api/logger";
interface TopBarProps {
  flow: { name: string; enabled: boolean } | null;
  onTrigger: () => void;
  onToggleEnabled: () => void;
  onSettingsClick: () => void;
  consoleOpen: boolean;
  onToggleConsole: () => void;
  errorCount: number;
}

export default function TopBar({
  flow,
  onTrigger,
  onToggleEnabled,
  onSettingsClick,
  consoleOpen,
  onToggleConsole,
  errorCount,
}: TopBarProps) {
  const [connected, setConnected] = useState(false);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;

    // Fast retry on boot: 1s, 2s, 3s... then settle at 10s
    const check = async (interval: number) => {
      if (cancelled) return;
      const ok = await api.checkConnection();
      if (!cancelled) {
        const wasDisconnected = !connected;
        setConnected(ok);

        if (ok && wasDisconnected) {
          log("info", `Connected to server at ${api.getServerUrl()}`);
        }

        // If still disconnected, retry faster (up to 10s)
        const nextInterval = ok ? 10000 : Math.min(interval + 1000, 10000);
        retryRef.current = setTimeout(() => check(nextInterval), nextInterval);
      }
    };

    check(1000);

    return () => {
      cancelled = true;
      if (retryRef.current) clearTimeout(retryRef.current);
    };
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
      <button
        className={`ghost ${consoleOpen ? "console-toggle-active" : ""}`}
        onClick={onToggleConsole}
        style={{ position: "relative" }}
      >
        Console
        {errorCount > 0 && !consoleOpen && (
          <span className="error-badge">{errorCount}</span>
        )}
      </button>
      <button className="ghost" onClick={onSettingsClick}>
        Settings
      </button>
    </div>
  );
}
