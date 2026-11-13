set positional-arguments
set shell := ["bash", "-uc"]

# Minimum supported Rust, as in Cargo.toml (`rust-version`) and the CI job.
msrv := "1.90.0"

# Offline demo and snapshot launchers.
mod fixtures 'just/fixtures.just'
# Opt-in local evidence checks; no live checks run by default.
mod live 'just/live.just'

# List available commands.
[private]
default:
    @just --list

# Run den, read-only; forward optional CLI arguments.
den *args:
    cargo run --bin den -- "$@"

# Run dew: den with Homebrew write actions allowed (x, then Enter on the chosen one).
dew *args:
    cargo run --bin dew -- "$@"

# Build the release binary.
build:
    cargo build --release

# Format Rust source.
fmt:
    cargo fmt --all

# Apply formatting and Clippy's automatic fixes; the only recipe that rewrites code.
fix:
    cargo fmt --all
    cargo clippy --fix --allow-dirty --all-targets

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

# Run what CI runs: locked checks, tests and build, smoke, minimum Rust, deny.toml.
ci: tools _fmt-check
    cargo clippy --locked --all-targets -- -D warnings
    cargo test --locked
    cargo build --locked --release
    python3 scripts/smoke_terminal.py target/release/den
    cargo +{{msrv}} check --locked --all-targets
    cargo deny check

# Verify the tools `ci` needs; installs nothing.
tools:
    @command -v python3 >/dev/null || { echo "missing: python3"; exit 1; }
    @command -v cargo-deny >/dev/null || { echo "missing: cargo-deny (cargo install --locked cargo-deny)"; exit 1; }
    @rustup toolchain list | grep -q '^{{msrv}}' || { echo "missing: Rust {{msrv}} (rustup toolchain install {{msrv}})"; exit 1; }
    @echo "tooling ready"
