import type { HeartbeatRun } from "../types/flow";

/** Generate last 14 days as ISO date strings */
function getLast14Days(): string[] {
  return Array.from({ length: 14 }, (_, i) => {
    const d = new Date();
    d.setDate(d.getDate() - (13 - i));
    return d.toISOString().slice(0, 10);
  });
}

function formatDayLabel(dateStr: string): string {
  const d = new Date(dateStr + "T12:00:00");
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

function DateLabels({ days }: { days: string[] }) {
  return (
    <div className="chart-date-labels">
      {days.map((day, i) => (
        <div key={day} className="chart-date-label">
          {(i === 0 || i === 6 || i === 13) ? (
            <span>{formatDayLabel(day)}</span>
          ) : null}
        </div>
      ))}
    </div>
  );
}

export function ChartCard({ title, subtitle, children }: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="chart-card">
      <div className="chart-card-header">
        <h3>{title}</h3>
        {subtitle && <span>{subtitle}</span>}
      </div>
      {children}
    </div>
  );
}

/** Stacked bar chart showing run counts per day (succeeded/failed/other) */
export function RunActivityChart({ runs }: { runs: HeartbeatRun[] }) {
  const days = getLast14Days();
  const grouped = new Map<string, { succeeded: number; failed: number; other: number }>();
  for (const day of days) grouped.set(day, { succeeded: 0, failed: 0, other: 0 });

  for (const run of runs) {
    const day = new Date(run.started_at).toISOString().slice(0, 10);
    const entry = grouped.get(day);
    if (!entry) continue;
    if (run.status === "succeeded") entry.succeeded++;
    else if (run.status === "failed" || run.status === "timed_out") entry.failed++;
    else entry.other++;
  }

  const maxValue = Math.max(
    ...Array.from(grouped.values()).map(v => v.succeeded + v.failed + v.other), 1);
  const hasData = Array.from(grouped.values()).some(v => v.succeeded + v.failed + v.other > 0);

  if (!hasData) return <p className="chart-empty">No runs yet</p>;

  return (
    <div>
      <div className="chart-bars">
        {days.map(day => {
          const entry = grouped.get(day)!;
          const total = entry.succeeded + entry.failed + entry.other;
          const heightPct = (total / maxValue) * 100;
          return (
            <div key={day} className="chart-bar-col" title={`${day}: ${total} runs`}>
              {total > 0 ? (
                <div className="chart-bar-stack" style={{ height: `${heightPct}%`, minHeight: 2 }}>
                  {entry.succeeded > 0 && <div className="chart-bar-seg chart-bar-green" style={{ flex: entry.succeeded }} />}
                  {entry.failed > 0 && <div className="chart-bar-seg chart-bar-red" style={{ flex: entry.failed }} />}
                  {entry.other > 0 && <div className="chart-bar-seg chart-bar-muted" style={{ flex: entry.other }} />}
                </div>
              ) : (
                <div className="chart-bar-empty" />
              )}
            </div>
          );
        })}
      </div>
      <DateLabels days={days} />
    </div>
  );
}

/** Success rate bar chart — colored by rate (green >= 80%, yellow >= 50%, red < 50%) */
export function SuccessRateChart({ runs }: { runs: HeartbeatRun[] }) {
  const days = getLast14Days();
  const grouped = new Map<string, { total: number; succeeded: number }>();
  for (const day of days) grouped.set(day, { total: 0, succeeded: 0 });

  for (const run of runs) {
    const day = new Date(run.started_at).toISOString().slice(0, 10);
    const entry = grouped.get(day);
    if (!entry) continue;
    entry.total++;
    if (run.status === "succeeded") entry.succeeded++;
  }

  const hasData = Array.from(grouped.values()).some(v => v.total > 0);
  if (!hasData) return <p className="chart-empty">No runs yet</p>;

  return (
    <div>
      <div className="chart-bars">
        {days.map(day => {
          const entry = grouped.get(day)!;
          if (entry.total === 0) {
            return <div key={day} className="chart-bar-col"><div className="chart-bar-empty" /></div>;
          }
          const rate = entry.succeeded / entry.total;
          const colorClass = rate >= 0.8 ? "chart-bar-green" : rate >= 0.5 ? "chart-bar-yellow" : "chart-bar-red";
          return (
            <div key={day} className="chart-bar-col" title={`${day}: ${Math.round(rate * 100)}%`}>
              <div className="chart-bar-stack" style={{ height: `${rate * 100}%`, minHeight: 2 }}>
                <div className={`chart-bar-seg ${colorClass}`} style={{ flex: 1 }} />
              </div>
            </div>
          );
        })}
      </div>
      <DateLabels days={days} />
    </div>
  );
}
