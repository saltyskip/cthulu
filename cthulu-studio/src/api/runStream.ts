import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { RunEvent } from "../types/flow";

export function subscribeToRuns(
  flowId: string,
  onEvent: (event: RunEvent) => void,
  onError?: (err: Event) => void
): () => void {
  let unlisten: UnlistenFn | null = null;
  let cleaned = false;

  listen<RunEvent>("run-event", (event) => {
    if (cleaned) return;
    try {
      // Filter by flowId since the Tauri bridge sends ALL run events
      const payload = event.payload;
      if (payload.flow_id === flowId) {
        onEvent(payload);
      }
    } catch (e) {
      if (onError) {
        onError(e as Event);
      }
    }
  }).then((fn) => {
    unlisten = fn;
    if (cleaned) fn(); // was cleaned up before listener registered
  });

  return () => {
    cleaned = true;
    unlisten?.();
  };
}
