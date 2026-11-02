set positional-arguments

# List available commands.
default:
    @just --list

# Run Den; forward optional CLI arguments.
run *args:
    cargo run -- "$@"

# Launch the embedded synthetic inventory.
demo:
    cargo run -- --demo

# Load a Den snapshot.
snapshot path="fixtures/demo.json":
    cargo run -- --snapshot "$1"

# Load raw Homebrew JSON without invoking brew.
brew-snapshot path="fixtures/brew.json":
    cargo run -- --brew-snapshot "$1"

# Build the release binary.
build:
    cargo build --release

# Format Rust source.
fmt:
    cargo fmt --all

# Check formatting without modifying files.
fmt-check:
    cargo fmt --all -- --check

# Run Clippy with warnings treated as errors.
lint:
    cargo clippy --all-targets -- -D warnings

# Run tests; forward optional Cargo test arguments.
test *args:
    cargo test "$@"

# Run formatting, lint, tests and a release build.
check: fmt-check lint test build

# Run offline PTY checks (macOS/Linux; requires Python 3).
smoke: build
    python3 scripts/smoke_terminal.py

# Reconcile counts, installed versions and intent against one live Homebrew response.
reconcile:
    cargo test live_homebrew_matches_installation_records -- --ignored --nocapture

# Compare representative dependencies with read-only Homebrew commands (opt-in).
dependency-check:
    cargo test live_dependency_examples -- --ignored --nocapture

# Reconcile dependency gaps, missing checks, leaves and autoremove --dry-run (read-only).
diagnostic-check:
    cargo test live_diagnostic_categories -- --ignored --nocapture

# Inspect local skills; print counts and validation categories without private contents.
skills-check:
    cargo test --test skills live_skills_inventory -- --ignored --nocapture

# Browse an isolated synthetic skills directory, without Homebrew or real skill roots.
skills-fixture path="fixtures/skills":
    cargo run -- --skills-fixture "$1"

# Opt-in reconciliation and terminal verification against local Homebrew.
live-check: build reconcile
    python3 scripts/smoke_terminal.py target/release/den --live

# Inspect local configuration; aggregate counts only, no raw values or instructions.
codex-config-check:
    cargo test --test codex_config live_codex_configuration -- --ignored --nocapture

# Inspect isolated synthetic configuration, instructions and skills.
codex-fixture path="fixtures/codex":
    cargo run -- --codex-fixture "$1"

# Aggregate configured MCP evidence only; never launch, connect or authenticate.
mcp-check:
    cargo test --test mcp live_mcp_configuration -- --ignored --nocapture
