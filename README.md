# Den

A read-only Rust TUI for Homebrew inventory, dependency diagnostics and local Codex
skills, configuration, instructions and MCP server definitions. Nothing inspected is installed, removed or executed.

## Run

Stable Rust on macOS/Linux; live Homebrew needs `brew` on `PATH`. `just` is optional.

```sh
just run                                  # Homebrew
just run --project /path/to/repo --skills  # Codex; defaults to current directory
just run --skills --codex-profile review  # inspect review.config.toml
just demo
just snapshot                             # Den JSON
just brew-snapshot fixtures/brew.json
just skills-fixture
just codex-fixture                        # isolated synthetic configuration
just codex-fixture fixtures/mcp           # synthetic MCP definitions
```

Snapshots/demo never collect live Codex data or run Homebrew. Fixtures use explicit roots.

## Navigation

`F2` switches Homebrew/Codex; `F3` cycles views. `/` searches; arrows or `j/k` select;
`Tab` switches panes; `Enter` opens a full-screen inspector; `Esc` returns.
`v` toggles Compact/Detailed; `z` expands a focused pane; `?` opens scrollable help.
`r` refreshes; `q` quits. Mac keyboards may require `Fn` with function keys.

Compact is the default: one row per resource and a short overview. Detailed retains
technical evidence without exposing withheld values. Below 100 columns, only the
focused pane is shown; wider terminals show inventory and overview together.

Number keys retain the existing detail sections. `Shift+V` now selects a Homebrew keg
or MCP source. `Backspace` returns from related packages, files and source definitions.
See [all keyboard controls](docs/KEYBOARD.md) and [before/after captures](docs/ux/README.md).

## Evidence and privacy

Homebrew intent uses installation records; unknown stays unknown. Runtime relationships
and current declarations stay separate. Diagnostics never establish safe removal.
Homebrew may fetch metadata.

Codex models [local precedence](https://developers.openai.com/codex/config-advanced):
trusted project, selected profile, user and system files. Untrusted project layers are
inactive; unknown trust is conditional. Runtime overrides remain unobserved;
unsupported values and unestablished merge/path semantics remain unresolved.

[Instruction chains](https://developers.openai.com/codex/guides/agents-md) combine one selected
file per directory, global-to-project, with byte accounting and best-effort redacted previews.

MCP inspection follows the [current schema](https://learn.chatgpt.com/docs/config-file/config-reference).
Stdio/HTTP definitions retain source and trust evidence. Multi-layer merges remain unresolved;
plugin overrides do not establish an installed server. Policies restrict configured tool names,
not a discovered tool inventory. Authentication availability is never checked.

TOML uses a display allowlist. Arguments, inline environment/header values, OAuth secrets,
helper commands and raw URLs are withheld. Debug output is aggregate only; endpoints show
scheme, network category and port. No credential stores or sessions are read.

Reads and traversal are bounded; configuration file symlinks are rejected. MCP limits:
1,024 definitions, 256 entries per collection field. `url` parses endpoints without DNS.
System/plugin coverage remains partial; eligible never means connected or invoked.
See the roadmap for filesystem limits and deliberate modeling gaps.

## Development

`just check` runs formatting, Clippy, tests and release build; `just smoke` checks terminals.
Opt-in local checks: `just reconcile`, `just dependency-check`, `just diagnostic-check`,
`just skills-check`, `just codex-config-check`, `just mcp-check` (aggregate local evidence).

See [ROADMAP.md](ROADMAP.md) and [AGENTS.md](AGENTS.md). UX-1 complete; deeper MCP security auditing is deferred.
