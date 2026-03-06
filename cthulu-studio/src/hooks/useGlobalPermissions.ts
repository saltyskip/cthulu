import { useState, useEffect, useCallback, useRef } from "react";

const API = "http://localhost:8081/api";

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
}

async function fetchPending(): Promise<PendingPermission[]> {
  try {
    const res = await fetch(`${API}/hooks/pending`);
    if (!res.ok) return [];
    const data = await res.json();
    return data.pending ?? [];
  } catch {
    return [];
  }
}

export function useGlobalPermissions(): GlobalPermissionState {
  const [permissions, setPermissions] = useState<PendingPermission[]>([]);
  const retryRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const controller = new AbortController();

    function connect() {
      fetch(`${API}/hooks/stream`, {
        signal: controller.signal,
        headers: { Accept: "text/event-stream" },
      })
        .then(async (res) => {
          if (!res.ok || !res.body) {
            throw new Error(`Hook stream HTTP ${res.status}`);
          }

          // On connect, fetch any pending requests we may have missed
          const pending = await fetchPending();
          if (pending.length > 0) {
            setPermissions(pending);
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
  }, []);

  const respondToPermission = useCallback(
    async (requestId: string, decision: "allow" | "deny") => {
      // Optimistically remove from UI
      setPermissions((prev) => prev.filter((p) => p.request_id !== requestId));

      try {
        await fetch(`${API}/hooks/permission-response`, {
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
        (p) => p.agent_id === agentId || p.session_id === sessionId
      );
    },
    [permissions]
  );

  return { pendingPermissions: permissions, respondToPermission, permissionsForSession };
}
