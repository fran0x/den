# Den

A read-only Rust TUI for Homebrew inventory, dependency diagnostics and local Codex
skills, configuration and instructions. Nothing inspected is installed, removed or executed.

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
```

Snapshots/demo never collect live Codex data or run Homebrew. Fixtures use explicit roots.

## Navigation

`F2`: Homebrew/Codex. `F3`: Skills/Configuration/Instructions. Arrows or `j/k` navigate;
`Tab` changes pane; `/` searches; `r` refreshes; `?` helps; `q` quits.

- **Homebrew:** `f/i` filter; `1–4` dependency views; `5–8` diagnostics. `s/v/t/Space`
  select evidence, keg, tree and expansion. `Enter/Backspace` jump/return; `c` checks.
- **Skills:** `f/i` filter scope/validation; `1–5` details; `Enter` previews files;
  `e` opens enablement provenance, `Backspace` returns.
- **Configuration:** `s` switches settings/sources; `Enter` opens a safe source summary.
- **Instructions:** `Enter` toggles redacted previews; `t` shows the conditional trusted
  scenario. Page keys and Home/End scroll. Views preserve selection and search.

## Evidence and privacy

Homebrew intent uses installation records; unknown stays unknown. Runtime relationships
remain separate from current declarations. `missing`, `leaves` and `autoremove --dry-run`
are supplementary evidence, never removal advice. Homebrew may fetch metadata.

Codex inspection models [local precedence](https://developers.openai.com/codex/config-advanced):
trusted project layers, selected separate profile, user and system files. Untrusted
project layers are inactive; unknown trust is conditional. CLI, cloud and managed policy
remain unobserved. Legacy profiles, unknown merge semantics and unsupported values are
flagged. Cross-layer skill arrays and non-project relative skill paths remain undetermined.

[Instruction chains](https://developers.openai.com/codex/guides/agents-md) select one nonempty
file per directory and combine global-to-project guidance. Byte accounting includes
blank-line separators. Den does not interpret prose conflicts.

TOML values use a display allowlist; raw files and parser snippets are never shown.
Instruction previews have best-effort redaction. Reads reject special files and file
symlinks; explicit discovery-root aliases are resolved safely. Limits: 256 KiB/file,
64 ancestors, 16 fallback names, 16 MiB instruction reads and 2 MiB previews per scenario.
Fallbacks currently require simple `.md` names. Blocking OS reads cannot be forcibly interrupted.
No credentials, sessions or agents are accessed. System/plugin skill coverage remains partial;
eligible does not mean loaded or invoked.

## Development

`just check` runs formatting, Clippy, tests and release build; `just smoke` checks terminals.
Opt-in local checks: `just reconcile`, `just dependency-check`, `just diagnostic-check`,
`just skills-check`, `just codex-config-check` (aggregate counts, no raw configuration).

See [ROADMAP.md](ROADMAP.md) and [AGENTS.md](AGENTS.md). Stage 3B only; MCP exploration is deferred.
