import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { DebugEvent } from "../components/chat/useAgentChat";

const MAX_HOOK_DEBUG_EVENTS = 200;

export interface PendingPermission {
  request_id: string;
  agent_id: string;
  session_id: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
}

export interface FileChangeData {
  agent_id: string;
  session_id: string;
  tool_name: string;
  tool_input: Record<string, unknown>;
}

const MAX_FILE_CHANGES = 500;

export interface GlobalPermissionState {
  pendingPermissions: PendingPermission[];
  respondToPermission: (requestId: string, decision: "allow" | "deny") => void;
  permissionsForSession: (agentId: string, sessionId: string) => PendingPermission[];
  hookDebugEvents: DebugEvent[];
  clearHookDebugEvents: () => void;
  fileChanges: FileChangeData[];
  clearFileChanges: () => void;
}

async function fetchPending(): Promise<PendingPermission[]> {
  try {
    const data = await invoke<{ pending: PendingPermission[] }>("list_pending_permissions");
    return data.pending ?? [];
  } catch {
    return [];
  }
}

export function useGlobalPermissions(): GlobalPermissionState {
  const [permissions, setPermissions] = useState<PendingPermission[]>([]);
  const [hookDebugEvents, setHookDebugEvents] = useState<DebugEvent[]>([]);
  const hookDebugRef = useRef<DebugEvent[]>([]);
  const [fileChanges, setFileChanges] = useState<FileChangeData[]>([]);
  const fileChangesRef = useRef<FileChangeData[]>([]);

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

  const clearFileChanges = useCallback(() => {
    fileChangesRef.current = [];
    setFileChanges([]);
  }, []);

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let cleaned = false;

    // Fetch any pending permissions we may have missed
    fetchPending().then((pending) => {
      if (cleaned || pending.length === 0) return;
      setPermissions((prev) => {
        const ids = new Set(prev.map((p) => p.request_id));
        const merged = [...prev];
        for (const p of pending) {
          if (!ids.has(p.request_id)) merged.push(p);
        }
        return merged;
      });
    });

    pushHookDebug("hook_connected", "{}");

    // Listen for permission requests via Tauri events
    listen<{ type: string; data: PendingPermission | FileChangeData | { request_id: string } }>(
      "hook-event",
      (event) => {
        if (cleaned) return;
        const msg = event.payload;
        const raw = JSON.stringify(msg);
        pushHookDebug("hook", raw);

        if (msg.type === "permission_request" && msg.data) {
          setPermissions((prev) => {
            const perm = msg.data as PendingPermission;
            if (prev.some((p) => p.request_id === perm.request_id)) return prev;
            return [...prev, perm];
          });
        } else if (msg.type === "permission_timeout" && msg.data) {
          const timeoutData = msg.data as { request_id: string };
          setPermissions((prev) =>
            prev.filter((p) => p.request_id !== timeoutData.request_id)
          );
        } else if (msg.type === "file_change" && msg.data) {
          const buf = fileChangesRef.current;
          buf.push(msg.data as FileChangeData);
          if (buf.length > MAX_FILE_CHANGES) buf.splice(0, buf.length - MAX_FILE_CHANGES);
          setFileChanges([...buf]);
        }
      }
    ).then((fn) => {
      if (cleaned) { fn(); return; }
      unlisteners.push(fn);
    });

    return () => {
      cleaned = true;
      for (const fn of unlisteners) fn();
    };
  }, [pushHookDebug]);

  const respondToPermission = useCallback(
    async (requestId: string, decision: "allow" | "deny") => {
      setPermissions((prev) => prev.filter((p) => p.request_id !== requestId));

      try {
        await invoke("permission_response", {
          request: { request_id: requestId, decision },
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
    fileChanges,
    clearFileChanges,
  };
}
