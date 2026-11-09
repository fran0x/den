# UX-1 terminal captures

These are deterministic Ratatui TestBackend captures at **120 × 32**, using only
committed synthetic fixtures. They represent terminal cells; ANSI styling is omitted.
They are historical UX-1 captures, not a preview of the current interface. UX-2A removed
the Compact/Detailed toggle; UX-2B added tree-first dependency controls. Use the
[keyboard guide](../KEYBOARD.md) for current behavior.
The former absolute fixture-root prefix is replaced by `<repo>` in the before capture.
No live inventory, private configuration or instruction content is included.

| Inventory | Before | After |
| --- | --- | --- |
| Homebrew | [Four-line entries](homebrew-before.txt) | [Single-row inventory](homebrew-after.txt) |
| Skills | [Technical overview](skills-before.txt) | [Eligibility and summary](skills-after.txt) |

The redesign uses the compact/detailed distinction described by [Tau](https://tau-agent.dev/)
and the visible navigation/focus emphasis of [Herdr](https://github.com/herdrdev/herdr).
Den retains its own domain models, workers and keyboard-driven explorers.

Current capture commands write to `docs/ux2/`; they do not recreate these frozen
UX-1 before/after files. See the [capture instructions](../ux2/README.md#design-and-verification)
before intentionally updating generated documentation.

Automated UI and PTY tests also cover **80 × 24** and **160 × 45**, full-screen reading,
redaction, search, source jumps, help and returning without losing context. Current
browsing stacks passive information below the inventory at narrow widths. Explicit
inspectors use `[` / `]` to switch full-width panes below 100 columns. Text columns
are capped at 92 cells. In the current interface, `?` documents specialist actions
and `v` opens Evidence.
