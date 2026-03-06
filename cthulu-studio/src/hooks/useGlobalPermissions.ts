import { useState, useEffect, useCallback, useRef } from "react";
import { getServerUrl } from "../api/client";
import type { DebugEvent } from "../components/chat/useAgentChat";

function getApi() {
  return `${getServerUrl()}/api`;
}

const MAX_HOOK_DEBUG_EVENTS = 200;

export interface PendingPermission {
  request_id: string;
  agent_id: string;
  session_id: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
}

export interface GlobalPermissionState {
  pendingPermissions: PendingPermission[];
  respondToPermission: (requestId: string, decision: "allow" | "deny") => void;
  permissionsForSession: (agentId: string, sessionId: string) => PendingPermission[];
  hookDebugEvents: DebugEvent[];
  clearHookDebugEvents: () => void;
}

async function fetchPending(): Promise<PendingPermission[]> {
  try {
    const res = await fetch(`${getApi()}/hooks/pending`);
    if (!res.ok) return [];
    const data = await res.json();
    return data.pending ?? [];
  } catch {
    return [];
  }
}

export function useGlobalPermissions(): GlobalPermissionState {
  const [permissions, setPermissions] = useState<PendingPermission[]>([]);
  const [hookDebugEvents, setHookDebugEvents] = useState<DebugEvent[]>([]);
  const hookDebugRef = useRef<DebugEvent[]>([]);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const pushHookDebug = useCallback((type: string, data: string) => {
    const entry: DebugEvent = { ts: Date.now(), type, data };
    const buf = hookDebugRef.current;
    buf.push(entry);
    if (buf.length > MAX_HOOK_DEBUG_EVENTS) buf.shift();
    setHookDebugEvents([...buf]);
  }, []);

  const clearHookDebugEvents = useCallback(() => {
    hookDebugRef.current = [];
    setHookDebugEvents([]);
  }, []);

  useEffect(() => {
    const controller = new AbortController();

    function connect() {
      fetch(`${getApi()}/hooks/stream`, {
        signal: controller.signal,
        headers: { Accept: "text/event-stream" },
      })
        .then(async (res) => {
          if (!res.ok || !res.body) {
            throw new Error(`Hook stream HTTP ${res.status}`);
          }

          pushHookDebug("hook_connected", "{}");

          // On connect, merge any pending requests we may have missed
          const pending = await fetchPending();
          if (pending.length > 0) {
            setPermissions((prev) => {
              const ids = new Set(prev.map((p) => p.request_id));
              const merged = [...prev];
              for (const p of pending) {
                if (!ids.has(p.request_id)) merged.push(p);
              }
              return merged;
            });
          }

          const reader = res.body.getReader();
          const decoder = new TextDecoder();
          let buffer = "";

          while (true) {
            const { done, value } = await reader.read();
            if (done) break;

            buffer += decoder.decode(value, { stream: true });
            const lines = buffer.split("\n");
            buffer = lines.pop() ?? "";

            for (const line of lines) {
              if (!line.startsWith("data: ")) continue;
              const raw = line.slice(6).trim();
              if (!raw || raw === "{}") continue;

              pushHookDebug("hook", raw);

              try {
                const msg = JSON.parse(raw);
                if (msg.type === "permission_request" && msg.data) {
                  setPermissions((prev) => {
                    if (prev.some((p) => p.request_id === msg.data.request_id)) return prev;
                    return [...prev, msg.data as PendingPermission];
                  });
                } else if (msg.type === "permission_timeout" && msg.data) {
                  setPermissions((prev) =>
                    prev.filter((p) => p.request_id !== msg.data.request_id)
                  );
                }
              } catch {
                // ignore parse errors
              }
            }
          }
        })
        .catch((err) => {
          if (controller.signal.aborted) return;
          pushHookDebug("hook_error", String(err));
          console.warn("Hook stream error, reconnecting in 3s:", err);
        })
        .finally(() => {
          if (!controller.signal.aborted) {
            retryRef.current = setTimeout(connect, 3000);
          }
        });
    }

    connect();

    return () => {
      controller.abort();
      if (retryRef.current) clearTimeout(retryRef.current);
    };
  }, [pushHookDebug]);

  const respondToPermission = useCallback(
    async (requestId: string, decision: "allow" | "deny") => {
      setPermissions((prev) => prev.filter((p) => p.request_id !== requestId));

      try {
        await fetch(`${getApi()}/hooks/permission-response`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ request_id: requestId, decision }),
        });
      } catch (err) {
        console.error("Failed to send permission response:", err);
      }
    },
    []
  );

  const permissionsForSession = useCallback(
    (agentId: string, sessionId: string) => {
      return permissions.filter(
        (p) => p.agent_id === agentId && p.session_id === sessionId
      );
    },
    [permissions]
  );

  return {
    pendingPermissions: permissions,
    respondToPermission,
    permissionsForSession,
    hookDebugEvents,
    clearHookDebugEvents,
  };
}
