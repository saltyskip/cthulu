import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { log } from "./logger";

export interface InteractSSEEvent {
  type: string;
  data: string;
}

export interface ImageData {
  media_type: string;
  data: string; // base64
}

/**
 * Start an agent chat session via Tauri IPC.
 * Invokes the `agent_chat` command and listens for streamed events
 * on the `chat-event-{sessionId}` Tauri event channel.
 * Returns an AbortController-compatible object to cancel the stream.
 */
export function startAgentChat(
  agentId: string,
  prompt: string,
  sessionId: string | null,
  onEvent: (event: InteractSSEEvent) => void,
  onDone: () => void,
  onError: (err: string) => void,
  flowContext?: { flow_id: string; node_id: string },
  images?: ImageData[],
): AbortController {
  // We reuse AbortController for API compatibility with useAgentChat
  const controller = new AbortController();
  let aborted = false;
  let unlisten: UnlistenFn | null = null;

  controller.signal.addEventListener("abort", () => {
    aborted = true;
    unlisten?.();
    log("info", "Agent chat stream aborted");
    onDone();
  });

  const eventChannel = `chat-event-${sessionId || "pending"}`;
  log("http", `invoke agent_chat agentId=${agentId} session=${sessionId ?? "active"}${images?.length ? `, ${images.length} images` : ""}`);
  // Start listening BEFORE invoking the command to avoid missing events
  listen<string>(eventChannel, (event) => {
    if (aborted) return;
    try {
      // The backend sends SSE-style JSON payloads
      const parsed = JSON.parse(event.payload);
      const eventType = parsed.type || parsed.event || "message";
      const data = parsed.data != null
        ? (typeof parsed.data === "string" ? parsed.data : JSON.stringify(parsed.data))
        : event.payload;
      onEvent({ type: eventType, data });
    } catch {
      // Fallback: treat as raw text message
      onEvent({ type: "message", data: event.payload });
    }
  }).then((fn) => {
    unlisten = fn;
    if (aborted) { fn(); return; }

    // Now invoke the chat command
    invoke<{ session_id: string }>("agent_chat", {
      agentId,
      prompt,
      sessionId,
      flowId: flowContext?.flow_id,
      nodeId: flowContext?.node_id,
      images: images && images.length > 0 ? images : undefined,
    })
      .then(() => {
        // Command completed — the stream is done
        if (!aborted) {
          unlisten?.();
          onDone();
        }
      })
      .catch((err) => {
        if (!aborted) {
          unlisten?.();
          onError(String(err));
        }
      });
  });

  return controller;
}

/**
 * Reconnect to an in-flight agent chat stream via Tauri IPC.
 * Invokes the `reconnect_agent_chat` command and listens for streamed events.
 * Returns an AbortController to cancel the stream.
 */
export function reconnectAgentChat(
  agentId: string,
  sessionId: string,
  onEvent: (event: InteractSSEEvent) => void,
  onDone: () => void,
  onError: (err: string) => void
): AbortController {
  const controller = new AbortController();
  let aborted = false;
  let unlisten: UnlistenFn | null = null;

  controller.signal.addEventListener("abort", () => {
    aborted = true;
    unlisten?.();
    log("info", "Agent chat reconnect stream aborted");
    onDone();
  });

  const eventChannel = `chat-event-${sessionId}`;
  log("http", `invoke reconnect_agent_chat agentId=${agentId} sessionId=${sessionId}`);
  console.log(`[RECONNECT-DEBUG] interactStream: listening on ${eventChannel}`);

  listen<string>(eventChannel, (event) => {
    if (aborted) return;
    try {
      const parsed = JSON.parse(event.payload);
      const eventType = parsed.type || parsed.event || "message";
      const data = parsed.data != null
        ? (typeof parsed.data === "string" ? parsed.data : JSON.stringify(parsed.data))
        : event.payload;
      onEvent({ type: eventType, data });
    } catch {
      onEvent({ type: "message", data: event.payload });
    }
  }).then((fn) => {
    unlisten = fn;
    if (aborted) { fn(); return; }

    invoke("reconnect_agent_chat", { agentId, sessionId })
      .then(() => {
        if (!aborted) {
          unlisten?.();
          onDone();
        }
      })
      .catch((err) => {
        if (!aborted) {
          unlisten?.();
          onError(String(err));
        }
      });
  });

  return controller;
}
