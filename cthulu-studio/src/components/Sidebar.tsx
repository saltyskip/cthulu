import { useState, useEffect, useCallback, useRef } from "react";
import anime from "animejs";
import { STUDIO_ASSISTANT_ID, type FlowSummary, type Flow, type NodeTypeSchema, type AgentSummary, type SavedPrompt, type ActiveView } from "../types/flow";
import { listAgents, createAgent, deleteAgent, listPrompts, savePrompt, deletePrompt as deletePromptApi, listAgentSessions, newAgentSession } from "../api/client";
import type { InteractSessionInfo } from "../api/client";
import { Switch } from "@/components/ui/switch";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";
import TemplateGallery from "./TemplateGallery";

interface SidebarProps {
  // Flow list
  flows: FlowSummary[];
  activeFlowId: string | null;
  onSelectFlow: (id: string) => void;
  onCreateFlow: () => void;
  onImportTemplate: (flow: Flow) => void;
  onToggleEnabled: (flowId: string) => void;
  // Agent + session selection
  selectedAgentId: string | null;
  selectedSessionId: string | null;
  onSelectSession: (agentId: string, sessionId: string) => void;
  agentListKey: number;
  onAgentCreated: (id: string) => void;
  // Prompts
  selectedPromptId: string | null;
  onSelectPrompt: (id: string) => void;
  promptListKey: number;
  // Node palette (only in flow editor view)
  activeView: ActiveView;
  nodeTypes: NodeTypeSchema[];
  onGrab: (nodeType: NodeTypeSchema) => void;
  onCollapse: () => void;
}

const BUGS_LINES = [
  "Ehhh... What's up, doc?",
  "Ain't I a stinker?",
  "Of course you realize...",
  "...this means war!",
  "Watch me paste 'em!",
  "I knew I shoulda taken",
  "that left turn at",
  "Albuquerque...",
  "What a maroon!",
  "Th-th-th-that's all folks!",
];

