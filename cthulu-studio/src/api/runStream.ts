import type { RunEvent } from "../types/flow";
import { getServerUrl } from "./client";

export function subscribeToRuns(
  flowId: string,
  onEvent: (event: RunEvent) => void,
  onError?: (err: Event) => void
): () => void {
  const url = `${getServerUrl()}/api/flows/${flowId}/runs/live`;
  const es = new EventSource(url);

  const eventTypes = [
    "run_started",
    "node_started",
    "node_completed",
    "node_failed",
    "run_completed",
    "run_failed",
    "log",
  ];

  for (const type of eventTypes) {
    es.addEventListener(type, (e: MessageEvent) => {
      try {
        const data: RunEvent = JSON.parse(e.data);
        onEvent(data);
      } catch {
        // ignore parse errors
      }
    });
  }

  if (onError) {
    es.onerror = onError;
  }

  return () => es.close();
}
