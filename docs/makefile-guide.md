# Makefile Guide

> Everything you need to build, run, and manage Cthulu from the command line.

---

## Quick Start

```bash
# Fresh clean build of everything
make clean-build

# Or just check it compiles first
make check
```

---

## Build Targets

| Command | What it does |
|---------|-------------|
| `make build` | Build the `cthulu` backend release binary |
| `make clean` | Wipe all build artifacts (`cargo clean`) |
| `make clean-build` | Clean + rebuild from scratch |
| `make check` | Run `cargo check` (fast compile check) |

---

## Run Targets

| Command | What it does |
|---------|-------------|
| `make run-backend` | Start the backend on `:8081` (dev profile) |

```bash
make run-backend
```

---

## Configuration

Override any variable on the command line:

```bash
make run-backend CTHULU_URL=http://localhost:9090
```

| Variable | Default | Description |
|----------|---------|-------------|
| `BACKEND_BINARY` | `./target/release/cthulu` | Path to backend binary |
| `CTHULU_URL` | `http://localhost:8081` | Backend API URL |

---

## Common Workflows

### First time setup
```bash
make clean-build        # Build everything fresh
```

### Daily development
```bash
make check              # Quick compile check after changes
make run-backend        # Start backend
```

### Release build
```bash
make clean-build        # Full clean rebuild
```

---

## Notes

- **Dev vs Release:** `make run-*` targets use the dev profile (faster compile, slower runtime). `make build*` targets produce optimized release binaries.
- **Port 8081:** Make sure nothing else is using `:8081` before running the backend. Kill squatters with `lsof -ti:8081 | xargs kill`.
