import { getServerUrl } from "./client";
import { log } from "./logger";

export interface InteractSSEEvent {
  type: string;
  data: string;
}

/**
 * Start an agent chat session via SSE.
 * Hits POST /api/agents/{agentId}/chat.
 * Returns an AbortController to cancel the stream.
 */
export function startAgentChat(
  agentId: string,
  prompt: string,
  sessionId: string | null,
  onEvent: (event: InteractSSEEvent) => void,
  onDone: () => void,
  onError: (err: string) => void,
  flowContext?: { flow_id: string; node_id: string }
): AbortController {
  const controller = new AbortController();
  const url = `${getServerUrl()}/api/agents/${agentId}/chat`;

  log("http", `POST /agents/${agentId}/chat (stream, session=${sessionId ?? "active"})`);

  const body: Record<string, string | undefined> = { prompt };
  if (sessionId) {
    body.session_id = sessionId;
  }
  if (flowContext) {
    body.flow_id = flowContext.flow_id;
    body.node_id = flowContext.node_id;
  }

  fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: controller.signal,
  })
    .then(async (response) => {
      if (!response.ok) {
        const text = await response.text();
        onError(`HTTP ${response.status}: ${text}`);
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        onError("No response body");
        return;
      }

      const decoder = new TextDecoder();
      let buffer = "";
      let currentEventType = "message";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          if (line.startsWith("event: ")) {
            currentEventType = line.slice(7).trim();
          } else if (line.startsWith("data: ")) {
            const data = line.slice(6);
            onEvent({ type: currentEventType, data });
            currentEventType = "message";
          } else if (line.startsWith(": ")) {
            // SSE comment (keep-alive), ignore
          }
        }
      }

      onDone();
    })
    .catch((err) => {
      if (err.name === "AbortError") {
        log("info", "Agent chat stream aborted");
        onDone();
      } else {
        onError(err.message || "Stream error");
      }
    });

  return controller;
}

/**
 * Reconnect to an in-flight agent chat stream via SSE.
 * Hits GET /api/agents/{agentId}/sessions/{sessionId}/chat/stream.
 * Replays buffered events then subscribes to live broadcast.
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
  const url = `${getServerUrl()}/api/agents/${agentId}/sessions/${sessionId}/chat/stream`;

  log("http", `GET /agents/${agentId}/sessions/${sessionId}/chat/stream (reconnect)`);
  console.log(`[RECONNECT-DEBUG] interactStream: fetching ${url}`);

  fetch(url, {
    method: "GET",
    signal: controller.signal,
  })
    .then(async (response) => {
      console.log(`[RECONNECT-DEBUG] interactStream: response status=${response.status}`);
      if (!response.ok) {
        const text = await response.text();
        console.error(`[RECONNECT-DEBUG] interactStream: HTTP error: ${response.status} ${text}`);
        onError(`HTTP ${response.status}: ${text}`);
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        onError("No response body");
        return;
      }

      const decoder = new TextDecoder();
      let buffer = "";
      let currentEventType = "message";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          if (line.startsWith("event: ")) {
            currentEventType = line.slice(7).trim();
          } else if (line.startsWith("data: ")) {
            const data = line.slice(6);
            onEvent({ type: currentEventType, data });
            currentEventType = "message";
          } else if (line.startsWith(": ")) {
            // SSE comment (keep-alive), ignore
          }
        }
      }

      onDone();
    })
    .catch((err) => {
      if (err.name === "AbortError") {
        log("info", "Agent chat reconnect stream aborted");
        onDone();
      } else {
        onError(err.message || "Reconnect stream error");
      }
    });

  return controller;
}
