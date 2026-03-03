import { useState, useEffect, useCallback, useRef } from "react";
import { getMcpStatus, buildMcp, registerMcp, type McpStatus } from "../api/client";

// ── Status badge ──────────────────────────────────────────────────────────────

function StatusBadge({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className={`mcp-badge ${ok ? "mcp-badge-ok" : "mcp-badge-fail"}`}>
      {ok ? "✓" : "✗"} {label}
    </span>
  );
}

// ── Copy button ───────────────────────────────────────────────────────────────

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      className="mcp-copy-btn"
      onClick={() => {
        navigator.clipboard.writeText(text).then(
          () => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          },
          () => { /* clipboard unavailable — ignore */ }
        );
      }}
    >
      {copied ? "Copied!" : "Copy"}
    </button>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export default function McpSetupView() {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [buildLog, setBuildLog] = useState<{ text: string; isError: boolean }[]>([]);
  const [building, setBuilding] = useState(false);
  const [registering, setRegistering] = useState(false);
  const [registerResult, setRegisterResult] = useState<{ ok: boolean; message?: string; error?: string } | null>(null);
  const logEndRef = useRef<HTMLDivElement>(null);

  const scrollToEnd = useCallback(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      setStatus(await getMcpStatus());
    } catch {
      setStatus(null);
    } finally {
      setLoading(false);
    }
  }, []);

  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { refresh(); }, []);

  async function handleBuild() {
    setBuilding(true);
    setBuildLog([]);
    setRegisterResult(null);
    try {
      await buildMcp((line, isError, isDone) => {
        if (!isDone) {
          setBuildLog((prev) => {
            const next = [...prev, { text: line, isError }];
            // Scroll after state update
            requestAnimationFrame(scrollToEnd);
            return next;
          });
        }
      });
      await refresh();
    } catch (e) {
      setBuildLog((prev) => [
        ...prev,
        { text: `Error: ${(e as Error).message}`, isError: true },
      ]);
    } finally {
      setBuilding(false);
    }
  }

  async function handleRegister() {
    setRegistering(true);
    setRegisterResult(null);
    try {
      const result = await registerMcp();
      setRegisterResult(result);
      if (result.ok) await refresh();
    } catch (e) {
      setRegisterResult({ ok: false, error: (e as Error).message });
    } finally {
      setRegistering(false);
    }
  }

  const manualBuildCmd = "cargo build --release --bin cthulu-mcp";
  const manualRegisterCmd = "make setup-mcp";
  const manualRestartHint = "Restart Claude Desktop after registering.";

  return (
    <div className="mcp-setup-view">
      <div className="mcp-setup-header">
        <h2>MCP Server Setup</h2>
        <button className="ghost mcp-refresh-btn" onClick={refresh} disabled={loading} title="Refresh status">
          {loading ? "…" : "↺"}
        </button>
      </div>

      <p className="mcp-setup-desc">
        <strong>cthulu-mcp</strong> exposes Cthulu's flows, agents and web search to Claude Desktop
        as 30 MCP tools. Set it up once and use natural language to manage your workflows from
        Claude Desktop.
      </p>

      {/* Status checklist */}
      <section className="mcp-section">
        <h3>Status</h3>
        {loading ? (
          <p className="mcp-loading">Checking…</p>
        ) : status ? (
          <div className="mcp-status-grid">
            <StatusBadge ok={status.binary_exists} label="Binary built" />
            <StatusBadge ok={status.launcher_exists} label="Launcher script" />
            <StatusBadge ok={status.registered_in_claude_desktop} label="Registered in Claude Desktop" />
            <StatusBadge ok={status.searxng_ok} label="SearXNG running" />
          </div>
        ) : (
          <p className="mcp-error">Could not reach backend — is Cthulu running?</p>
        )}
      </section>

      {/* Step 1: Build */}
      <section className="mcp-section">
        <h3>Step 1 — Build binary</h3>
        <p className="mcp-step-desc">
          Compiles <code>cthulu-mcp</code> in release mode. Takes ~30–90 s on first build.
        </p>
        <div className="mcp-action-row">
          <button
            className="mcp-action-btn"
            onClick={handleBuild}
            disabled={building || loading}
          >
            {building ? "Building…" : status?.binary_exists ? "Rebuild binary" : "Build binary"}
          </button>
          <span className="mcp-manual-label">or run manually:</span>
          <div className="mcp-code-row">
            <code className="mcp-code">{manualBuildCmd}</code>
            <CopyButton text={manualBuildCmd} />
          </div>
        </div>

        {buildLog.length > 0 && (
          <div className="mcp-build-log">
            {buildLog.map((entry, i) => (
              <div key={i} className={`mcp-log-line${entry.isError ? " mcp-log-error" : ""}`}>
                {entry.text}
              </div>
            ))}
            <div ref={logEndRef} />
          </div>
        )}
      </section>

      {/* Step 2: Register */}
      <section className="mcp-section">
        <h3>Step 2 — Register in Claude Desktop</h3>
        <p className="mcp-step-desc">
          Writes the <code>cthulu</code> entry into{" "}
          <code>~/Library/Application Support/Claude/claude_desktop_config.json</code>.
          Existing entries are preserved.
        </p>
        <div className="mcp-action-row">
          <button
            className="mcp-action-btn"
            onClick={handleRegister}
            disabled={registering || loading || !status?.binary_exists}
            title={!status?.binary_exists ? "Build the binary first" : undefined}
          >
            {registering
              ? "Registering…"
              : status?.registered_in_claude_desktop
              ? "Re-register"
              : "Register"}
          </button>
          <span className="mcp-manual-label">or run manually:</span>
          <div className="mcp-code-row">
            <code className="mcp-code">{manualRegisterCmd}</code>
            <CopyButton text={manualRegisterCmd} />
          </div>
        </div>

        {registerResult && (
          <div className={`mcp-result ${registerResult.ok ? "mcp-result-ok" : "mcp-result-fail"}`}>
            {registerResult.ok
              ? `${registerResult.message ?? "Registered!"}`
              : `Error: ${registerResult.error ?? "Unknown error"}`}
          </div>
        )}
      </section>

      {/* Step 3: Restart Claude Desktop */}
      <section className="mcp-section">
        <h3>Step 3 — Restart Claude Desktop</h3>
        <p className="mcp-step-desc">
          {manualRestartHint} The <strong>cthulu</strong> server will appear in the tool panel
          (hammer icon).
        </p>
        {status?.registered_in_claude_desktop && (
          <div className="mcp-hint-box">
            Already registered — just restart Claude Desktop if the server isn't showing.
          </div>
        )}
      </section>

      {/* SearXNG */}
      <section className="mcp-section">
        <h3>SearXNG (optional — for unlimited web search)</h3>
        <p className="mcp-step-desc">
          Without SearXNG, <code>web_search</code> falls back to DuckDuckGo (rate-limited to 30
          req/min). Start SearXNG with Docker:
        </p>
        <div className="mcp-code-row">
          <code className="mcp-code">make searxng-start</code>
          <CopyButton text="make searxng-start" />
        </div>
        {status && (
          <p className="mcp-status-line">
            Status:{" "}
            <span className={status.searxng_ok ? "mcp-ok-text" : "mcp-warn-text"}>
              {status.searxng_ok ? `Running at ${status.searxng_url}` : "Not running (DDG fallback active)"}
            </span>
          </p>
        )}
      </section>

      {/* Quick-test */}
      <section className="mcp-section">
        <h3>Quick test (after Claude Desktop restart)</h3>
        <p className="mcp-step-desc">
          In Claude Desktop, type:
        </p>
        <div className="mcp-code-row">
          <code className="mcp-code">Use get_token_status and tell me if my Claude token is valid.</code>
          <CopyButton text="Use get_token_status and tell me if my Claude token is valid." />
        </div>
        <p className="mcp-step-desc mcp-step-desc-mt">
          If the <strong>cthulu</strong> server appears in the tool panel, setup is complete.
        </p>
      </section>
    </div>
  );
}
