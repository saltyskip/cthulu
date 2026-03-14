/**
 * useWorkflowActions.ts
 *
 * React hook that listens to PTY output from the Studio Assistant terminal,
 * detects `create_flow` JSON actions, and automatically:
 *  1. Publishes the workflow via `api.publishWorkflow()`
 *  2. Opens it in the canvas editor via the provided `openWorkflow` callback
 *
 * Usage:
 *   useWorkflowActions({ sessionId, workspace, openWorkflow });
 *
 * The hook taps into the same `pty-data-{sessionId}` Tauri event that
 * AgentTerminal uses, accumulating text in a buffer and running the parser
 * after each chunk.
 */

import { useEffect, useRef, useCallback } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { parseAgentActions, type CreateFlowAction } from "../utils/parseAgentActions";
import * as api from "../api/client";
import { log } from "../api/logger";

interface UseWorkflowActionsOptions {
  /** The PTY session ID to listen on. Null = disabled. */
  sessionId: string | null;
  /** Target workspace for new workflows. Null = use "default". */
  workspace: string | null;
  /** Callback to open a workflow in the canvas editor. */
  openWorkflow: (workspace: string, name: string) => Promise<void>;
  /** Optional: refresh workflow list after creation. */
  onWorkflowCreated?: (workspace: string, name: string) => void;
}

export function useWorkflowActions({
  sessionId,
  workspace,
  openWorkflow,
  onWorkflowCreated,
}: UseWorkflowActionsOptions): void {
  const bufferRef = useRef("");
  // Track which workflow names we've already processed (avoid duplicates
  // if the same JSON block stays in the buffer across partial writes)
  const processedRef = useRef(new Set<string>());

  const handleAction = useCallback(
    async (action: CreateFlowAction) => {
      const ws = workspace || "default";
      const name = action.name;

      // Dedup: don't process the same workflow name twice in one session
      if (processedRef.current.has(`${ws}::${name}`)) return;
      processedRef.current.add(`${ws}::${name}`);

      log("info", `[useWorkflowActions] Detected create_flow: ${ws}/${name}`);

      // Build the flow payload matching publishWorkflow's expected shape
      const nodes = action.nodes.map((n, i) => ({
        id: `node-${i}`,
        node_type: n.node_type,
        kind: n.kind,
        label: n.label,
        config: n.config,
        position: { x: 300 * i, y: 100 },
      }));

      const flow: Record<string, unknown> = {
        name: action.name,
        description: action.description || "",
        nodes,
        edges: action.edges, // "auto" or array — backend handles both
      };

      try {
        await api.publishWorkflow(ws, name, flow);
        log("info", `[useWorkflowActions] Published workflow ${ws}/${name}`);

        // Notify caller (e.g. to refresh sidebar workflow list)
        onWorkflowCreated?.(ws, name);

        // Open in the canvas editor
        await openWorkflow(ws, name);
        log("info", `[useWorkflowActions] Opened workflow ${ws}/${name} in editor`);
      } catch (e) {
        const msg = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
        log("error", `[useWorkflowActions] Failed to create workflow ${ws}/${name}: ${msg}`);
      }
    },
    [workspace, openWorkflow, onWorkflowCreated],
  );

  useEffect(() => {
    if (!sessionId) return;

    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    // Reset buffer when session changes
    bufferRef.current = "";
    processedRef.current = new Set();

    (async () => {
      unlisten = await listen<string>(`pty-data-${sessionId}`, (event) => {
        if (cancelled) return;

        // Append new data to buffer
        bufferRef.current += event.payload;

        // Run the parser
        const result = parseAgentActions(bufferRef.current);

        // Trim consumed portion from buffer
        if (result.consumedUpTo > 0) {
          bufferRef.current = bufferRef.current.slice(result.consumedUpTo);
        }

        // Process any detected actions
        for (const action of result.actions) {
          handleAction(action);
        }
      });

      if (cancelled) {
        unlisten?.();
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [sessionId, handleAction]);
}
