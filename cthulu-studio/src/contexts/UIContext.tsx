import { createContext, useContext, useState, type ReactNode } from "react";

interface UIContextValue {
  sidebarCollapsed: boolean;
  setSidebarCollapsed: React.Dispatch<React.SetStateAction<boolean>>;
  showSettings: boolean;
  setShowSettings: React.Dispatch<React.SetStateAction<boolean>>;
  /** @deprecated No-op in desktop mode. Kept for backward compatibility. */
  serverUrl: string;
  /** @deprecated No-op in desktop mode. Kept for backward compatibility. */
  setServerUrl: React.Dispatch<React.SetStateAction<string>>;
}

const UIContext = createContext<UIContextValue | null>(null);

export function UIProvider({ children }: { children: ReactNode }) {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  // Kept as state for backward compatibility but no longer drives behavior
  const [serverUrl, setServerUrl] = useState("tauri://localhost");

  return (
    <UIContext.Provider
      value={{
        sidebarCollapsed,
        setSidebarCollapsed,
        showSettings,
        setShowSettings,
        serverUrl,
        setServerUrl,
      }}
    >
      {children}
    </UIContext.Provider>
  );
}

export function useUI(): UIContextValue {
  const ctx = useContext(UIContext);
  if (!ctx) throw new Error("useUI must be used within UIProvider");
  return ctx;
}
