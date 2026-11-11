# Den

A read-only Rust TUI for Homebrew inventory, dependency diagnostics, local Codex
skills, configuration, instructions and MCP server definitions, and Claude Code skills,
instructions, settings, MCP servers, hooks and plugins, AWS CLI profiles, and Docker
containers, images and volumes. Den runs a fixed set
of read-only Homebrew commands; it never installs/removes tools, runs inspected skill
scripts or starts MCP servers.

## Run

Run from the repository root with stable Rust (edition 2024) on macOS/Linux.
Live Homebrew needs `brew` on `PATH`; `just` is optional.

```sh
just run                                  # live Homebrew; Agents loads when opened
just run --project /path/to/repo --skills  # start in Agents; defer Homebrew
just run --skills --codex-profile review  # inspect review.config.toml
just fixtures demo
just fixtures snapshot                    # Den JSON
just fixtures brew fixtures/brew.json
just fixtures skills
just fixtures codex                       # isolated synthetic configuration
just fixtures codex fixtures/mcp          # synthetic MCP definitions
just fixtures claude                      # synthetic Codex + Claude Code side by side
just fixtures aws                         # synthetic AWS profiles in Cloud
just fixtures docker                      # saved synthetic Docker listings
just fixtures brew fixtures/brew-dependencies.json # synthetic dependency trees
```

Without `just`, use `cargo run -- --demo`, `cargo run -- --skills` or pass the
corresponding CLI flags after `cargo run --`. `cargo run -- --help` lists the flags.
`--project` defaults to the current directory; `--codex-profile review` explicitly
selects `$CODEX_HOME/review.config.toml` (normally `~/.codex/review.config.toml`).
No active Codex session/profile is inferred.

Snapshots/demo never collect live Codex data or run Homebrew. Skills fixtures inspect
only their explicit skill roots; Codex fixtures inspect isolated configuration,
instructions, skills and MCP definitions. Fixture refresh rereads those roots only.

## Navigation

The top bar shows **Homebrew**, **Agents**, **Cloud** and **Docker**; the active area is
highlighted and followed by its section. Tab switches area; Shift+Tab switches section
(Homebrew: Packages / Diagnostics; Agents: Skills / Instructions / Settings / MCP / Hooks /
Plugins; Cloud: AWS; Docker: Containers / Images / Volumes).
Each area and section keeps its place. There is no section switcher.

**Agents** shows Codex and Claude Code side by side. Every list has the same columns:
name, **Tools** (Both, Codex only or Claude only) and **Status**, in one vocabulary for
both tools: Ready, Off, Needs approval, Problem or Unknown (Instructions: Used, Not used, Problem or Unknown). When both tools have a row
and agree, one status is shown; otherwise `Codex x · Claude y`. Skills match by folder
(Claude usually reaches `~/.agents/skills` through symlinks); MCP servers match by name,
and **Both, differ** marks a name that points to a different kind of server or program;
instructions pair `AGENTS.md` and `CLAUDE.md` by where they apply; settings match by name
and show the value. Hooks and Plugins are Claude Code only. The panel keeps each tool's
own words (Eligible, Pending approval, …) next to the shared status.

The inventory keeps focus while browsing. One line above the list shows its view, filters
and search. Selection updates an information panel beside it, or below it on narrow
terminals. `/` searches; Enter or Esc finishes editing and keeps the query.

Homebrew starts with **Tools**: packages Homebrew recorded as installed on request.
`a` switches between Tools and **All**; tools are bold and casks are bracketed, for
example `[firefox]`. Counts follow the current search. Packages without a request record appear only in All;
Unknown intent is never guessed. Rows show intent and **required by N**: other installed
packages with recorded runtime references to an installed version. Current declarations,
self references and unresolved versions do not count; no count does not mean unused.

Packages have no inspector: the information panel is the package view. It shows
**Depends on** and **Required by** trees, opened automatically to two levels. Uncertain
records stay labeled leaves; `PARTIAL` / `…` marks omitted information. A package without
recorded runtime metadata shows current declarations for that package only.

There is no inspector anywhere. In Agents the information panel shows everything about
the selection, each tool under its own heading (for a skill: each tool's status, then
Findings, Instructions, Supporting files, Evidence and coverage; for an MCP server:
Codex's sources with transport, authentication and tools, then Claude Code's entries).
PgUp/PgDn scroll it. Esc goes back within the
current section; switching areas or sections never adds a Back step. `?` gives contextual help; `r` refreshes the configured source; `q`
quits. While editing any search, `q` is text and Ctrl-C quits.

