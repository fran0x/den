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

## Releasing

1. Update `version` in `Cargo.toml` and move the `Unreleased` notes in `CHANGELOG.md`
   under the new version.
2. Run `just check`, `just smoke` and `cargo publish --dry-run`.
3. Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.

The Release workflow checks that the tag matches `Cargo.toml`, builds `den` for macOS
(Apple Silicon and Intel) and Linux (x86_64 and arm64), and attaches the archives and
their SHA-256 sums to a GitHub release.

The crate is published as `den-tui` (the name `den` is taken on crates.io); it installs
the `den` command. The first version is published by hand with `cargo publish`. After
that, add GitHub trusted publishing for `fran0x/den`, workflow `release.yml`, environment
`crates-io` in the crate's settings on crates.io, create the `crates-io` environment in the
repository, and set the repository variable `PUBLISH_CRATE` to `true`. Each release tag
then publishes without a stored token.
