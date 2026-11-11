# Every check here is opt-in. Local metadata is inspected, never modified.
# Homebrew may fetch metadata; MCP servers, helpers and skill scripts are never run.
set positional-arguments
set working-directory := '..'

# List the opt-in checks; never inspect the machine by default.
[private]
default:
    @just --list live

# Reconcile live Homebrew counts, installed versions and intent; aggregate output only.
reconcile:
    cargo test live_homebrew_matches_installation_records -- --ignored --nocapture

# Compare representative live dependencies with read-only Homebrew commands.
dependencies:
    cargo test live_dependency_examples -- --ignored --nocapture

# Reconcile live dependency gaps, leaves and autoremove dry-run evidence; no removal.
diagnostics:
    cargo test live_diagnostic_categories -- --ignored --nocapture

# Inspect local skills; print counts and duplicate names, not instructions or paths.
skills:
    cargo test --test skills live_skills_inventory -- --ignored --nocapture

# Inspect local configuration; aggregate counts, not raw values or instructions.
codex:
    cargo test --test codex_config live_codex_configuration -- --ignored --nocapture

# Aggregate local MCP declarations; never launch, connect or authenticate.
mcp:
    cargo test --test mcp live_mcp_configuration -- --ignored --nocapture

# Reconcile Homebrew and exercise the live TUI and cleanup preview in a PTY.
terminal binary="target/release/den": _build reconcile
    python3 scripts/smoke_terminal.py "$1" --live

# Reuse the root release build without duplicating its Cargo command.
_build:
    @just build
