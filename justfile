set positional-arguments

# Offline demo and snapshot launchers.
mod fixtures 'just/fixtures.just'
# Opt-in local evidence checks; no live checks run by default.
mod live 'just/live.just'

# List available commands.
[private]
default:
    @just --list

# Run Den; forward optional CLI arguments.
run *args:
    cargo run -- "$@"

# Build the release binary.
build:
    cargo build --release

# Format Rust source.
fmt:
    cargo fmt --all

# Check formatting without modifying files.
_fmt-check:
    cargo fmt --all -- --check

# Run Clippy with warnings treated as errors.
lint:
    cargo clippy --all-targets -- -D warnings

# Run tests; forward optional Cargo test arguments.
test *args:
    cargo test "$@"

# Run formatting, lint, tests and a release build.
check: _fmt-check lint test build

# Run offline PTY checks (macOS/Linux; requires Python 3).
smoke binary="target/release/den": build
    python3 scripts/smoke_terminal.py "$1"