function BugsBunnyDancer() {
  const svgRef = useRef<SVGSVGElement>(null);
  const [line, setLine] = useState(0);

  useEffect(() => {
    const talk = setInterval(() => setLine((l) => (l + 1) % BUGS_LINES.length), 2200);
    return () => clearInterval(talk);
  }, []);

  useEffect(() => {
    if (!svgRef.current) return;

    // Whole-body bounce (gentle hop)
    const bounce = anime({
      targets: svgRef.current.querySelector("#bugs-body"),
      translateY: [-3, 3],
      duration: 500,
      easing: "easeInOutSine",
      direction: "alternate",
      loop: true,
    });

    // Left ear flop
    const earL = anime({
      targets: svgRef.current.querySelector("#bugs-ear-l"),
      rotate: [-10, 10],
      duration: 700,
      easing: "easeInOutQuad",
      direction: "alternate",
      loop: true,
    });

    // Right ear flop (offset timing)
    const earR = anime({
      targets: svgRef.current.querySelector("#bugs-ear-r"),
      rotate: [8, -8],
      duration: 800,
      easing: "easeInOutQuad",
      direction: "alternate",
      loop: true,
    });

    // Right arm wave (holding carrot)
    const arm = anime({
      targets: svgRef.current.querySelector("#bugs-arm-r"),
      rotate: [0, -20, 0, -20, 0],
      duration: 1800,
      easing: "easeInOutSine",
      loop: true,
    });

    // Carrot chomp pulse
    const carrot = anime({
      targets: svgRef.current.querySelector("#bugs-carrot"),
      translateY: [0, -2, 0],
      scale: [1, 0.92, 1],
      duration: 600,
      easing: "easeInOutSine",
      loop: true,
    });

    // Mouth open/close (chomp)
    const mouth = anime({
      targets: svgRef.current.querySelector("#bugs-mouth-open"),
      scaleY: [1, 0.3, 1],
      duration: 600,
      easing: "easeInOutSine",
      loop: true,
    });

    // Left foot tap
    const foot = anime({
      targets: svgRef.current.querySelector("#bugs-foot-l"),
      rotate: [-5, 5],
      duration: 400,
      easing: "easeInOutSine",
      direction: "alternate",
      loop: true,
    });

    return () => {
      [bounce, earL, earR, arm, carrot, mouth, foot].forEach((a) => a.pause());
    };
  }, []);

  return (
    <div className="sidebar-toon-dancer">
      <svg
        ref={svgRef}
        viewBox="0 0 120 150"
        width="96"
        height="120"
        xmlns="http://www.w3.org/2000/svg"
        style={{ display: "block", margin: "0 auto" }}
      >
        <defs>
          {/* Gray fur */}
          <linearGradient id="fur" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#a0a4aa" />
            <stop offset="100%" stopColor="#7a7e85" />
          </linearGradient>
          {/* Inner ear warm peach/orange */}
          <linearGradient id="inner-ear" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#f5c78a" />
            <stop offset="100%" stopColor="#e8a44e" />
          </linearGradient>
        </defs>

        <g id="bugs-body">
          {/* === EARS === */}
          {/* Left ear */}
          <g id="bugs-ear-l" style={{ transformOrigin: "44px 30px" }}>
            <ellipse cx="44" cy="14" rx="7" ry="22"
              fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
            <ellipse cx="44" cy="14" rx="3.5" ry="17"
              fill="url(#inner-ear)" />
          </g>
          {/* Right ear */}
          <g id="bugs-ear-r" style={{ transformOrigin: "68px 30px" }}>
            <ellipse cx="68" cy="12" rx="7.5" ry="24"
              fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
            <ellipse cx="68" cy="12" rx="3.8" ry="19"
              fill="url(#inner-ear)" />
          </g>

          {/* === HEAD === */}
          {/* Head base (gray) */}
          <ellipse cx="56" cy="48" rx="22" ry="19"
            fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
          {/* White muzzle/cheek area */}
          <ellipse cx="56" cy="54" rx="16" ry="13"
            fill="#fff" stroke="none" />
          {/* Cheek puffs */}
          <ellipse cx="42" cy="52" rx="8" ry="7" fill="#fff" />
          <ellipse cx="70" cy="52" rx="8" ry="7" fill="#fff" />

          {/* Eyes — classic Bugs half-lidded look */}
          <ellipse cx="48" cy="44" rx="4" ry="5" fill="#fff" stroke="#222" strokeWidth="0.8" />
          <ellipse cx="64" cy="44" rx="4" ry="5" fill="#fff" stroke="#222" strokeWidth="0.8" />
          {/* Pupils (looking slightly to side, classic smug) */}
          <circle cx="49.5" cy="44.5" r="2" fill="#111" />
          <circle cx="65.5" cy="44.5" r="2" fill="#111" />
          {/* Eye highlights */}
          <circle cx="50.5" cy="43.5" r="0.7" fill="#fff" />
          <circle cx="66.5" cy="43.5" r="0.7" fill="#fff" />
          {/* Eyelids (half-lidded) */}
          <path d="M44 41 Q48 39 52 41" fill="url(#fur)" stroke="#222" strokeWidth="0.6" />
          <path d="M60 41 Q64 39 68 41" fill="url(#fur)" stroke="#222" strokeWidth="0.6" />

          {/* Nose — pink oval */}
          <ellipse cx="56" cy="50" rx="3" ry="2.2" fill="#e88" stroke="#222" strokeWidth="0.6" />

          {/* Whiskers */}
          <line x1="40" y1="50" x2="28" y2="47" stroke="#222" strokeWidth="0.5" />
          <line x1="40" y1="52" x2="27" y2="52" stroke="#222" strokeWidth="0.5" />
          <line x1="40" y1="54" x2="28" y2="57" stroke="#222" strokeWidth="0.5" />
          <line x1="72" y1="50" x2="84" y2="47" stroke="#222" strokeWidth="0.5" />
          <line x1="72" y1="52" x2="85" y2="52" stroke="#222" strokeWidth="0.5" />
          <line x1="72" y1="54" x2="84" y2="57" stroke="#222" strokeWidth="0.5" />

          {/* Mouth — open grin */}
          <g id="bugs-mouth-open" style={{ transformOrigin: "56px 58px" }}>
            <path d="M47 56 Q52 55 56 56 Q60 55 65 56 Q62 64 56 65 Q50 64 47 56Z"
              fill="#c0392b" stroke="#222" strokeWidth="0.8" />
            {/* Tongue */}
            <ellipse cx="56" cy="62" rx="4" ry="2.5" fill="#e57373" />
          </g>
          {/* Buck teeth */}
          <rect x="52" y="55.5" width="3.5" height="4.5" rx="1" fill="#fff" stroke="#222" strokeWidth="0.5" />
          <rect x="56" y="55.5" width="3.5" height="4.5" rx="1" fill="#fff" stroke="#222" strokeWidth="0.5" />

          {/* === BODY === */}
          {/* Torso (gray) */}
          <ellipse cx="56" cy="82" rx="18" ry="20"
            fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
          {/* White belly */}
          <ellipse cx="56" cy="84" rx="12" ry="15"
            fill="#fff" stroke="none" />

          {/* === LEFT ARM (resting on hip) === */}
          <path d="M38 72 Q30 78 28 86 Q27 88 30 88"
            fill="none" stroke="url(#fur)" strokeWidth="5" strokeLinecap="round" />
          {/* White glove */}
          <circle cx="29" cy="87" r="4" fill="#fff" stroke="#222" strokeWidth="0.8" />

          {/* === RIGHT ARM (holding carrot) === */}
          <g id="bugs-arm-r" style={{ transformOrigin: "74px 72px" }}>
            <path d="M74 72 Q82 64 84 58"
              fill="none" stroke="url(#fur)" strokeWidth="5" strokeLinecap="round" />
            {/* White glove */}
            <circle cx="84" cy="57" r="4" fill="#fff" stroke="#222" strokeWidth="0.8" />
            {/* Carrot */}
            <g id="bugs-carrot" style={{ transformOrigin: "88px 48px" }}>
              <polygon points="82,54 92,38 86,54"
                fill="#e67e22" stroke="#222" strokeWidth="0.6" />
              {/* Carrot top leaves */}
              <path d="M91 38 Q89 32 86 30" fill="none" stroke="#27ae60" strokeWidth="1.5" strokeLinecap="round" />
              <path d="M92 38 Q93 33 91 29" fill="none" stroke="#27ae60" strokeWidth="1.5" strokeLinecap="round" />
              <path d="M92 39 Q96 34 95 30" fill="none" stroke="#2ecc71" strokeWidth="1" strokeLinecap="round" />
            </g>
          </g>

          {/* === LEGS === */}
          {/* Left leg */}
          <path d="M44 98 Q40 108 38 116"
            fill="none" stroke="url(#fur)" strokeWidth="6" strokeLinecap="round" />
          {/* Right leg */}
          <path d="M68 98 Q72 108 74 116"
            fill="none" stroke="url(#fur)" strokeWidth="6" strokeLinecap="round" />

          {/* === FEET (big cartoon rabbit feet) === */}
          <g id="bugs-foot-l" style={{ transformOrigin: "36px 120px" }}>
            <ellipse cx="32" cy="120" rx="10" ry="4"
              fill="url(#fur)" stroke="#222" strokeWidth="0.8" />
            {/* Toe lines */}
            <line x1="25" y1="119" x2="25" y2="121" stroke="#222" strokeWidth="0.4" />
            <line x1="28" y1="118" x2="28" y2="122" stroke="#222" strokeWidth="0.4" />
          </g>
          <ellipse cx="80" cy="120" rx="10" ry="4"
            fill="url(#fur)" stroke="#222" strokeWidth="0.8" />
          <line x1="87" y1="119" x2="87" y2="121" stroke="#222" strokeWidth="0.4" />
          <line x1="84" y1="118" x2="84" y2="122" stroke="#222" strokeWidth="0.4" />

          {/* Tail (small white puff) */}
          <circle cx="74" cy="96" r="4" fill="#fff" stroke="#ccc" strokeWidth="0.5" />
        </g>
      </svg>
      <div className="toon-dialog">{BUGS_LINES[line]}</div>
    </div>
  );
}

