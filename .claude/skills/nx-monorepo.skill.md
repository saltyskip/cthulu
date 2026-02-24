---
name: nx-monorepo
description: Use when running builds, tests, or lints across the Cthulu monorepo -- understanding workspace structure, project dependencies, and Nx task orchestration.
---

# Nx Monorepo Management (Cthulu)

## When to Apply

- Running builds, tests, or lints
- Understanding project dependencies
- Adding new projects or configuring targets

## Workspace Structure

| Project | Type | Tech | Port | Nx Plugin |
|---------|------|------|------|-----------|
| `cthulu` (root) | Rust backend | Axum, Tokio | 8081 | Custom cargo targets |
| `cthulu-studio` | Desktop app | React 19, Vite, Tauri | 5173 | `@nx/vite` |
| `cthulu-site` | Marketing site | Next.js 15, Tailwind 4 | 3000 | `@nx/next` |

`cthulu-studio` has an implicit dependency on `cthulu` (needs the API running).

## Essential Commands

| Task | Command |
|------|---------|
| Start backend + Studio | `npm run dev` (uses `scripts/dev.sh`) |
| Start all 3 projects | `npm run dev:all` |
| Build Rust backend | `npx nx build cthulu` (cargo build --release) |
| Dev Rust backend | `npx nx dev cthulu` (cargo run -- serve) |
| Test Rust backend | `npx nx test cthulu` (cargo test) |
| Lint Rust backend | `npx nx lint cthulu` (cargo clippy) |
| Build Studio | `npx nx build cthulu-studio` |
| Dev Studio | `npx nx dev cthulu-studio` |
| Build Site | `npx nx build cthulu-site` |
| Dev Site | `npx nx dev cthulu-site` |
| Dependency graph | `npm run graph` or `npx nx graph` |

## Project Tags

- `cthulu`: `scope:backend`, `lang:rust`
- `cthulu-studio`: `scope:frontend` (inferred)
- `cthulu-site`: `scope:site` (inferred)

## Cargo Targets (project.json)

The Rust backend uses custom Nx targets that shell out to `cargo`:

```json
{
  "build": { "command": "cargo build --release" },
  "dev":   { "command": "cargo run -- serve", "continuous": true },
  "test":  { "command": "cargo test" },
  "lint":  { "command": "cargo clippy -- -D warnings" }
}
```

## Named Inputs

- `default` -- all project files + shared globals
- `production` -- excludes test files
- `rust` -- `src/**/*`, `Cargo.toml`, `Cargo.lock` (used by backend targets)

## Dev Startup Sequence

`scripts/dev.sh` starts the backend first, waits for health on `:8081`, then starts the Studio. This ensures the API is available before the frontend tries to connect.
