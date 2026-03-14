"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

const skills = [
  {
    id: "pr-review",
    label: "PR Review",
    icon: "🔍",
    color: "var(--source-color)",
    headline: "Deep code review, not diff scanning",
    description:
      "Clones the repo, explores the codebase, traces call chains, and leaves inline comments with context — the same way a senior engineer reviews your code.",
    capabilities: [
      "Full repo clone with filesystem access",
      "Grep, trace imports, read related modules",
      "Inline GitHub comments on specific lines",
      "Approve or request changes with explanations",
    ],
    config: `skills:
  - name: pr-review
    trigger: github-pr
    tools: [Bash, Read, Grep, Glob]
    prompt: prompts/review.md
    permissions:
      github: comment, review`,
  },
  {
    id: "newsletter",
    label: "Newsletter",
    icon: "📰",
    color: "var(--trigger-color)",
    headline: "Recurring content on autopilot",
    description:
      "Fetches from RSS, web scrapers, and market data. Filters by keywords, generates rich content, and publishes to Slack or Notion on a schedule.",
    capabilities: [
      "Multi-source aggregation (RSS, web, APIs)",
      "Keyword filtering with AND/OR logic",
      "Rich output: images, tables, callouts",
      "Scheduled delivery to Slack and Notion",
    ],
    config: `skills:
  - name: market-brief
    trigger: cron("0 9 * * 1-5")
    sources: [rss, market-data]
    filter: keywords(crypto, AI, markets)
    output: slack(#daily-brief)`,
  },
  {
    id: "changelog",
    label: "Changelog",
    icon: "📋",
    color: "var(--executor-color)",
    headline: "Ship notes that write themselves",
    description:
      "Watches merged PRs, reads the actual code changes, and generates developer changelogs with context — not just commit message regurgitation.",
    capabilities: [
      "Polls GitHub for merged PRs",
      "Reads diffs and understands intent",
      "Groups by feature, bugfix, refactor",
      "Posts formatted updates to Notion or Slack",
    ],
    config: `skills:
  - name: dev-changelog
    trigger: cron("0 17 * * 5")
    source: github-merged-prs
    prompt: prompts/changelog.md
    output: notion(Changelog DB)`,
  },
  {
    id: "research",
    label: "Research",
    icon: "🧪",
    color: "var(--sink-color)",
    headline: "Deep-dive research with real tools",
    description:
      "Agents browse the web, scrape pages, parse data, and synthesize findings — then deliver a structured report wherever you need it.",
    capabilities: [
      "Web scraping with CSS selectors",
      "Multi-page crawling and extraction",
      "Data synthesis and summarization",
      "Structured output in Markdown or JSON",
    ],
    config: `skills:
  - name: competitor-watch
    trigger: cron("0 8 * * 1")
    source: web-scrape(competitor-urls)
    prompt: prompts/competitor-analysis.md
    output: notion(Research DB)`,
  },
];

export default function Skills() {
  const [active, setActive] = useState("pr-review");
  const current = skills.find((s) => s.id === active)!;

  return (
    <section id="skills" className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Agent Skills
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Pre-built skills that turn agents into specialists. Each skill
          defines triggers, tools, and permissions — ready to deploy.
        </p>

        {/* Skill tabs */}
        <div className="mt-10 flex flex-wrap justify-center gap-3" role="tablist" aria-label="Agent skills">
          {skills.map((s) => (
            <button
              key={s.id}
              role="tab"
              aria-selected={active === s.id}
              aria-controls={`skills-tabpanel-${s.id}`}
              id={`skills-tab-${s.id}`}
              tabIndex={active === s.id ? 0 : -1}
              onClick={() => setActive(s.id)}
              className={`rounded-lg px-5 py-2.5 text-sm font-medium transition-colors ${
                active === s.id
                  ? "bg-accent text-primary-foreground"
                  : "border border-border text-text-secondary hover:text-text"
              }`}
            >
              <span className="mr-1.5">{s.icon}</span>
              {s.label}
            </button>
          ))}
        </div>

        {/* Skill detail */}
        <AnimatePresence mode="wait">
          <motion.div
            key={current.id}
            role="tabpanel"
            id={`skills-tabpanel-${current.id}`}
            aria-labelledby={`skills-tab-${current.id}`}
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            transition={{ duration: 0.3 }}
            className="mt-8 grid gap-6 lg:grid-cols-2"
          >
            {/* Left: description + capabilities */}
            <div className="rounded-xl border border-border bg-bg-secondary p-8">
              <div
                className="flex h-10 w-10 items-center justify-center rounded-lg text-xl"
                style={{
                  background: `color-mix(in srgb, ${current.color} 12%, transparent)`,
                }}
              >
                {current.icon}
              </div>
              <h3 className="mt-4 text-xl font-semibold text-text">
                {current.headline}
              </h3>
              <p className="mt-2 text-text-secondary">{current.description}</p>

              <ul className="mt-6 space-y-2">
                {current.capabilities.map((c, i) => (
                  <li key={i} className="flex gap-3 text-sm text-text-secondary">
                    <span className="mt-0.5 text-sink">&#10003;</span>
                    {c}
                  </li>
                ))}
              </ul>
            </div>

            {/* Right: config preview */}
            <div className="rounded-xl border border-border bg-bg-secondary overflow-hidden">
              <div className="flex items-center gap-2 border-b border-border bg-bg-tertiary px-4 py-2">
                <span className="h-2.5 w-2.5 rounded-full bg-danger opacity-60" />
                <span className="h-2.5 w-2.5 rounded-full bg-warning opacity-60" />
                <span className="h-2.5 w-2.5 rounded-full bg-success opacity-60" />
                <span className="ml-3 text-xs text-text-secondary">
                  skill.yaml
                </span>
              </div>
              <pre className="overflow-x-auto p-6 text-sm leading-relaxed">
                <code className="text-text-secondary">{current.config}</code>
              </pre>
            </div>
          </motion.div>
        </AnimatePresence>
      </div>
    </section>
  );
}
