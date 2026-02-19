"use client";

import { motion } from "framer-motion";

const steps = [
  {
    label: "Define",
    color: "#d29922",
    icon: "\ud83d\udccb",
    title: "Pick a trigger, point at your data",
    description: "Choose what kicks off the pipeline — a cron schedule, a new GitHub PR, or a manual trigger. Then define where the data comes from: RSS feeds, web scrapers, GitHub repos, or market APIs.",
  },
  {
    label: "Sandbox",
    color: "#58a6ff",
    icon: "\ud83d\udd12",
    title: "Scoped environments, real access",
    description: "Each agent runs in a sandboxed environment with filesystem access, CLI tools, and only the permissions you grant. Not a prompt wrapper — a real runtime with real capabilities.",
  },
  {
    label: "Execute",
    color: "#bc8cff",
    icon: "\ud83e\udde0",
    title: "Agents that actually do things",
    description: "Agents don't just process text. They clone repos, grep codebases, read files, run commands, and generate structured output. The same things a human would do, on autopilot.",
  },
  {
    label: "Deliver",
    color: "#3fb950",
    icon: "\ud83d\udce4",
    title: "Output where you need it",
    description: "Results go to Slack (with Block Kit threading), Notion (rich database pages with images and callouts), GitHub (inline PR comments), or wherever your team works.",
  },
];

export default function HowItWorks() {
  return (
    <section id="how-it-works" className="px-6 py-20">
      <div className="mx-auto max-w-4xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          How It Works
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          More than API calls. Agents with real tools in real environments.
        </p>

        <div className="mt-12 space-y-0">
          {steps.map((step, i) => (
            <motion.div
              key={step.label}
              initial={{ opacity: 0, x: -20 }}
              whileInView={{ opacity: 1, x: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.4, delay: i * 0.1 }}
              className="flex gap-6"
            >
              {/* Timeline */}
              <div className="flex flex-col items-center">
                <div
                  className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-lg"
                  style={{ background: step.color + "22", color: step.color }}
                >
                  {step.icon}
                </div>
                {i < steps.length - 1 && (
                  <div
                    className="w-0.5 grow"
                    style={{ background: step.color + "33" }}
                  />
                )}
              </div>

              {/* Content */}
              <div className="pb-10">
                <span
                  className="rounded px-2 py-0.5 text-xs font-semibold uppercase"
                  style={{ background: step.color + "22", color: step.color }}
                >
                  {step.label}
                </span>
                <h3 className="mt-2 text-lg font-semibold text-text">
                  {step.title}
                </h3>
                <p className="mt-1 text-sm text-text-secondary">
                  {step.description}
                </p>
              </div>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}
