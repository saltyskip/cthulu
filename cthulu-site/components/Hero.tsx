"use client";

import { motion } from "framer-motion";
import FlowDemo from "./FlowDemo";

export default function Hero() {
  return (
    <section className="relative px-6 pt-32 pb-20 overflow-hidden">
      {/* Cthulhu background illustration */}
      <div className="pointer-events-none absolute inset-0" aria-hidden="true">
        <img
          src="/cthulu-hero.jpg"
          alt=""
          className="h-full w-full object-cover object-top opacity-20"
        />
        <div className="absolute inset-0 bg-gradient-to-b from-bg/60 via-bg/40 to-bg" />
      </div>

      <div className="relative mx-auto max-w-6xl">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6 }}
          className="text-center"
        >
          <h1 className="text-5xl font-bold tracking-tight sm:text-6xl lg:text-7xl">
            AI agents that do the{" "}
            <span className="bg-gradient-to-r from-accent to-executor bg-clip-text text-transparent">
              work you keep putting off
            </span>
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg text-text-secondary sm:text-xl">
            Sandboxed agents with full filesystem and tool access that review
            your PRs, write your newsletters, and generate changelogs — defined
            in JSON, deployed via git.
          </p>
          <div className="mt-8 flex items-center justify-center gap-4">
            <a
              href="#waitlist"
              className="rounded-lg bg-accent px-6 py-3 font-medium text-primary-foreground transition-opacity hover:opacity-90"
            >
              Sign Up for the Waitlist
            </a>
          </div>
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 30 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.6, delay: 0.3 }}
          className="mt-16"
        >
          <FlowDemo />
        </motion.div>
      </div>
    </section>
  );
}
