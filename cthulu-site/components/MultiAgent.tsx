"use client";

import { motion } from "framer-motion";

const terminalOutputs = [
  {
    title: "claude \u2014 pr-review",
    lines: [
      { text: "$ claude --allowedTools Read,Grep,Bash", color: "var(--success)" },
      { text: "Cloning acme-corp/api-server...", color: "var(--text-secondary)" },
      { text: "Reading src/handlers/auth.rs...", color: "var(--text-secondary)" },
      { text: "\u280b Analyzing PR #142 diff...", color: "var(--source-color)" },
      { text: "Found 3 issues in auth middleware", color: "var(--warning)" },
      { text: "Posting inline comment on L47...", color: "var(--text-secondary)" },
      { text: "Posting inline comment on L93...", color: "var(--text-secondary)" },
      { text: "gh pr review --request-changes", color: "var(--success)" },
    ],
    delay: 0,
  },
  {
    title: "claude \u2014 newsletter",
    lines: [
      { text: "$ claude --allowedTools Read,Bash", color: "var(--success)" },
      { text: "Fetching thedefiant.io/feed...", color: "var(--text-secondary)" },
      { text: "Fetching blockworks.co/feed...", color: "var(--text-secondary)" },
      { text: "\u280b 23 items fetched, filtering...", color: "var(--source-color)" },
      { text: "Generating newsletter draft...", color: "var(--text-secondary)" },
      { text: "Adding market data table...", color: "var(--text-secondary)" },
      { text: "Publishing to Notion...", color: "var(--executor-color)" },
      { text: "\u2713 Newsletter published", color: "var(--success)" },
    ],
    delay: 2,
  },
  {
    title: "claude \u2014 changelog",
    lines: [
      { text: "$ claude --allowedTools Read,Grep", color: "var(--success)" },
      { text: "Querying merged PRs (7 days)...", color: "var(--text-secondary)" },
      { text: "Found 12 PRs across 4 repos", color: "var(--text-secondary)" },
      { text: "\u280b Categorizing changes...", color: "var(--source-color)" },
      { text: "3 features, 7 fixes, 2 chores", color: "var(--warning)" },
      { text: "Formatting Slack blocks...", color: "var(--text-secondary)" },
      { text: "Posted to #dev-updates", color: "var(--executor-color)" },
      { text: "\u2713 Changelog delivered", color: "var(--success)" },
    ],
    delay: 4,
  },
  {
    title: "you@macbook \u2014 zsh",
    lines: [
      { text: "$ gh pr review 142 --approve", color: "var(--success)" },
      { text: "\u2713 Approved  # didn't read it", color: "var(--danger)" },
      { text: "$ gh pr review 143 --approve", color: "var(--success)" },
      { text: "\u2713 Approved  # mass deletes prod db?", color: "var(--danger)" },
      { text: "$ gh pr review 144 --approve", color: "var(--success)" },
      { text: "\u2713 Approved  # rm -rf / looks fine", color: "var(--danger)" },
      { text: "$ gh pr review 145 --approve", color: "var(--success)" },
      { text: "\u2713 Approved  # who needs tests anyway", color: "var(--danger)" },
    ],
    delay: 1,
  },
];

function AnimatedTerminal({
  title,
  lines,
  delay,
}: {
  title: string;
  lines: { text: string; color: string }[];
  delay: number;
}) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded border border-border" style={{ background: "var(--bg)" }}>
      <div className="flex items-center gap-1.5 border-b border-border px-2.5 py-1.5 shrink-0">
        <div className="flex gap-1">
          <div className="h-2 w-2 rounded-full" style={{ background: "var(--danger)" }} />
          <div className="h-2 w-2 rounded-full" style={{ background: "var(--warning)" }} />
          <div className="h-2 w-2 rounded-full" style={{ background: "var(--success)" }} />
        </div>
        <span className="ml-1 truncate text-[9px] text-text-secondary font-mono">{title}</span>
      </div>
      <div className="flex-1 overflow-hidden px-2.5 py-2 font-mono">
        {lines.map((line, i) => (
          <div
            key={i}
            className="terminal-line text-[10px] leading-relaxed whitespace-nowrap"
            style={{
              color: line.color,
              animationDelay: `${delay + i * 0.6}s`,
            }}
          >
            {line.text}
          </div>
        ))}
      </div>
    </div>
  );
}

