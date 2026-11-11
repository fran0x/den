# Development

`just check` runs formatting, Clippy, tests and release build; `just smoke` checks terminals.
Bare `just` lists everyday commands: `run`, `build`, `fmt`, `lint`, `test`, `check`
and `smoke`. `just fixtures` lists offline launchers; `just live` lists opt-in local
checks. Listing either module does no collection or launching. Recipes run from the
repository root, including when invoked from a subdirectory.
The equivalent checks are `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release`,
then `python3 scripts/smoke_terminal.py`. The normal test suite leaves live checks,
capture regeneration and PTY-only tests ignored; the smoke harness runs the terminal tests.
Opt-in local checks: `just live reconcile`, `just live dependencies`, `just live diagnostics`,
`just live skills`, `just live codex`, `just live mcp` (aggregate local evidence).
`just live terminal` also launches the live TUI and cleanup preview in a PTY; it is opt-in.
With a separate `CARGO_TARGET_DIR`, pass the release binary explicitly:
`just smoke /path/to/target/release/den` or `just live terminal /path/to/target/release/den`.
For documentation-only edits, verify commands and links without collecting live data
or regenerating captures.

CI (`.github/workflows/ci.yml`) runs the same checks on Ubuntu and macOS for every push to
`main` and every pull request: formatting, Clippy, tests, release build and the offline
terminal smoke checks. Live checks never run in CI.

Security (`.github/workflows/security.yml`) runs `cargo deny check` on the same triggers and
every Monday: RustSec advisories (including unmaintained and yanked crates), allowed
licenses and crates.io as the only source, as set in `deny.toml`. Dependabot opens weekly
grouped updates for crates and for the GitHub Actions, which are pinned to commits.
