"use client";

import { motion } from "framer-motion";

const props = [
  {
    title: "Sandboxed, Not Limited",
    description:
      "These aren't API wrappers. Each agent runs in a sandboxed environment with full filesystem access, CLI tools, and scoped permissions.",
    icon: "\ud83d\udd12",
  },
  {
    title: "Config as Code",
    description:
      "Pipelines are JSON files you can PR review, version, and deploy like infrastructure. GitOps for your AI agents.",
    icon: "\ud83d\udccb",
  },
  {
    title: "Visual Pipeline Builder",
    description:
      "Cthulu Studio: drag-and-drop flow editor. Build pipelines visually, monitor runs, trigger manually.",
    icon: "\ud83c\udfa8",
  },
];

export default function ValueProps() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto grid max-w-6xl gap-8 md:grid-cols-3">
        {props.map((p, i) => (
          <motion.div
            key={p.title}
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5, delay: i * 0.1 }}
            className="rounded-xl border border-border bg-bg-secondary p-6"
          >
            <span className="text-3xl">{p.icon}</span>
            <h3 className="mt-4 text-lg font-semibold text-text">{p.title}</h3>
            <p className="mt-2 text-sm leading-relaxed text-text-secondary">
              {p.description}
            </p>
          </motion.div>
        ))}
      </div>
    </section>
  );
}
