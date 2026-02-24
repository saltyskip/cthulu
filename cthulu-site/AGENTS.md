# Cthulu Site - AI Assistant Guide

Marketing website for Cthulu. Built with Next.js 15, React 19, Tailwind CSS 4, and Framer Motion.

**Parent Documentation**: [Root CLAUDE.md](../CLAUDE.md)

---

## Architecture

```
cthulu-site/
├── app/
│   ├── layout.tsx              # Root layout (fonts, metadata)
│   ├── page.tsx                # Landing page (assembles all sections)
│   └── globals.css             # Global styles + Tailwind imports
├── components/
│   ├── Hero.tsx                # Hero section with tagline
│   ├── Features.tsx            # Feature cards grid
│   ├── HowItWorks.tsx          # Pipeline visualization
│   ├── UseCases.tsx            # Use case examples
│   ├── StudioShowcase.tsx      # Studio screenshot/demo
│   ├── MultiAgent.tsx          # Multi-agent workflow explanation
│   ├── FlowDemo.tsx            # Interactive React Flow demo
│   ├── FlowDemoNodes.tsx       # Custom nodes for the demo
│   ├── ConfigExample.tsx       # YAML/JSON config example
│   ├── ValueProps.tsx          # Value proposition section
│   ├── GetStarted.tsx          # Call to action / getting started
│   ├── Nav.tsx                 # Navigation bar
│   └── Footer.tsx              # Footer
└── public/
    ├── cthulu-hero.jpg
    ├── cthulu-hero.png
    └── cthulu-logo.png
```

---

## Key Patterns

- **Server Components by default** -- Next.js 15 App Router; data fetching in server components
- **Tailwind CSS 4** -- utility-first styling, configured via `postcss.config.mjs`
- **Framer Motion** -- scroll animations, section transitions
- **React Flow** -- used in `FlowDemo.tsx` for an interactive pipeline visualization on the landing page
- **`next/link`** for internal links, **`next/image`** for images -- never use `<a>` or `<img>`

---

## Verification

```bash
npx nx build cthulu-site   # Next.js production build
```
