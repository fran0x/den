# UX-1 terminal captures

These are deterministic Ratatui TestBackend captures at **120 × 32**, using only
committed synthetic fixtures. They represent terminal cells; ANSI styling is omitted.
The former absolute fixture-root prefix is replaced by `<repo>` in the before capture.
No live inventory, private configuration or instruction content is included.

| Inventory | Before | After |
| --- | --- | --- |
| Homebrew | [Four-line entries](homebrew-before.txt) | [Single-row inventory](homebrew-after.txt) |
| Skills | [Technical overview](skills-before.txt) | [Eligibility and summary](skills-after.txt) |

The redesign uses the compact/detailed distinction described by [Tau](https://tau-agent.dev/)
and the visible navigation/focus emphasis of [Herdr](https://github.com/herdrdev/herdr).
Den retains its own domain models, workers and keyboard-driven explorers.

Recreate the after captures from the repository root:

```sh
cargo test --test ux captures -- --ignored
```

Automated UI and PTY tests also cover **80 × 24** and **160 × 45**, full-screen reading,
redaction, search, source jumps, help and returning without losing context. Below
100 columns, Tab switches full-width panes instead of stacking them. Text columns
are capped at 92 cells. `?` documents specialist actions; `v` exposes technical evidence.
