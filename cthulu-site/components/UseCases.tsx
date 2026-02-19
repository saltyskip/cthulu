"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

const cases = [
  {
    id: "pr-reviews",
    label: "PR Reviews",
    icon: "\ud83d\udd0d",
    headline: "Not another diff checker. A real code reviewer.",
    description:
      "Most PR review bots just scan the diff and leave generic comments. Cthulu agents clone the entire repo, explore the codebase, trace call chains, and run tools — the same way a senior engineer would review your code.",
    details: [
      "Agents clone the full repo and navigate the codebase — not just the diff, but the code around it",
      "Can grep for related usages, read imported modules, and understand how changes affect the wider system",
      "Runs in a sandboxed environment with real filesystem access, Bash, and CLI tools",
      "Posts inline comments on specific lines with context-aware, actionable feedback",
      "Approves clean PRs or requests changes — with explanations that reference the actual codebase",
      "Scoped permissions per pipeline — each agent only gets the tools it needs",
    ],
    pipeline: {
      trigger: "GitHub PR (polling every 60s)",
      source: "Full repo clone + PR diff",
      output: "Inline comments + approve/request-changes via GitHub API",
    },
  },
  {
    id: "content-creation",
    label: "Content Creation",
    icon: "\ud83d\udcdd",
    headline: "Newsletters, changelogs, and briefs — on autopilot",
    description:
      "Stop spending hours writing the same recurring content. Define the sources, the voice, and the destination — agents handle the rest.",
    details: [
      "Fetch from RSS feeds, web scrapers, GitHub merged PRs, and live market data",
      "Filter content with keyword matching (AND/OR logic) to surface what matters",
      "Generate rich output with images, callouts, memes, and formatted tables",
      "Publish directly to Slack (with Block Kit threading) or Notion (rich database pages)",
      "Battle-tested prompt templates for market briefs, dev changelogs, stakeholder updates, and newsletters",
    ],
    pipeline: {
      trigger: "Cron (hourly, daily, weekly — you choose)",
      source: "RSS + Web Scrapers + GitHub + Market Data",
      output: "Slack (threaded) + Notion (rich blocks)",
    },
  },
];

export default function UseCases() {
  const [active, setActive] = useState("pr-reviews");
  const current = cases.find((c) => c.id === active)!;

  return (
    <section id="use-cases" className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Built for two things you hate doing
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Real agents with real access, solving the two biggest time sinks on
          your team.
        </p>

        {/* Tabs */}
        <div className="mt-10 flex justify-center gap-3">
          {cases.map((c) => (
            <button
              key={c.id}
              onClick={() => setActive(c.id)}
              className={`rounded-lg px-5 py-2.5 text-sm font-medium transition-colors ${
                active === c.id
                  ? "bg-accent text-white"
                  : "border border-border text-text-secondary hover:text-text"
              }`}
            >
              <span className="mr-1.5">{c.icon}</span>
              {c.label}
            </button>
          ))}
        </div>

        {/* Content */}
        <AnimatePresence mode="wait">
          <motion.div
            key={current.id}
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.3 }}
            className="mt-8 rounded-xl border border-border bg-bg-secondary p-8"
          >
            <h3 className="text-xl font-semibold text-text">
              {current.headline}
            </h3>
            <p className="mt-2 text-text-secondary">{current.description}</p>

            <ul className="mt-6 space-y-2">
              {current.details.map((d, i) => (
                <li key={i} className="flex gap-3 text-sm text-text-secondary">
                  <span className="mt-0.5 text-sink">&#10003;</span>
                  {d}
                </li>
              ))}
            </ul>

            <div className="mt-8 grid gap-4 sm:grid-cols-3">
              <div className="rounded-lg border border-border bg-bg-tertiary p-3">
                <div className="text-[10px] font-semibold uppercase text-trigger">
                  Trigger
                </div>
                <div className="mt-1 text-sm text-text">
                  {current.pipeline.trigger}
                </div>
              </div>
              <div className="rounded-lg border border-border bg-bg-tertiary p-3">
                <div className="text-[10px] font-semibold uppercase text-source">
                  Source
                </div>
                <div className="mt-1 text-sm text-text">
                  {current.pipeline.source}
                </div>
              </div>
              <div className="rounded-lg border border-border bg-bg-tertiary p-3">
                <div className="text-[10px] font-semibold uppercase text-sink">
                  Output
                </div>
                <div className="mt-1 text-sm text-text">
                  {current.pipeline.output}
                </div>
              </div>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
    </section>
  );
}
