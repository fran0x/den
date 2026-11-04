# Keyboard guide

Den is read-only. Compact and Detailed change presentation only. Withheld configuration
values stay withheld in both modes and in full-screen inspectors.

## Common controls

| Key | Action |
| --- | --- |
| F2 | Switch Homebrew / Codex, retaining each view's state |
| F3 | Cycle Homebrew overview, relationships and diagnostics; or Codex Skills / Configuration / Instructions / MCP |
| / | Search the current inventory immediately; leaves full-screen inspection |
| Arrows, j/k | Select in inventory; navigate or scroll in details |
| Tab / Shift-Tab | Switch inventory/detail focus; also works below 100 columns |
| Enter | From inventory, open a full-screen inspector; inside details, follow the selected entry or action |
| z | Expand/collapse the focused details pane |
| v | Toggle Compact / Detailed |
| Esc | Close help/search; return from inspector/source; focus inventory from details; reset filters when already browsing |
| Backspace | Return from related package, file or configuration source |
| PageUp / PageDown, Home / End | Page or move to start/end of focused content |
| r | Refresh the configured live collection; snapshots stay offline |
| ? / F1 | Scrollable help; Esc, ? or F1 closes it without losing context |
| q / Ctrl-C | Quit; q is ordinary text while searching |

Search: Enter/Esc finishes without clearing the query. Backspace deletes a character;
Ctrl-U clears it. Source inventories remain searchable after leaving an inspector.
The bottom bar shows actions for the focused view; Tab returns to the inventory.
On Mac keyboards, use Fn+F2/F3 if the keys control system features.

## Homebrew

- `f`: type filter; `i`: installation-intent filter.
- `1–4`: Overview, Dependencies, Required by, Paths.
- In relationship details: `s` selects recorded/current evidence; `Shift+V` selects a
  keg; `t` switches direct/tree views; Space expands/collapses a branch. Enter follows
  a related package; Backspace restores the previous package, query and filters.
- `5–8`: Unresolved, Missing, Leaves, Cleanup diagnostics.
- Diagnostics: Enter drills down or opens a package; Space expands evidence; `o`
  shows original sanitized output; `c` runs the selected read-only check on demand.

## Codex

- **Skills:** `f/i` filter scope/validation; `1–5` select Overview, Instructions, Files,
  Findings, Coverage. Enter in Files previews text; Backspace returns. `e` opens the
  enablement source when available.
- **Configuration:** `s` switches settings/sources. Enter in the inspector opens the
  selected setting's source summary; Esc/Backspace returns. Raw TOML is never shown.
- **Instructions:** Enter in the inspector toggles a bounded redacted preview; `t`
  switches the conditional trusted scenario. Files retain their selection status.
- **MCP:** `1–6` select Overview, Transport, Authentication, Tools/approvals, Sources,
  Findings. `f` selects a filter dimension; `i` cycles its value. `Shift+V` chooses a
  source; `e` or Enter in the inspector opens it. Esc/Backspace returns to the server.

Eligibility never establishes invocation or connectivity. Inspection does not execute
skills, MCP commands or credential helpers.
