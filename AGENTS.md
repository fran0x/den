# Repository Guidelines

## Project Structure & Module Organization

Den is a Rust terminal inspector. Keep resource types in `src/inventory.rs`, loading
in `src/source.rs`, interaction state in `src/app.rs`, rendering in `src/ui.rs`, and
terminal lifecycle in `src/terminal.rs`. `src/main.rs` handles CLI arguments.
Synthetic data lives in `fixtures/`; integration tests live in `tests/` and the Unix
PTY smoke harness in `scripts/`. Follow `ROADMAP.md`; implement only the requested stage.

## Build, Test, and Development Commands

- `cargo run`: launch the embedded synthetic inventory.
- `cargo run -- --snapshot fixtures/demo.json`: load a local snapshot.
- `cargo fmt --all -- --check`: verify standard Rust formatting.
- `cargo clippy --all-targets -- -D warnings`: reject lint warnings.
- `cargo test --all-targets`: run state, loading and rendering tests.
- `cargo build --release`: build the optimized binary.
- `python3 scripts/smoke_terminal.py`: verify launch, input and restoration in a PTY;
  requires the release build and runs the otherwise ignored terminal lifecycle tests.

## Coding Style & Naming Conventions

Use Rust 2024, rustfmt's four-space indentation, `snake_case` for modules/functions,
and `PascalCase` for types. Prefer explicit enums and small functions over frameworks.
Propagate recoverable I/O errors; reserve `unwrap` for tests. Keep dependencies minimal
and commit `Cargo.lock`. Review wrapped scrolling tests when upgrading pinned Ratatui.

## Testing Guidelines

Use Rust's built-in test framework and Ratatui `TestBackend`. Name tests for observable
behavior. Cover empty results, selection stability, focus, search/filter combinations,
resizing and hostile text. Add regression coverage for meaningful behavior changes;
there is no numeric coverage target. Run all checks above before submitting.

## Commit & Pull Request Guidelines

No Git history exists yet to establish a convention. Use concise imperative subjects
such as `Add snapshot validation`. Keep changes scoped. PRs should explain behavior,
design choices, validation results and limitations; link relevant issues and include
terminal screenshots for visible layout changes.

## Security & Scope

Den stays read-only. Stage 0 must not spawn processes, access the network or inspect
environment secrets. Sanitize every untrusted string before terminal display. Treat
snapshot claims as unverified; never infer trust from installation alone. Preserve
terminal cleanup on errors and panics. Do not add collectors without explicit scope.
