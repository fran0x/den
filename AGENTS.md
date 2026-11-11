# Repository Guidelines

## Project Structure & Module Organization

Den is a read-only Rust terminal inspector for a developer machine: Homebrew, Codex and
Claude Code, cloud profiles (AWS, Kubernetes), Docker and language toolchains.
Keep changes within the requested scope.

| Location | Responsibility |
| --- | --- |
| `src/main.rs`, `src/terminal.rs` | CLI arguments, terminal lifecycle and collection/event loop |
| `src/app.rs` | Homebrew application state, refresh and diagnostic requests |
| `src/presentation.rs`, `src/presentation/location.rs` | Shared section/inspector navigation and bounded per-section Back history |
| `src/ui.rs`, `src/ui/` | Rendering, layout, focused action hints and contextual help |
| `src/inventory.rs`, `src/source.rs` | Den resource/snapshot types and Den JSON loading |
| `src/homebrew.rs`, `src/homebrew/` | Homebrew normalization, bounded subprocesses and reconciliation tests |
| `src/graph.rs`, `src/explorer.rs`, `src/diagnostics.rs`, `src/diagnostics/` | Dependency evidence, tree projection and diagnostic interpretation |
| `src/skills.rs`, `src/skills/` | Skill models, bounded discovery, metadata, filesystem access and state |
| `src/codex.rs`, `src/codex/` | Configuration, instructions, privacy, MCP models/normalization and state |
| `src/aws.rs`, `src/kube.rs`, `src/docker.rs`, `src/toolchains.rs` (each with state) | Cloud, Docker and Toolchains collectors |

Synthetic data lives in `fixtures/`, integration tests in `tests/`, and the Unix
PTY smoke harness in `scripts/smoke_terminal.py`. Domain models stay in their existing
modules; do not funnel Codex/Skills models into the original snapshot resource type.
Browsing keeps input in the inventory and updates a passive information panel.
Tab switches areas (Homebrew / Agents / Cloud / Docker / Toolchains) and Shift+Tab sections. Agents shows Codex and
Claude Code side by side (`src/agents.rs` matches their results into rows; collectors and
privacy rules stay per tool). There are no inspectors: the Agents information panel stacks
every section and scrolls with PgUp/PgDn. Only Homebrew Diagnostics uses `[` / `]`.

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
- `cargo test`: run unit, integration and documentation tests; live and PTY cases are ignored.
- `cargo build --release`: build the optimized binary.
- `python3 scripts/smoke_terminal.py`: verify launch, input and restoration in a PTY;
  requires the release build, uses synthetic data and runs the ignored terminal lifecycle tests.
  Pass a binary path as the first argument when using a separate `CARGO_TARGET_DIR`.

`just check` composes formatting, Clippy, tests and release build. `just smoke` builds
and runs the offline PTY harness. `just ci` runs what CI runs (locked checks, smoke, Rust
1.90 and `cargo deny`); `just tools` verifies its prerequisites and installs nothing. Use `just smoke /path/to/release/den` with an external
`CARGO_TARGET_DIR`. Root `justfile` recipes are everyday commands; `just/fixtures.just`
contains offline launchers and `just/live.just` contains opt-in local checks. Each module
sets the working directory back to the repository root and lists commands by default.
Use `just fixtures demo`, `just fixtures snapshot`, `just fixtures brew`,
`just fixtures skills` or `just fixtures codex`; path parameters remain optional.
Live checks (`just live reconcile`, `just live dependencies`, `just live diagnostics`,
`just live skills`, `just live codex`, `just live mcp`, `just live terminal`) are opt-in;
do not include them in routine offline validation. The terminal recipe accepts an optional
release binary path too. When changing recipes, validate module defaults, working directories,
argument forwarding and the offline/live boundary without incidentally executing live checks.

## Coding Style & Naming Conventions

Use Rust 2024, rustfmt's four-space indentation, `snake_case` for modules/functions,
and `PascalCase` for types. Prefer explicit enums and small functions over frameworks.
Propagate recoverable I/O errors; reserve `unwrap` for tests. Clippy enforces this and
`// SAFETY:` comments on every `unsafe` block (`[lints]` in `Cargo.toml`, `clippy.toml`);
integration test files allow `unwrap` explicitly at the top. Keep dependencies minimal
and commit `Cargo.lock`. Review wrapped scrolling tests when upgrading pinned Ratatui.

## Testing Guidelines

Use Rust's built-in test framework and Ratatui `TestBackend`. Name tests for observable
behavior. Cover empty results, selection stability, focus, search/filter combinations,
resizing and hostile text. Add regression coverage for meaningful behavior changes;
there is no numeric coverage target. For code/navigation changes, run `just check`
and the offline smoke checks before submitting. For documentation-only changes, verify
referenced commands, paths, links and formatting; do not run live collectors as
incidental checks.

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
Docker uses the same runner (`homebrew::run_bounded`) for exactly the four listings in
`docker::COMMANDS`, only when the Docker area is opened; never add commands that change
state or read container internals (`inspect`, `logs`, `exec`).
Homebrew may fetch metadata despite disabled auto-update/analytics. Demo/snapshot modes
never run Homebrew or discover live Codex data; fixtures stay within their explicit roots.

From `~/.claude.json` keep only `mcpServers` and the selected project's MCP approvals,
toggles and trust flag; never surface its account or session data. Claude credential
helpers stay withheld and hooks are never run.

AWS: keep only section and key names from the credentials file, never values; show
config values only from the allowlist in `src/aws.rs`; never read SSO or CLI caches or
call AWS. Account IDs and role ARNs are shown in full by decision.

Kubernetes: read kubeconfig files only (`KUBECONFIG` or `~/.kube/config`); parse tokens,
passwords and certificate/key data as presence (`IgnoredAny`), never keep them. Keep only
scheme, address class and port of server URLs, and the program name and env variable names
of exec plugins. Never run `kubectl`, an exec plugin or contact a cluster.

Toolchains: read PATH folders, rustup settings, `.crates2.json` and package metadata only;
never run a program found on PATH, a toolchain or a package manager (versions come from
paths and metadata). Withhold git/private-registry URLs and local source paths of crates.

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
