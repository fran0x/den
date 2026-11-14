# Repository Guidelines

## Project Structure & Module Organization

Den is a Rust terminal inspector for a developer machine: Homebrew, Codex and Claude Code,
cloud profiles (AWS, Kubernetes), Docker and language toolchains. `den` is read-only; `dew`
is the same program with confirmed write actions (Homebrew, Docker and toolchains).
Keep changes within the requested scope.

| Location | Responsibility |
| --- | --- |
| `src/main.rs` | CLI arguments and choosing live or offline sources |
| `src/app.rs`, `src/listing.rs`, `src/status.rs` | Application state; shared list state (selection, search, refresh); the typed row status (Ready, Problem, Off, Installed) |
| `src/inventory.rs`, `src/source.rs` | Den resource/snapshot types and Den JSON loading |
| `src/tui/` | Terminal lifecycle and event loop (`terminal`), split escape sequences (`keys`), section/inspector navigation and Back history (`presentation`), rendering and help (`ui/`) |
| `src/areas/homebrew/` | Homebrew normalization, bounded subprocesses, dependency graph, tree projection, diagnostics and reconciliation tests |
| `src/areas/agents/` | Rows matched across Codex and Claude Code (`agents.rs`); `claude/`, `codex/`, `skills/` collectors |
| `src/areas/cloud/` | AWS and Kubernetes collectors |
| `src/areas/docker/`, `src/areas/toolchains/` | Docker and Toolchains collectors |

Each area module is `name.rs` beside a `name/` folder (no `mod.rs`; `clippy::mod_module_files` enforces it).

The `den` library is internal (published only as part of `den-tui`): every module is
`#[doc(hidden)]` and has no stable API; keep new modules hidden too.

Synthetic data lives in `fixtures/`, integration tests in `tests/`, and the Unix
PTY smoke harness in `scripts/smoke_terminal.py`. Domain models stay in their existing
modules; do not funnel Codex/Skills models into the original snapshot resource type.
Browsing keeps input in the inventory and updates a passive information panel.
Tab switches areas (Homebrew / Agents / Cloud / Docker / Toolchains) and Shift+Tab sections. Agents shows Codex and
Claude Code side by side (`src/areas/agents.rs` matches their results into rows; collectors and
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

README screenshots in `docs/images` come from the fixtures: `just screenshots` regenerates
them (`examples/screenshot.rs`); never edit the SVGs by hand.

Run `cargo publish --dry-run` with `--target-dir target/publish`: verifying the package in
the shared `target` points the normal build at `target/package` sources, so later builds
look fresh while the code has changed (fix with `cargo clean -p den-tui`).

Releases: follow the steps at the top of `.github/workflows/release.yml`; pushing a
`v*` tag builds both binaries, creates the GitHub release and publishes to crates.io.
Then `just tap` writes the Homebrew formula into a `fran0x/homebrew-tap` checkout; commit
and push it there.

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

Den is read-only by default. Writes live only in `src/actions.rs`: a closed `Action` enum
whose command lines are built from names checked by `Package::new`, a preview (dry run)
where the tool has one, and a run that needs a `Permit` (created only for the `dew` binary, in `cli`)
after an explicit Enter on the chosen action (uninstall
is always listed last). Writes use `homebrew::run_write`:
no shell, closed stdin, own process group, timeout, SIGTERM with a grace period before
SIGKILL. Never add `--force`, `--ignore-dependencies`, `--zap`, sudo or free-form
arguments; each new action needs known-answer tests of its command line, a smoke step
with a fake tool, and SECURITY.md/README/guide updates. The action log
(`$XDG_STATE_HOME/den/actions.log`) keeps time, command and result, never output.
Reads use the existing bounded Homebrew runner:
`brew info --json=v2 --installed`, `brew missing`, `brew leaves` and
`brew autoremove --dry-run`. Keep fixed arguments, time/output limits and cancellation;
never add removal/repair commands or execute inspected scripts, MCP servers or helpers.
Docker uses the same runner (`homebrew::run_bounded`) for exactly the four listings in
`docker::COMMANDS`, only when the Docker area is opened; never read container internals
(`inspect`, `logs`, `exec`). Docker writes are `dew` actions only (`container stop`,
`container rm`, `image rm`, `volume rm`), on IDs and names checked by `DockerObject`,
offered only for what nothing uses, and never with `-f`, `--force` or `prune`.
Homebrew may fetch metadata despite disabled auto-update/analytics. Den reads only the
modification times of `*.jws.json` in Homebrew's API cache (how old "outdated" is), never
their contents. Update notices use data already on the machine (Homebrew's, Codex's
`version.json`: `latest_version` and the check date only); never query a registry. Demo/snapshot modes
never run Homebrew or discover live Codex data; fixtures stay within their explicit roots.

From `~/.claude.json` keep only `mcpServers` and the selected project's MCP approvals,
toggles and trust flag; never surface its account or session data. Claude credential
helpers stay withheld and hooks are never run.

AWS: keep only section and key names from the credentials file, never values; show
config values only from the allowlist in `src/areas/cloud/aws.rs`; never read SSO or CLI caches or
call AWS. Account IDs and role ARNs are shown in full by decision.

Kubernetes: read kubeconfig files only (`KUBECONFIG` or `~/.kube/config`); parse tokens,
passwords and certificate/key data as presence (`IgnoredAny`), never keep them. Keep only
scheme, address class and port of server URLs, and the program name and env variable names
of exec plugins. Never run `kubectl`, an exec plugin or contact a cluster.

Toolchains: read PATH folders, rustup settings, `.crates2.json` and package metadata only;
reading never runs a program found on PATH, a toolchain or a package manager (versions come
from paths and metadata). Withhold git/private-registry URLs and local source paths of crates.
Toolchain writes are `dew` actions only, run by the full path of the copy that wins on PATH
(`Program::found`): `rustup update --no-self-update` (dry run `rustup check`, exit 100 means
an update), `rustup toolchain uninstall` (never the default or an overridden toolchain),
`cargo install --locked` (crates.io crates without chosen features) and `cargo uninstall`,
`npm update -g` / `npm uninstall -g` (dry run: the same with `--dry-run`; never npm or
corepack, which come with Node), `uv tool upgrade` / `uninstall` and `pipx upgrade` /
`uninstall`, on names checked by `Name`. No actions for uv Pythons, bun or go programs.
A PATH command Homebrew installed reuses that package's Homebrew menu (`homebrew_menu`).
Never `--force`, `rustup self`, `cargo install --force` or a command without a name.

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