const typeColors: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

export default function Sidebar({
  flows,
  activeFlowId,
  onSelectFlow,
  onCreateFlow,
  onImportTemplate,
  onToggleEnabled,
  selectedAgentId,
  selectedSessionId,
  onSelectSession,
  agentListKey,
  onAgentCreated,
  selectedPromptId,
  onSelectPrompt,
  promptListKey,
  activeView,
  nodeTypes,
  onGrab,
  onCollapse,
}: SidebarProps) {
  const [showGallery, setShowGallery] = useState(false);
  const [agents, setAgents] = useState<AgentSummary[]>([]);
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  const [agentMeta, setAgentMeta] = useState<Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>>(new Map());
  const [expandedAgents, setExpandedAgents] = useState<Set<string>>(new Set());

  const refreshAgents = useCallback(async () => {
    try {
      const list = await listAgents();
      setAgents(list);
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshAgents();
  }, [refreshAgents, agentListKey]);

  // Poll agent session data for tree display
  useEffect(() => {
    if (agents.length === 0) return;

    const fetchMeta = async () => {
      const results = await Promise.allSettled(
        agents.map((a) => listAgentSessions(a.id).then((info) => ({ id: a.id, info })))
      );
      const next = new Map<string, { busy: boolean; sessions: InteractSessionInfo[]; cost: number }>();
      for (const r of results) {
        if (r.status === "fulfilled") {
          const { id, info } = r.value;
          const busy = info.sessions.some((s) => s.busy);
          const cost = info.sessions.reduce((sum, s) => sum + s.total_cost, 0);
          next.set(id, { busy, sessions: info.sessions, cost });
        }
      }
      setAgentMeta(next);
    };

    fetchMeta();
    const interval = setInterval(fetchMeta, 5000);
    return () => clearInterval(interval);
  }, [agents]);

  const refreshPrompts = useCallback(async () => {
    try {
      setPrompts(await listPrompts());
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshPrompts();
  }, [refreshPrompts, promptListKey]);

  async function handleCreatePrompt() {
    try {
      const { id } = await savePrompt({
        title: "New Prompt",
        summary: "",
        source_flow_name: "",
        tags: [],
      });
      await refreshPrompts();
      onSelectPrompt(id);
    } catch (e) {
      console.error("Failed to create prompt:", e);
    }
  }

  async function handleDeletePrompt(e: React.MouseEvent, id: string) {
    e.stopPropagation();
    if (!confirm("Delete this prompt?")) return;
    try {
      await deletePromptApi(id);
      await refreshPrompts();
    } catch (e) {
      console.error("Failed to delete prompt:", e);
    }
  }

  async function handleCreateAgent() {
    try {
      const { id } = await createAgent({ name: "New Agent" });
      await refreshAgents();
      onAgentCreated(id);
    } catch (e) {
      console.error("Failed to create agent:", e);
    }
  }

  async function handleDeleteAgent(e: React.MouseEvent, agentId: string) {
    e.stopPropagation();
    if (!confirm("Delete this agent?")) return;
    try {
      await deleteAgent(agentId);
      await refreshAgents();
    } catch (err) {
      console.error("Failed to delete agent:", err);
    }
  }

  function handleNewFlowClick() {
    setShowGallery(true);
  }

  function handleGalleryImport(flow: Flow) {
    setShowGallery(false);
    onImportTemplate(flow);
  }

  function handleBlank() {
    setShowGallery(false);
    onCreateFlow();
  }

  const grouped = {
    trigger: nodeTypes.filter((n) => n.node_type === "trigger"),
    source: nodeTypes.filter((n) => n.node_type === "source"),
    executor: nodeTypes.filter((n) => n.node_type === "executor"),
    sink: nodeTypes.filter((n) => n.node_type === "sink"),
  };

  return (
    <div className="unified-sidebar">
      <div className="sidebar-collapse-bar">
        <button className="sidebar-collapse-btn" onClick={onCollapse} title="Collapse sidebar">
          ◨
        </button>
      </div>
      {showGallery && (
        <TemplateGallery
          onImport={handleGalleryImport}
          onBlank={handleBlank}
          onClose={() => setShowGallery(false)}
        />
      )}

      <BugsBunnyDancer />

      {/* Agents section (primary, expanded by default) */}
      <Collapsible defaultOpen className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Agents</h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleCreateAgent();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {[...agents].sort((a, b) => {
              if (a.id === STUDIO_ASSISTANT_ID) return -1;
              if (b.id === STUDIO_ASSISTANT_ID) return 1;
              return 0;
            }).map((agent) => {
              const meta = agentMeta.get(agent.id);
              const isExpanded = expandedAgents.has(agent.id);
              const isActive = agent.id === selectedAgentId && activeView === "agent-workspace";
              const sessions = meta?.sessions ?? [];

              return (
                <div key={agent.id} className="sb-agent">
                  <div
                    className={`sb-agent-row${isActive ? " sb-agent-active" : ""}`}
                    onClick={() => {
                      setExpandedAgents((prev) => {
                        const next = new Set(prev);
                        if (next.has(agent.id)) next.delete(agent.id);
                        else next.add(agent.id);
                        return next;
                      });
                      if (sessions.length > 0) {
                        onSelectSession(agent.id, sessions[0].session_id);
                      }
                    }}
                  >
                    <span className="sb-agent-chevron">{isExpanded ? "▾" : "▸"}</span>
                    {meta?.busy && <span className="sb-agent-pulse" />}
                    <span className="sb-agent-name">{agent.name}</span>
                    {meta && meta.cost > 0 && (
                      <span className="sb-agent-cost">${meta.cost.toFixed(2)}</span>
                    )}
                    {agent.id !== STUDIO_ASSISTANT_ID && (
                      <button
                        className="ghost sb-agent-delete"
                        onClick={(e) => handleDeleteAgent(e, agent.id)}
                        title="Delete agent"
                      >
                        ×
                      </button>
                    )}
                  </div>
                  {isExpanded && (
                    <div className="sb-sessions">
                      {sessions.map((s) => {
                        const isSessionActive = s.session_id === selectedSessionId && agent.id === selectedAgentId;
                        const label = s.summary || (s.kind === "flow_run" ? `Run: ${s.flow_run?.flow_name ?? ""}` : "New session");
                        return (
                          <div
                            key={s.session_id}
                            className={`sb-session${isSessionActive ? " sb-session-active" : ""}`}
                            onClick={() => onSelectSession(agent.id, s.session_id)}
                          >
                            {s.busy && <span className="sb-session-pulse" />}
                            <span className="sb-session-label">{label}</span>
                            {s.total_cost > 0 && (
                              <span className="sb-session-cost">${s.total_cost.toFixed(2)}</span>
                            )}
                          </div>
                        );
                      })}
                      <button
                        className="sb-session-new"
                        onClick={async (e) => {
                          e.stopPropagation();
                          try {
                            const result = await newAgentSession(agent.id);
                            onSelectSession(agent.id, result.session_id);
                          } catch (err) {
                            console.error("Failed to create session:", err);
                          }
                        }}
                      >
                        + New Session
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
            {agents.length === 0 && (
              <div className="sidebar-item-empty">No agents yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Flows section (collapsed by default) */}
      <Collapsible className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Flows</h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleNewFlowClick();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {flows.map((flow) => (
              <div
                key={flow.id}
                className={`sidebar-item${flow.id === activeFlowId && activeView === "flow-editor" ? " active" : ""}${!flow.enabled ? " disabled" : ""}`}
                onClick={() => onSelectFlow(flow.id)}
              >
                <div className="sidebar-item-row">
                  <div className="sidebar-item-name">{flow.name}</div>
                  <Switch
                    checked={flow.enabled}
                    onCheckedChange={() => onToggleEnabled(flow.id)}
                    onClick={(e) => e.stopPropagation()}
                    className="data-[state=checked]:bg-[var(--success)]"
                  />
                </div>
                <div className="sidebar-item-meta">{flow.node_count} nodes</div>
              </div>
            ))}
            {flows.length === 0 && (
              <div className="sidebar-item-empty">No flows yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Prompts section */}
      <Collapsible defaultOpen className="sidebar-section">
        <CollapsibleTrigger asChild>
          <div className="sidebar-section-header">
            <span className="sidebar-chevron">▶</span>
            <h2>Prompts</h2>
            <div style={{ flex: 1 }} />
            <button
              className="ghost sidebar-action-btn"
              onClick={(e) => {
                e.stopPropagation();
                handleCreatePrompt();
              }}
            >
              +
            </button>
          </div>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <div className="sidebar-section-body">
            {prompts.map((p) => (
              <div
                key={p.id}
                className={`sidebar-item${p.id === selectedPromptId && activeView === "prompt-editor" ? " active" : ""}`}
                onClick={() => onSelectPrompt(p.id)}
              >
                <div className="sidebar-item-row">
                  <div className="sidebar-item-name">{p.title}</div>
                  <button
                    className="ghost sidebar-delete-btn"
                    onClick={(e) => handleDeletePrompt(e, p.id)}
                    title="Delete prompt"
                  >
                    ×
                  </button>
                </div>
                {p.tags.length > 0 && (
                  <div className="sidebar-item-meta">{p.tags.join(", ")}</div>
                )}
              </div>
            ))}
            {prompts.length === 0 && (
              <div className="sidebar-item-empty">No prompts yet</div>
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>

      {/* Node palette — only visible in flow editor with an active flow */}
      {activeView === "flow-editor" && activeFlowId && (
        <Collapsible defaultOpen className="sidebar-section sidebar-palette-section">
          <CollapsibleTrigger asChild>
            <div className="sidebar-section-header">
              <span className="sidebar-chevron">▶</span>
              <h2>Nodes</h2>
            </div>
          </CollapsibleTrigger>
          <CollapsibleContent>
            <div className="sidebar-section-body">
              {(["trigger", "source", "executor", "sink"] as const).map((type) => (
                <div key={type}>
                  {grouped[type].map((nt) => (
                    <div
                      key={nt.kind}
                      className="palette-item"
                      onMouseDown={(e) => {
                        e.preventDefault();
                        onGrab(nt);
                      }}
                    >
                      <div
                        className="palette-dot"
                        style={{ background: typeColors[nt.node_type] }}
                      />
                      {nt.label}
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </CollapsibleContent>
        </Collapsible>
      )}

    </div>
  );
}
