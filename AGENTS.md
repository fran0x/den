# Repository Guidelines

## Project Structure & Module Organization

Den is a read-only Rust terminal inspector for Homebrew and local Codex evidence.
Keep changes within the requested scope; later stages in `ROADMAP.md` are proposals.

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/terminal.rs` | CLI arguments, terminal lifecycle and collection/event loop |
| `src/app.rs` | Homebrew application state, refresh and diagnostic requests |
| `src/presentation.rs`, `src/presentation/location.rs` | Shared section/inspector navigation and bounded Back history |
| `src/ui.rs`, `src/ui/` | Rendering, layout, focused action hints and contextual help |
| `src/inventory.rs`, `src/source.rs` | Den resource/snapshot types and Den JSON loading |
| `src/homebrew.rs`, `src/homebrew/` | Homebrew normalization, bounded subprocesses and reconciliation tests |
| `src/graph.rs`, `src/explorer.rs`, `src/diagnostics.rs`, `src/diagnostics/` | Dependency evidence, tree projection and diagnostic interpretation |
| `src/skills.rs`, `src/skills/` | Skill models, bounded discovery, metadata, filesystem access and state |
| `src/codex.rs`, `src/codex/` | Configuration, instructions, privacy, MCP models/normalization and state |

Synthetic data lives in `fixtures/`, integration tests in `tests/`, and the Unix
PTY smoke harness in `scripts/smoke_terminal.py`. Domain models stay in their existing
modules; do not funnel Codex/Skills models into the original snapshot resource type.

## Build, Test, and Development Commands

- `cargo run`: collect live Homebrew inventory; Codex is collected when opened.
- `cargo run -- --demo`: launch the embedded synthetic inventory, offline.
- `cargo run -- --skills`: start in Codex Skills; defer Homebrew until opened.
- `cargo run -- --project /path/to/repo --skills`: select the live Codex project context.
- `cargo run -- --snapshot fixtures/demo.json`: load a local snapshot.
- `cargo run -- --brew-snapshot fixtures/brew-dependencies.json`: explore synthetic dependencies.
- `cargo run -- --codex-fixture fixtures/codex`: inspect isolated synthetic Codex data.
- `cargo run -- --help`: print CLI help without entering the TUI or collecting data.
- `cargo fmt --all -- --check`: verify standard Rust formatting.
- `cargo clippy --all-targets -- -D warnings`: reject lint warnings.
- `cargo test`: run unit, integration and documentation tests; live/capture/PTY cases are ignored.
- `cargo build --release`: build the optimized binary.
- `python3 scripts/smoke_terminal.py`: verify launch, input and restoration in a PTY;
  requires the release build, uses synthetic data and runs the ignored terminal lifecycle tests.
  Pass a binary path as the first argument when using a separate `CARGO_TARGET_DIR`.

`just check` composes formatting, Clippy, tests and release build. `just smoke` builds
and runs the offline PTY harness. The `justfile` is the command reference. Live checks
(`reconcile`, `dependency-check`, `diagnostic-check`, `skills-check`, `codex-config-check`,
`mcp-check`, `live-check`) are opt-in; do not include them in routine offline validation.

## Coding Style & Naming Conventions

Use Rust 2024, rustfmt's four-space indentation, `snake_case` for modules/functions,
and `PascalCase` for types. Prefer explicit enums and small functions over frameworks.
Propagate recoverable I/O errors; reserve `unwrap` for tests. Keep dependencies minimal
and commit `Cargo.lock`. Review wrapped scrolling tests when upgrading pinned Ratatui.

## Testing Guidelines

Use Rust's built-in test framework and Ratatui `TestBackend`. Name tests for observable
behavior. Cover empty results, selection stability, focus, search/filter combinations,
resizing and hostile text. Add regression coverage for meaningful behavior changes;
there is no numeric coverage target. For code/navigation changes, run `just check`
and the offline smoke checks before submitting. For documentation-only changes, verify
referenced commands, paths, links and formatting; do not run live collectors or
regenerate capture files as incidental checks.

## Commit & Pull Request Guidelines

Existing history contains milestone and UX commits. Use concise imperative subjects
that describe the actual change, such as `Preserve dependency tree context`. Keep changes scoped.
Do not commit or push unless requested. PRs should explain behavior,
design choices, validation results and limitations; link relevant issues and include
terminal screenshots for visible layout changes.

## Security & Scope

Den stays read-only. Production subprocesses use the existing bounded Homebrew runner:
`brew info --json=v2 --installed`, `brew missing`, `brew leaves` and
`brew autoremove --dry-run`. Keep fixed arguments, time/output limits and cancellation;
never add removal/repair commands or execute inspected scripts, MCP servers or helpers.
Homebrew may fetch metadata despite disabled auto-update/analytics. Demo/snapshot modes
never run Homebrew or discover live Codex data; fixtures stay within their explicit roots.

Sanitize every untrusted string before terminal display. Preserve descriptor-relative,
bounded filesystem reads and configuration/instruction symlink rejection. Do not query
Codex credential/session stores. MCP configuration values use classified summaries;
skill-declared MCP URL fields retain presence only, and raw `agents/openai.yaml` previews
are withheld. Ordinary skill/supporting prose is not universally secret-redacted.

Keep incomplete applicable configuration visibly provisional. Preserve recorded/current
evidence and exact keg linkage; unknown intent, missing paths and dry-run eligibility
never establish safe removal. Navigation history stores view state, not old inventories
or workers; refresh must reject outdated graph expansion indices. Preserve terminal
cleanup on errors and panics. Add collectors or external integrations only within explicit scope.