Claude Code skills come from `~/.claude/skills` and the project's `.claude/skills`.
Claude Code instructions are `CLAUDE.md` files in reading order:
managed, user (`~/.claude/CLAUDE.md`), then `CLAUDE.md`, `.claude/CLAUDE.md` and
`CLAUDE.local.md` from the outermost parent folder down. Previews are redacted; `@imports`
are counted, not followed; linked files are reported, not read.

Claude **Settings** resolves managed > local > project > user files (command-line
`--settings` is not observed); lists such as `permissions.allow` combine, and unreadable
files are flagged. **MCP** lists servers from `managed-mcp.json`, `~/.claude.json`, the
project's `.mcp.json` and enabled plugins, with collisions, approvals and trust; URLs,
arguments and values are withheld. **Hooks** lists every hook from settings and enabled
plugins, with commands redacted. **Plugins** lists installed plugins, whether settings
enable them, and their components. From `~/.claude.json` Den keeps only MCP servers, this
project's MCP approvals and its trust flag; account and session data are discarded.
Nothing is run.

**Cloud › AWS** lists profiles from `~/.aws/config` and `~/.aws/credentials` (or
`AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`), then SSO sessions, as Profile · Auth
(Role, SSO, Process, Static keys, None set) · Status (Ready with the region, or Problem:
a missing `source_profile` or `sso-session`, a role loop, an SSO profile without account
or role, a credentials file other users can read). The panel shows region, account ID,
role ARN, role chain and where each profile is defined; `AWS_PROFILE` marks the active
one. From the credentials file only profile and key names are kept; values are discarded
as they are read. SSO start URLs, endpoints, external IDs and `credential_process`
commands are withheld; SSO and CLI caches are never read; no AWS call is made.

**Docker** lists containers (state, image, Compose project), images and volumes (which
containers use them, or Unused; dangling images and anonymous volumes are marked), with
disk use and reclaimable space from `docker system df`. Den runs only `docker container ls
--all`, `docker image ls`, `docker volume ls` and `docker system df`, through the same
bounded runner as Homebrew, and only once the Docker area is opened; nothing is started,
stopped, pulled, inspected or removed. Container commands, environment, labels (except the
Compose project) and Docker's error messages are not shown; from `~/.docker/config.json`
only the current context, registry names and credential helper are kept.

See [keyboard controls](docs/KEYBOARD.md). The [UX-2 walkthroughs](docs/ux2/README.md) are historical.

## Evidence and privacy

Homebrew intent uses installation records; unknown stays unknown. Runtime relationships
and current declarations stay separate. Diagnostics never establish safe removal.
Homebrew may fetch metadata.

Codex models supported settings from [local configuration layers](https://learn.chatgpt.com/docs/config-file/config-advanced):
trusted project, selected profile, user and system files. Untrusted project layers are
inactive; unknown trust is conditional. Runtime overrides remain unobserved;
unsupported values and unestablished merge/path semantics remain unresolved. When
applicable configuration is malformed, unavailable or a selected profile is missing,
inventory and Overview mark retained values/defaults as **provisional**. Named permission
profiles (`default_permissions` / `[permissions]`) are not currently modeled.

[Instruction chains](https://learn.chatgpt.com/docs/agent-configuration/agents-md) combine one selected
file per directory, global-to-project, with byte accounting and best-effort redacted previews.

MCP inspection models a supported subset of the [configuration schema](https://learn.chatgpt.com/docs/config-file/config-reference).
Stdio/HTTP definitions retain source and trust evidence. Multi-layer merges remain unresolved;
plugin overrides do not establish an installed server. Policies restrict configured tool names,
not a discovered tool inventory. Authentication availability is never checked.

TOML uses a display allowlist. Arguments, inline environment/header values, OAuth secrets,
helper commands and raw endpoint URLs are withheld. MCP debug output is aggregate only;
configured endpoints show scheme, network category and port. Credential stores and
sessions are not queried.

Skill-declared MCP URL fields show `[URL withheld]`, retaining presence without URL
validation or connection checks. Raw `agents/openai.yaml` previews are withheld; normalized
declarations remain available in Evidence. Instruction-chain previews use best-effort
redaction. `SKILL.md` and ordinary supporting-text previews are terminal-sanitized,
but may contain private prose; their contents are not universally secret-redacted.

Reads and traversal are bounded; configuration file symlinks are rejected. MCP limits:
1,024 definitions, 256 entries per collection field. `url` parses endpoints without DNS.
System/plugin coverage remains partial; eligible never means connected or invoked.
See the roadmap for filesystem limits and deliberate modeling gaps.

## Development

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

See [ROADMAP.md](ROADMAP.md) and [AGENTS.md](AGENTS.md). UX-2B complete; deeper MCP security auditing is deferred.
