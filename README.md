# Den

A read-only Rust TUI for Homebrew inventory, dependency diagnostics and local Codex
skills, configuration, instructions and MCP server definitions. Den runs a fixed set
of read-only Homebrew commands; it never installs/removes tools, runs inspected skill
scripts or starts MCP servers.

## Run

Run from the repository root with stable Rust (edition 2024) on macOS/Linux.
Live Homebrew needs `brew` on `PATH`; `just` is optional.

```sh
just run                                  # live Homebrew; Codex loads when opened
just run --project /path/to/repo --skills  # start in Codex; defer Homebrew
just run --skills --codex-profile review  # inspect review.config.toml
just demo
just snapshot                             # Den JSON
just brew-snapshot fixtures/brew.json
just skills-fixture
just codex-fixture                        # isolated synthetic configuration
just codex-fixture fixtures/mcp           # synthetic MCP definitions
just brew-snapshot fixtures/brew-dependencies.json # synthetic dependency trees
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

`g` opens **Sections**. Press `/` to find a destination, arrows or `j/k` to select,
and Enter to open it. Function keys are optional compatibility shortcuts.

In either browse pane, Enter inspects the selected resource. `/` edits inventory search;
Enter or Esc finishes editing and retains the query. The inspector starts at **Overview**.
Tab selects its section list; arrows and Enter
open Dependencies, Files, Sources, Evidence or other available details. Esc returns
to the previous context, preserving selection, filters, scroll and dependency expansion.

Dependencies and Required by start as trees: Right expands or enters children;
Left collapses or selects the parent; Space toggles a branch. Enter inspects a
package and Esc restores the tree. `t` retains the direct-list alternative.

There are no Compact/Detailed modes. Technical information lives in **Evidence**.
Below 100 columns, Tab switches full-width inspector panes. `?` gives contextual
help; `r` refreshes the configured source; `q` quits, including from the section switcher.
While editing any search, `q` is text and Ctrl-C quits. No persistent sidebar consumes inventory space.

See [keyboard controls](docs/KEYBOARD.md) and the [four navigation walkthroughs](docs/ux2/README.md).

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
The equivalent checks are `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release`,
then `python3 scripts/smoke_terminal.py`. The normal test suite leaves live checks,
capture regeneration and PTY-only tests ignored; the smoke harness runs the terminal tests.
Opt-in local checks: `just reconcile`, `just dependency-check`, `just diagnostic-check`,
`just skills-check`, `just codex-config-check`, `just mcp-check` (aggregate local evidence).
`just live-check` also launches the live TUI and cleanup preview in a PTY; it is opt-in.
For documentation-only edits, verify commands and links without collecting live data
or regenerating captures.

See [ROADMAP.md](ROADMAP.md) and [AGENTS.md](AGENTS.md). UX-2B complete; deeper MCP security auditing is deferred.
