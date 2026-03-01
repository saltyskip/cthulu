"use client";

import { useState } from "react";

const links = [
  { label: "Use Cases", href: "#use-cases" },
  { label: "How It Works", href: "#how-it-works" },
  { label: "Studio", href: "#studio" },
];

export default function Nav() {
  const [open, setOpen] = useState(false);

  return (
    <nav className="fixed top-0 left-0 right-0 z-50 border-b border-border bg-bg/80 backdrop-blur-md">
      <div className="mx-auto flex max-w-6xl items-center justify-between px-6 py-4">
        <div className="flex items-center gap-1">
          <div
            className="h-8 w-8 flex-shrink-0 self-center"
            style={{
              WebkitMaskImage: "url(/cthulu-logo.png)",
              WebkitMaskSize: "150%",
              WebkitMaskRepeat: "no-repeat",
              WebkitMaskPosition: "center 45%",
              maskImage: "url(/cthulu-logo.png)",
              maskSize: "150%",
              maskRepeat: "no-repeat",
              maskPosition: "center 45%",
              backgroundColor: "var(--accent)",
            }}
          />
          <span className="text-xl font-bold leading-tight text-text">Cthulu</span>
          <span className="hidden translate-y-px text-sm leading-tight text-text-secondary sm:inline">
            Agent Orchestration Platform
          </span>
        </div>

        {/* Desktop links */}
        <div className="hidden items-center gap-6 md:flex">
          {links.map((l) => (
            <a
              key={l.label}
              href={l.href}
              className="text-sm text-text-secondary transition-colors hover:text-text"
            >
              {l.label}
            </a>
          ))}
          <a
            href="#waitlist"
            className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90"
          >
            Sign Up for the Waitlist
          </a>
        </div>

        {/* Mobile menu button */}
        <button
          className="text-text-secondary md:hidden"
          onClick={() => setOpen(!open)}
          aria-label="Toggle menu"
        >
          <svg
            width="24"
            height="24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          >
            {open ? (
              <path d="M6 6l12 12M6 18L18 6" />
            ) : (
              <path d="M4 6h16M4 12h16M4 18h16" />
            )}
          </svg>
        </button>
      </div>

      {/* Mobile menu */}
      {open && (
        <div className="border-t border-border bg-bg px-6 py-4 md:hidden">
          {links.map((l) => (
            <a
              key={l.label}
              href={l.href}
              className="block py-2 text-sm text-text-secondary"
              onClick={() => setOpen(false)}
            >
              {l.label}
            </a>
          ))}
          <a
            href="#waitlist"
            className="mt-2 block rounded-lg bg-accent px-4 py-2 text-center text-sm font-medium text-primary-foreground"
          >
            Sign Up for the Waitlist
          </a>
        </div>
      )}
    </nav>
  );
}
