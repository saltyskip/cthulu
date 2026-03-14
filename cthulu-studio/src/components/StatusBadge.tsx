import { statusBadge, statusBadgeDefault } from "../lib/status-colors";

export function StatusBadge({ status }: { status: string }) {
  const cls = statusBadge[status] ?? statusBadgeDefault;
  return (
    <span className={`sb-badge ${cls}`}>
      {status.replace(/_/g, " ")}
    </span>
  );
}
