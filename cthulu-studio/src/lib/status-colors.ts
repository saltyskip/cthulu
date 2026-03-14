/**
 * Centralized status & run color definitions.
 * Every component rendering a status indicator imports from here.
 */

// Agent status (derived from heartbeat_enabled + busy state)
export type AgentStatus = "active" | "idle" | "paused" | "busy" | "error";

// Badge CSS class names (defined in styles.css)
export const statusBadge: Record<string, string> = {
  // Agent statuses
  active: "sb-badge-green",
  idle: "sb-badge-yellow",
  busy: "sb-badge-cyan",
  paused: "sb-badge-orange",
  error: "sb-badge-red",

  // Run statuses
  pending: "sb-badge-yellow",
  running: "sb-badge-cyan",
  succeeded: "sb-badge-green",
  failed: "sb-badge-red",
  timed_out: "sb-badge-orange",
  cancelled: "sb-badge-muted",
};
export const statusBadgeDefault = "sb-badge-muted";

// Small indicator dots (solid background)
export const agentStatusDot: Record<string, string> = {
  active: "var(--success)",
  idle: "var(--warning)",
  busy: "var(--accent)",
  paused: "var(--warning)",
  error: "var(--danger)",
};
export const agentStatusDotDefault = "var(--text-secondary)";

// Run status dot colors
export const runStatusDot: Record<string, string> = {
  pending: "var(--text-secondary)",
  running: "var(--accent)",
  succeeded: "var(--success)",
  failed: "var(--danger)",
  timed_out: "var(--warning)",
  cancelled: "var(--text-secondary)",
};

/**
 * Derive agent status from available data.
 */
export function deriveAgentStatus(
  heartbeatEnabled: boolean,
  hasBusySession: boolean,
  lastRunFailed: boolean,
): AgentStatus {
  if (hasBusySession) return "busy";
  if (lastRunFailed) return "error";
  if (!heartbeatEnabled) return "paused";
  return "active";
}
