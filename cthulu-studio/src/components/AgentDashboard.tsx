import { useState, useEffect, useMemo } from "react";
import * as api from "../api/client";
import type { Agent, HeartbeatRun } from "../types/flow";
import { StatusBadge } from "./StatusBadge";
import { ChartCard, RunActivityChart, SuccessRateChart } from "./ActivityCharts";
import AgentTerminal from "./AgentTerminal";

interface AgentDashboardProps {
  agent: Agent;
  sessionId: string;
}

function formatRelativeTime(iso: string): string {
  const ms = Date.now() - new Date(iso).getTime();
  if (ms < 60_000) return "just now";
  const min = Math.floor(ms / 60_000);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  return `${Math.floor(hr / 24)}d ago`;
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${Math.round(secs)}s`;
  const m = Math.floor(secs / 60);
  const s = Math.round(secs % 60);
  return s > 0 ? `${m}m ${s}s` : `${m}m`;
}

export function AgentDashboard({ agent, sessionId }: AgentDashboardProps) {
  const [runs, setRuns] = useState<HeartbeatRun[]>([]);

  useEffect(() => {
    api.listHeartbeatRuns(agent.id).then(setRuns).catch(() => {});
  }, [agent.id]);

  const latestRun = runs[0] ?? null;
  const isLive = latestRun?.status === "running";

  // Cost totals
  const totalCost = useMemo(() => runs.reduce((sum, r) => sum + r.cost_usd, 0), [runs]);
  const totalInput = useMemo(() => runs.reduce((sum, r) => sum + (r.usage?.input_tokens ?? 0), 0), [runs]);
  const totalOutput = useMemo(() => runs.reduce((sum, r) => sum + (r.usage?.output_tokens ?? 0), 0), [runs]);
  const totalCached = useMemo(() => runs.reduce((sum, r) => sum + (r.usage?.cached_input_tokens ?? 0), 0), [runs]);

  return (
    <div className="agent-dashboard">
      {/* Latest Run Card */}
      {latestRun && (
        <div className={`dashboard-latest-run${isLive ? " dashboard-latest-run-live" : ""}`}>
          <div className="dashboard-latest-run-header">
            <span className="dashboard-latest-run-label">Latest Run</span>
            <div className="dashboard-latest-run-meta">
              <StatusBadge status={latestRun.status} />
              <span className="dashboard-run-id">{latestRun.id.slice(0, 8)}</span>
              <span className="dashboard-run-time">{formatRelativeTime(latestRun.started_at)}</span>
            </div>
          </div>
          {latestRun.error && (
            <p className="dashboard-run-error">{latestRun.error}</p>
          )}
          <div className="dashboard-run-stats">
            <span>Duration: {formatDuration(latestRun.duration_secs)}</span>
            <span>Cost: ${latestRun.cost_usd.toFixed(4)}</span>
            {latestRun.model && <span>Model: {latestRun.model}</span>}
          </div>
        </div>
      )}

      {/* Charts Grid */}
      <div className="dashboard-charts-grid">
        <ChartCard title="Run Activity" subtitle="Last 14 days">
          <RunActivityChart runs={runs} />
        </ChartCard>
        <ChartCard title="Success Rate" subtitle="Last 14 days">
          <SuccessRateChart runs={runs} />
        </ChartCard>
      </div>

      {/* Terminal */}
      <div>
        <h3 className="dashboard-section-title">Terminal</h3>
        <div className="dashboard-terminal-container">
          <AgentTerminal
            agentId={agent.id}
            sessionId={sessionId}
          />
        </div>
      </div>

      {/* Token Usage / Costs */}
      {runs.length > 0 && (
        <div>
          <h3 className="dashboard-section-title">Token Usage</h3>
          <div className="dashboard-token-grid">
            <div className="dashboard-token-card">
              <span className="dashboard-token-label">Input Tokens</span>
              <span className="dashboard-token-value">{totalInput.toLocaleString()}</span>
            </div>
            <div className="dashboard-token-card">
              <span className="dashboard-token-label">Output Tokens</span>
              <span className="dashboard-token-value">{totalOutput.toLocaleString()}</span>
            </div>
            <div className="dashboard-token-card">
              <span className="dashboard-token-label">Cached</span>
              <span className="dashboard-token-value">{totalCached.toLocaleString()}</span>
            </div>
            <div className="dashboard-token-card">
              <span className="dashboard-token-label">Total Cost</span>
              <span className="dashboard-token-value">${totalCost.toFixed(4)}</span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
