## Cthulu project Makefile
##
## Common targets:
##   make build              Build the backend binary (release)
##   make clean              Remove all build artifacts (cargo clean)
##   make clean-build        Clean + rebuild (release)
##   make check              Run cargo check on all targets
##   make run-backend        Run the backend locally (cargo run)
##   make help               Show this help

# ---------------------------------------------------------------------------
# Configurable variables (override on command line or in environment)
# ---------------------------------------------------------------------------

# Absolute path to the binary
BACKEND_BINARY ?= $(shell pwd)/target/release/cthulu

# Cthulu backend URL
CTHULU_URL ?= http://localhost:8081

# ---------------------------------------------------------------------------
# Phony targets
# ---------------------------------------------------------------------------
.PHONY: help build clean clean-build check run-backend

# ---------------------------------------------------------------------------
# help
# ---------------------------------------------------------------------------
help:
	@echo ""
	@echo "Usage: make <target> [VAR=value ...]"
	@echo ""
	@echo "Build targets:"
	@echo "  build              Build the backend release binary"
	@echo "  clean              Remove all build artifacts (cargo clean)"
	@echo "  clean-build        Clean + rebuild (release)"
	@echo "  check              Run cargo check on all targets"
	@echo ""
	@echo "Run targets:"
	@echo "  run-backend        Run backend locally on :8081 (dev profile)"
	@echo ""
	@echo "Variables (current values):"
	@echo "  BACKEND_BINARY = $(BACKEND_BINARY)"
	@echo "  CTHULU_URL     = $(CTHULU_URL)"
	@echo ""

# ---------------------------------------------------------------------------
# build — build the backend release binary
# ---------------------------------------------------------------------------
build:
	cargo build --release --bin cthulu
	@echo ""
	@echo "Binary: $(BACKEND_BINARY)"

# ---------------------------------------------------------------------------
# clean — remove all build artifacts
# ---------------------------------------------------------------------------
clean:
	cargo clean
	@echo ""
	@echo "Build artifacts removed."

# ---------------------------------------------------------------------------
# clean-build — clean + rebuild
# ---------------------------------------------------------------------------
clean-build: clean build

# ---------------------------------------------------------------------------
# check — cargo check all targets
# ---------------------------------------------------------------------------
check:
	cargo check --bin cthulu
	@echo ""
	@echo "All targets compile."

# ---------------------------------------------------------------------------
# run-backend — run the backend locally (dev profile)
# ---------------------------------------------------------------------------
run-backend:
	cargo run --bin cthulu -- serve