function TerminalChaos() {
  return (
    <div className="relative h-72 overflow-hidden rounded-lg border border-border bg-bg-tertiary p-1.5">
      <div className="grid h-full grid-cols-2 grid-rows-2 gap-1.5">
        {terminalOutputs.map((t, i) => (
          <AnimatedTerminal
            key={i}
            title={t.title}
            lines={t.lines}
            delay={t.delay}
          />
        ))}
      </div>
      {/* Overlay badge */}
      <div className="absolute inset-0 flex items-end justify-center pb-3 pointer-events-none">
        <span
          className="rounded-full px-3 py-1 text-xs"
          style={{
            background: "color-mix(in srgb, var(--bg) 93%, transparent)",
            border: "1px solid color-mix(in srgb, var(--danger) 27%, transparent)",
            color: "var(--danger)",
          }}
        >
          You, pretending you know what&apos;s happening
        </span>
      </div>
    </div>
  );
}

function OrchestrationView() {
  const flows = [
    {
      name: "PR Review Bot",
      status: "running",
      trigger: "GitHub PR",
      lastRun: "12s ago",
      color: "var(--success)",
    },
    {
      name: "Market Brief",
      status: "scheduled",
      trigger: "Every 4h",
      lastRun: "2h ago",
      color: "var(--source-color)",
    },
    {
      name: "Dev Changelog",
      status: "idle",
      trigger: "Mondays 9am",
      lastRun: "5d ago",
      color: "var(--text-secondary)",
    },
    {
      name: "Newsletter",
      status: "running",
      trigger: "Daily 8am",
      lastRun: "3m ago",
      color: "var(--success)",
    },
  ];

  return (
    <div className="h-72 overflow-hidden rounded-lg border border-border bg-bg-secondary">
      {/* Header bar */}
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <span className="text-xs font-medium text-text">Cthulu Dashboard</span>
        <div className="flex items-center gap-2">
          <span className="h-1.5 w-1.5 rounded-full" style={{ background: "var(--success)" }} />
          <span className="text-[10px] text-text-secondary">4 flows active</span>
        </div>
      </div>
      {/* Flow rows */}
      <div className="divide-y divide-border">
        {flows.map((f) => (
          <div key={f.name} className="flex items-center justify-between px-4 py-3.5">
            <div className="flex items-center gap-3">
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: f.color }}
              />
              <div>
                <div className="text-xs font-medium text-text">{f.name}</div>
                <div className="text-[10px] text-text-secondary">{f.trigger}</div>
              </div>
            </div>
            <div className="text-right">
              <div
                className="text-[10px] font-medium"
                style={{ color: f.color }}
              >
                {f.status}
              </div>
              <div className="text-[10px] text-text-secondary">{f.lastRun}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function MultiAgent() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Multi-agent, minus the chaos
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Running agents today means opening terminal windows and hoping for the
          best. Cthulu gives you a single control plane for all of them.
        </p>

        <div className="mt-12 grid gap-6 lg:grid-cols-2">
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5 }}
          >
            <div className="mb-3 flex items-center gap-2">
              <span
                className="rounded px-2 py-0.5 text-xs font-medium"
                style={{
                  background: "color-mix(in srgb, var(--danger) 13%, transparent)",
                  color: "var(--danger)",
                }}
              >
                Before
              </span>
              <span className="text-sm text-text-secondary">
                The &ldquo;multi-agent&rdquo; experience today
              </span>
            </div>
            <TerminalChaos />
          </motion.div>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5 }}
          >
            <div className="mb-3 flex items-center gap-2">
              <span
                className="rounded px-2 py-0.5 text-xs font-medium"
                style={{
                  background: "color-mix(in srgb, var(--success) 13%, transparent)",
                  color: "var(--success)",
                }}
              >
                With Cthulu
              </span>
              <span className="text-sm text-text-secondary">
                One server. All your agents. Full visibility.
              </span>
            </div>
            <OrchestrationView />
          </motion.div>
        </div>
      </div>
    </section>
  );
}
