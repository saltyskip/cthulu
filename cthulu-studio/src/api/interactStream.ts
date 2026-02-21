import { getServerUrl } from "./client";
import { log } from "./logger";

export interface InteractSSEEvent {
  type: string;
  data: string;
}

/**
 * Start an interactive Claude session for a flow.
 * Uses fetch() + ReadableStream because EventSource only supports GET.
 * Returns an AbortController to cancel the stream.
 */
export function startInteract(
  flowId: string,
  prompt: string,
  sessionId: string | null,
  onEvent: (event: InteractSSEEvent) => void,
  onDone: () => void,
  onError: (err: string) => void
): AbortController {
  const controller = new AbortController();
  const url = `${getServerUrl()}/api/flows/${flowId}/interact`;

  log("http", `POST /flows/${flowId}/interact (stream, session=${sessionId ?? "active"})`);

  const body: Record<string, string> = { prompt };
  if (sessionId) {
    body.session_id = sessionId;
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

        // Parse SSE lines
        const lines = buffer.split("\n");
        buffer = lines.pop() || ""; // Keep incomplete line in buffer

        for (const line of lines) {
          if (line.startsWith("event: ")) {
            currentEventType = line.slice(7).trim();
          } else if (line.startsWith("data: ")) {
            const data = line.slice(6);
            onEvent({ type: currentEventType, data });
            currentEventType = "message"; // Reset for next event
          } else if (line.startsWith(": ")) {
            // SSE comment (keep-alive), ignore
          } else if (line === "") {
            // Empty line = end of event block, already handled
          }
        }
      }

      onDone();
    })
    .catch((err) => {
      if (err.name === "AbortError") {
        log("info", "Interact stream aborted");
        onDone();
      } else {
        onError(err.message || "Stream error");
      }
    });

  return controller;
}

/**
 * Start an interactive Claude session for a specific node (node-level chat).
 * Same SSE protocol as startInteract, but hits the node-scoped endpoint.
 */
export function startNodeInteract(
  flowId: string,
  nodeId: string,
  prompt: string,
  sessionId: string | null,
  onEvent: (event: InteractSSEEvent) => void,
  onDone: () => void,
  onError: (err: string) => void
): AbortController {
  const controller = new AbortController();
  const url = `${getServerUrl()}/api/flows/${flowId}/nodes/${nodeId}/interact`;

  log("http", `POST /flows/${flowId}/nodes/${nodeId}/interact (stream, session=${sessionId ?? "active"})`);

  const body: Record<string, string> = { prompt };
  if (sessionId) {
    body.session_id = sessionId;
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
        log("info", "Node interact stream aborted");
        onDone();
      } else {
        onError(err.message || "Stream error");
      }
    });

  return controller;
}
