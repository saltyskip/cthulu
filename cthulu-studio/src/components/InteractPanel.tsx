import { useState, useEffect, useRef, useCallback } from "react";
import * as api from "../api/client";
import { startInteract } from "../api/interactStream";
import { log } from "../api/logger";
import type {
  SessionInfo,
  OutputLine,
  SavedPrompt,
  InteractSessionInfo,
} from "../types/flow";

// Per-tab UI state (prompt text, output, running flag)
export interface SessionTabState {
  prompt: string;
  outputLines: OutputLine[];
  running: boolean;
}

// Full panel state persisted in App.tsx sessionsRef
export interface InteractPanelState {
  session: SessionInfo | null;
  tabs: Record<string, SessionTabState>; // session_id -> tab state
  activeSessionId: string | null;
  sessionList: InteractSessionInfo[];
}

interface InteractPanelProps {
  flowId: string;
  onClose: () => void;
  initialState: InteractPanelState | null;
  onStateChange: (state: InteractPanelState) => void;
}

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
    }) + " " + d.toLocaleTimeString("en-US", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  } catch {
    return iso;
  }
}

export default function InteractPanel({
  flowId,
  onClose,
  initialState,
  onStateChange,
}: InteractPanelProps) {
  const [session, setSession] = useState<SessionInfo | null>(
    initialState?.session ?? null
  );
  const [sessionList, setSessionList] = useState<InteractSessionInfo[]>(
    initialState?.sessionList ?? []
  );
  const [activeSessionId, setActiveSessionId] = useState<string | null>(
    initialState?.activeSessionId ?? null
  );
  const [tabs, setTabs] = useState<Record<string, SessionTabState>>(
    initialState?.tabs ?? {}
  );
  const [loading, setLoading] = useState(false);
  const [showHistory, setShowHistory] = useState(false);
  const [savedPrompts, setSavedPrompts] = useState<SavedPrompt[]>([]);
  const [showSuggestions, setShowSuggestions] = useState(true);

  const abortRef = useRef<AbortController | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);
  const historyRef = useRef<HTMLDivElement>(null);

  // Current tab state (derived)
  const currentTab: SessionTabState = activeSessionId
    ? tabs[activeSessionId] ?? { prompt: "", outputLines: [], running: false }
    : { prompt: "", outputLines: [], running: false };

  // Helper to update the current tab's state
  const updateTab = useCallback(
    (
      sessionId: string,
      updater: (prev: SessionTabState) => SessionTabState
    ) => {
      setTabs((prev) => ({
        ...prev,
        [sessionId]: updater(
          prev[sessionId] ?? { prompt: "", outputLines: [], running: false }
        ),
      }));
    },
    []
  );

  // State ref for unmount save
  const stateRef = useRef<InteractPanelState>({
    session,
    tabs,
    activeSessionId,
    sessionList,
  });
  stateRef.current = { session, tabs, activeSessionId, sessionList };

  // Save to parent on unmount
  useEffect(() => {
    return () => {
      onStateChange(stateRef.current);
    };
  }, [onStateChange]);

  // Auto-scroll output
  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [currentTab.outputLines]);

  // Close history dropdown on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        historyRef.current &&
        !historyRef.current.contains(e.target as Node)
      ) {
        setShowHistory(false);
      }
    };
    if (showHistory) {
      document.addEventListener("mousedown", handler);
    }
    return () => document.removeEventListener("mousedown", handler);
  }, [showHistory]);

  // Load session list + session info on mount
  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    const init = async () => {
      try {
        // Load flow session metadata
        const sess = await api.getSession(flowId);
        if (cancelled) return;
        setSession(sess);

        // Load interact sessions list
        const info = await api.listInteractSessions(flowId);
        if (cancelled) return;

        if (info.sessions.length > 0) {
          setSessionList(info.sessions);
          // If we have a saved activeSessionId that exists in the list, keep it
          const savedActive = initialState?.activeSessionId;
          const activeExists =
            savedActive &&
            info.sessions.some((s) => s.session_id === savedActive);
          const targetId = activeExists
            ? savedActive!
            : info.active_session || info.sessions[0].session_id;
          setActiveSessionId(targetId);

          // Initialize tab if no saved state
          if (!tabs[targetId]) {
            setTabs((prev) => ({
              ...prev,
              [targetId]: {
                prompt: sess.prompt || "",
                outputLines: [
                  { type: "system", text: `Flow: ${sess.flow_name}` },
                  {
                    type: "system",
                    text: `Working dir: ${sess.working_dir}`,
                  },
                  {
                    type: "system",
                    text: "Send a message to resume this session.",
                  },
                ],
                running: false,
              },
            }));
          }
        } else {
          // No sessions yet — create the first one
          const newSess = await api.newInteractSession(flowId);
          if (cancelled) return;
          const newInfo: InteractSessionInfo = {
            session_id: newSess.session_id,
            summary: "",
            message_count: 0,
            total_cost: 0,
            created_at: newSess.created_at,
            busy: false,
          };
          setSessionList([newInfo]);
          setActiveSessionId(newSess.session_id);
          setTabs((prev) => ({
            ...prev,
            [newSess.session_id]: {
              prompt: sess.prompt || "",
              outputLines: [
                { type: "system", text: `Flow: ${sess.flow_name}` },
                { type: "system", text: `Working dir: ${sess.working_dir}` },
                {
                  type: "system",
                  text: sess.prompt
                    ? "Prompt pre-filled below. Edit and press Send."
                    : "Type your message to start a conversation.",
                },
              ],
              running: false,
            },
          }));
        }
      } catch (err) {
        if (!cancelled) {
          const errorTabId = "_error";
          setActiveSessionId(errorTabId);
          setTabs((prev) => ({
            ...prev,
            [errorTabId]: {
              prompt: "",
              outputLines: [
                {
                  type: "error",
                  text: `Failed to load: ${(err as Error).message}`,
                },
              ],
              running: false,
            },
          }));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    init();
    return () => {
      cancelled = true;
    };
  }, [flowId]); // eslint-disable-line react-hooks/exhaustive-deps

  // Load saved prompts
  useEffect(() => {
    api.listPrompts().then(setSavedPrompts).catch(() => {});
  }, []);

  const handleSend = useCallback(() => {
    if (!activeSessionId || currentTab.running) return;
    const promptText = currentTab.prompt.trim();
    if (!promptText) return;

    const sid = activeSessionId;

    // Clear prompt and show what was sent
    updateTab(sid, (prev) => ({
      ...prev,
      prompt: "",
      running: true,
      outputLines: [
        ...prev.outputLines,
        {
          type: "system",
          text: `> ${promptText.length > 200 ? promptText.slice(0, 200) + "..." : promptText}`,
        },
      ],
    }));
    setShowSuggestions(false);

    log("info", `Sending message to flow ${flowId}, session ${sid}`);

    const controller = startInteract(
      flowId,
      promptText,
      sid,
      (event) => {
        try {
          const parsed = JSON.parse(event.data);
          switch (event.type) {
            case "system":
              updateTab(sid, (prev) => ({
                ...prev,
                outputLines: [
                  ...prev.outputLines,
                  {
                    type: "system",
                    text: parsed.message || "System event",
                  },
                ],
              }));
              break;
            case "text":
              updateTab(sid, (prev) => ({
                ...prev,
                outputLines: [
                  ...prev.outputLines,
                  { type: "text", text: parsed.text || "" },
                ],
              }));
              break;
            case "tool_use": {
              const inputPreview =
                (parsed.input || "").length > 300
                  ? parsed.input.slice(0, 300) + "..."
                  : parsed.input || "";
              updateTab(sid, (prev) => ({
                ...prev,
                outputLines: [
                  ...prev.outputLines,
                  {
                    type: "tool_use",
                    text: `${parsed.tool}: ${inputPreview}`,
                  },
                ],
              }));
              break;
            }
            case "tool_result": {
              const content =
                (parsed.content || "").length > 500
                  ? parsed.content.slice(0, 500) + "..."
                  : parsed.content || "";
              updateTab(sid, (prev) => ({
                ...prev,
                outputLines: [
                  ...prev.outputLines,
                  { type: "tool_result" as OutputLine["type"], text: content },
                ],
              }));
              break;
            }
            case "result":
              updateTab(sid, (prev) => ({
                ...prev,
                outputLines: [
                  ...prev.outputLines,
                  { type: "result", text: parsed.text || "" },
                  {
                    type: "cost",
                    text: `Cost: $${(parsed.cost || 0).toFixed(4)} | Turns: ${parsed.turns || 0}`,
                  },
                ],
              }));
              // Update session list with new message count
              setSessionList((prev) =>
                prev.map((s) =>
                  s.session_id === sid
                    ? {
                        ...s,
                        message_count: s.message_count + 1,
                        total_cost: s.total_cost + (parsed.cost || 0),
                        summary:
                          s.summary ||
                          (promptText.length > 80
                            ? promptText.slice(0, 77) + "..."
                            : promptText),
                      }
                    : s
                )
              );
              break;
            case "error":
              updateTab(sid, (prev) => ({
                ...prev,
                outputLines: [
                  ...prev.outputLines,
                  { type: "error", text: parsed.message || event.data },
                ],
              }));
              break;
          }
        } catch {
          updateTab(sid, (prev) => ({
            ...prev,
            outputLines: [
              ...prev.outputLines,
              { type: "text", text: event.data },
            ],
          }));
        }
      },
      () => {
        updateTab(sid, (prev) => ({ ...prev, running: false }));
        log("info", "Interact message completed");
      },
      (err) => {
        updateTab(sid, (prev) => ({
          ...prev,
          running: false,
          outputLines: [
            ...prev.outputLines,
            {
              type: err.includes("409") ? "system" : "error",
              text: err.includes("409")
                ? "Processing previous message... please wait."
                : `Stream error: ${err}`,
            },
          ],
        }));
      }
    );

    abortRef.current = controller;
  }, [activeSessionId, currentTab.prompt, currentTab.running, flowId, updateTab]);

  const handleStop = () => {
    abortRef.current?.abort();
    abortRef.current = null;
    if (activeSessionId) {
      api.stopInteract(flowId, activeSessionId).catch(() => {});
      updateTab(activeSessionId, (prev) => ({
        ...prev,
        running: false,
        outputLines: [
          ...prev.outputLines,
          { type: "system", text: "Session stopped by user" },
        ],
      }));
    }
  };

  const handleNewSession = async () => {
    if (currentTab.running) return;
    try {
      const result = await api.newInteractSession(flowId);
      log("info", `Created new session ${result.session_id} for flow ${flowId}`);
      const newInfo: InteractSessionInfo = {
        session_id: result.session_id,
        summary: "",
        message_count: 0,
        total_cost: 0,
        created_at: result.created_at,
        busy: false,
      };
      setSessionList((prev) => [...prev, newInfo]);
      setActiveSessionId(result.session_id);
      setTabs((prev) => ({
        ...prev,
        [result.session_id]: {
          prompt: session?.prompt || "",
          outputLines: [
            { type: "system", text: "New session created." },
            {
              type: "system",
              text: `Flow: ${session?.flow_name || ""}`,
            },
          ],
          running: false,
        },
      }));
      setShowHistory(false);
      setShowSuggestions(true);
    } catch (err) {
      log(
        "error",
        `Failed to create session: ${(err as Error).message}`
      );
    }
  };

  const handleSelectSession = (sessionId: string) => {
    if (sessionId === activeSessionId) {
      setShowHistory(false);
      return;
    }
    setActiveSessionId(sessionId);
    setShowHistory(false);
    // Initialize tab if not visited before
    if (!tabs[sessionId]) {
      const info = sessionList.find((s) => s.session_id === sessionId);
      setTabs((prev) => ({
        ...prev,
        [sessionId]: {
          prompt: "",
          outputLines: [
            {
              type: "system",
              text: info?.summary
                ? `Resuming: "${info.summary}"`
                : "Send a message to resume this session.",
            },
            {
              type: "system",
              text: `${info?.message_count ?? 0} previous messages`,
            },
          ],
          running: false,
        },
      }));
    }
  };

  const handleDeleteSession = async (
    e: React.MouseEvent,
    sessionId: string
  ) => {
    e.stopPropagation();
    if (sessionList.length <= 1) return;
    try {
      const result = await api.deleteInteractSession(flowId, sessionId);
      setSessionList((prev) => prev.filter((s) => s.session_id !== sessionId));
      setTabs((prev) => {
        const next = { ...prev };
        delete next[sessionId];
        return next;
      });
      if (activeSessionId === sessionId) {
        const updated = sessionList.filter((s) => s.session_id !== sessionId);
        const fallback = updated.length > 0 ? updated[0].session_id : undefined;
        const next = result.active_session && updated.some((s) => s.session_id === result.active_session)
          ? result.active_session
          : fallback;
        setActiveSessionId(next ?? null);
      }
    } catch (err) {
      log(
        "error",
        `Failed to delete session: ${(err as Error).message}`
      );
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSend();
    }
  };

  const handlePromptChange = (value: string) => {
    if (activeSessionId) {
      updateTab(activeSessionId, (prev) => ({ ...prev, prompt: value }));
    }
  };

  const handleUseSavedPrompt = (saved: SavedPrompt) => {
    if (activeSessionId) {
      updateTab(activeSessionId, (prev) => ({
        ...prev,
        prompt: saved.summary,
      }));
      setShowSuggestions(false);
    }
  };

  const lineClass = (type: OutputLine["type"]) => {
    switch (type) {
      case "system":
        return "interact-line interact-line-system";
      case "text":
        return "interact-line interact-line-text";
      case "tool_use":
        return "interact-line interact-line-tool";
      case "tool_result":
        return "interact-line interact-line-tool-result";
      case "result":
        return "interact-line interact-line-result";
      case "error":
        return "interact-line interact-line-error";
      case "cost":
        return "interact-line interact-line-cost";
      default:
        return "interact-line";
    }
  };

  const linePrefix = (type: OutputLine["type"]) => {
    switch (type) {
      case "tool_use":
        return "\u2699 ";
      case "tool_result":
        return "  \u2192 ";
      case "result":
        return "\u2713 ";
      case "error":
        return "\u2717 ";
      case "cost":
        return "$ ";
      default:
        return "";
    }
  };

  const activeInfo = sessionList.find(
    (s) => s.session_id === activeSessionId
  );
  const showPromptSuggestions =
    showSuggestions &&
    savedPrompts.length > 0 &&
    currentTab.outputLines.length <= 6 &&
    !currentTab.running;

  return (
    <div className="interact-panel console-panel">
      <div className="console-header interact-header">
        <span className="console-title interact-title">
          Interact{session ? `: ${session.flow_name}` : ""}
        </span>
        {activeInfo?.summary && (
          <span className="interact-session-subtitle" title={activeInfo.summary}>
            {activeInfo.summary}
          </span>
        )}
        <div style={{ flex: 1 }} />

        {/* History dropdown */}
        <div className="interact-history-wrapper" ref={historyRef}>
          <button
            className="ghost console-btn"
            onClick={() => setShowHistory((v) => !v)}
            title="Session history"
          >
            History ({sessionList.length})
          </button>
          {showHistory && (
            <div className="interact-history-dropdown">
              {sessionList.map((s) => (
                <div
                  key={s.session_id}
                  className={`interact-history-item${s.session_id === activeSessionId ? " active" : ""}`}
                  onClick={() => handleSelectSession(s.session_id)}
                >
                  <div className="interact-history-summary">
                    {s.summary || "New session"}
                  </div>
                  <div className="interact-history-meta">
                    {formatDate(s.created_at)} &middot; {s.message_count} msgs
                  </div>
                  {sessionList.length > 1 && (
                    <button
                      className="interact-history-close"
                      onClick={(e) => handleDeleteSession(e, s.session_id)}
                      title="Delete this session"
                    >
                      {"\u00d7"}
                    </button>
                  )}
                </div>
              ))}
              <button
                className="interact-history-new"
                onClick={handleNewSession}
              >
                + New Session
              </button>
            </div>
          )}
        </div>

        <button
          className="ghost console-btn"
          onClick={handleNewSession}
          disabled={currentTab.running}
          title="Create a new session"
        >
          + New
        </button>
        {currentTab.running && (
          <button className="ghost console-btn" onClick={handleStop}>
            Stop
          </button>
        )}
        <button className="ghost console-btn" onClick={onClose}>
          {"\u00d7"}
        </button>
      </div>

      <div className="interact-output" ref={outputRef}>
        {loading && (
          <div className="interact-line interact-line-system">
            Loading session...
          </div>
        )}
        {currentTab.outputLines.map((line, i) => (
          <div key={i} className={lineClass(line.type)}>
            {linePrefix(line.type)}
            {line.text}
          </div>
        ))}

        {showPromptSuggestions && (
          <div className="interact-suggestions">
            <span className="interact-suggestions-label">
              Start from a saved prompt:
            </span>
            {savedPrompts.map((p) => (
              <button
                key={p.id}
                className="interact-suggestion-chip"
                onClick={() => handleUseSavedPrompt(p)}
                title={p.summary.slice(0, 200)}
              >
                {p.title}
              </button>
            ))}
            <button
              className="interact-suggestion-dismiss"
              onClick={() => setShowSuggestions(false)}
            >
              dismiss
            </button>
          </div>
        )}
      </div>

      <div className="interact-prompt-area">
        <textarea
          value={currentTab.prompt}
          onChange={(e) => handlePromptChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={
            currentTab.running
              ? "Waiting for response..."
              : "Type your message... (Ctrl+Enter to send)"
          }
          disabled={currentTab.running || loading}
          rows={3}
        />
        <button
          className="primary interact-send"
          onClick={handleSend}
          disabled={currentTab.running || !currentTab.prompt.trim() || loading}
        >
          {currentTab.running ? "..." : "Send"}
        </button>
      </div>
    </div>
  );
}
