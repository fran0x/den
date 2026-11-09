# UX-2A navigation walkthroughs

The earlier interface required F2/F3 plus a mixture of modes, focus and inspector
shortcuts. UX-2A uses one searchable Sections switcher and one explicit Back history.
The [UX-1 captures](../ux/README.md) remain frozen for comparison.
These walkthroughs describe the navigation introduced in UX-2A; the
[keyboard guide](../KEYBOARD.md) includes current UX-2B tree controls and search/quit behavior.

## 1. Homebrew → search → inspect → back

**Before:** `/ripgrep`, Enter, Enter opened the inspector; `v` changed display mode.
Esc and filter reset depended on the current focus.

**After:** `/ripgrep`, Enter finishes search; Enter opens Overview. Esc restores the
same filtered inventory and selected package. For technical fields, Tab selects the
inspector section list, then choose Evidence and Enter. Esc returns to Overview.

## 2. Dependencies → follow package → back

**Before:** remember `2` for dependencies, Enter to follow, Backspace to return.

**After:** inspect the package, Tab to sections, select Dependencies and Enter.
Select a related package and Enter. Esc restores the original package's dependency
section, selected relationship, filters, scroll and existing expansion state.
Recorded runtime/current declaration distinctions and installation intent are unchanged.

## 3. Homebrew → Skills → Configuration → MCP

**Before:** F2 switched modules; repeated F3 presses cycled Codex views.

**After:** `g`, `/skills`, Enter. Then `g`, `/configuration`, Enter. Then `g`, `/mcp`,
Enter. Every destination is visible in the switcher; returning restores its saved
context. No function keys or permanent sidebar are needed.

## 4. Evidence → return to the original position

**Before:** `v`, full-screen state and pane focus interacted independently.

**After:** Enter opens Overview. Tab selects inspector sections; choose Evidence
and Enter. Scroll with arrows or PageDown. Esc restores the prior overview position;
another Esc returns to the original inventory. Source jumps use the same history.

## Synthetic terminal captures

- [Homebrew inventory](homebrew-after.txt) · [Skills inventory](skills-after.txt)
- [Section switcher, 80×24](sections-80x24.txt)
- [Resource inspector, 120×32](inspector-120x32.txt)
- [Evidence, 80×24](evidence-80x24.txt)

## Design and verification

[Tau](https://tau-agent.dev/) illustrates on-demand discovery and compact presentation;
[Herdr](https://github.com/herdrdev/herdr) emphasizes explicit selection and keyboard
navigation. Den uses those principles without importing their runtime architecture.

A persistent sidebar was considered and omitted: the same switcher works at 80×24,
120×32 and 160×45. Inspector sections remain visible beside content when space allows.
Deterministic UI tests and PTY checks cover these workflows, privacy and terminal
restoration. The following commands render the current implementation and overwrite
the synthetic text captures in `docs/ux2/` (they do not recreate historical layouts).
Run them from the repository root only when intentionally refreshing those captures:

```sh
cargo test --test ux captures -- --ignored
cargo test --test navigation navigation_captures -- --ignored
```

These captures document UX-2A. UX-2B adds tree-first dependency exploration;
see the [current dependency controls](../KEYBOARD.md#dependency-trees-ux-2b).
