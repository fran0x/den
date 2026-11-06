# Keyboard guide

Den uses **Section → Resource → Details**. Navigation changes presentation only;
collectors, recorded dependency semantics and privacy rules are unchanged.

## Common controls

| Key | Action |
| --- | --- |
| g | Open the Sections switcher |
| Arrows or j/k | Select in the focused list; scroll focused text |
| / | Search the current inventory, switcher, or inspector section list |
| Enter | Open the selected resource/section; finish an inventory or inspector search first |
| Esc | Return one navigation step; close help, switcher or search editing first |
| Tab / Shift-Tab | Switch inventory/overview or inspector sections/content |
| PageUp/PageDown, Home/End | Page or move to start/end of focused content |
| ? | Context-aware, scrollable help; Esc closes without losing position |
| r | Refresh the configured collection; snapshots remain offline |
| q / Ctrl-C | Quit; q is text while editing search |

In the **Sections switcher**, press `/`, type a name and Enter to jump directly.
Arrows can select matches while browsing; Esc cancels without changing the resource.
Search editing uses Backspace to delete and Ctrl-U to clear. An empty query shows all
matches. Esc no longer clears filters: it returns to the previous context.

## The resource inspector

Enter from an inventory opens **Overview**. Tab exposes the inspector's sections;
select with arrows and Enter. At 80 columns, Tab switches full-width panes. At wider
sizes both are visible, with a single focus marker. `/` finds an inspector section;
Enter finishes that search, then Enter opens the selected result.

- **Homebrew:** Dependencies, Required by, Dependency paths, Diagnostics and Evidence.
  Visible actions select recorded/current evidence, the installed keg and the existing
  direct/tree view. Enter follows a related package; Esc restores the original tree.
- **Diagnostics:** Enter drills into groups or affected packages. Inspector actions
  show full findings, original output or run the selected available read-only check.
- **Skills:** Instructions, Supporting files, Findings, Coverage, Configuration source,
  Evidence. Enter on a supporting file opens its text; Esc restores the file selection.
- **Configuration:** Configuration source and Evidence retain precedence and provenance.
- **Instructions:** Instruction preview, Evidence and the conditional trusted scenario.
- **MCP:** Transport, Authentication, Tools/approvals, Configuration source, Findings,
  Evidence and source selection. No server or credential helper is executed.

Each Esc restores one explicit context, including search, filters, selection, focused
pane and scroll. History holds 64 contexts. Refresh matches stable resource identities;
removed resources are clamped, and outdated graph expansion indices are not restored.

## Compatibility shortcuts

F2/F3, numbered detail sections and existing specialist keys remain available but
are not required for section navigation. `v` opens Evidence; `z` opens the inspector.
Neither toggles a display mode. Backspace outside search is an alias for Back.

Inventory `f/i` retain type/intent, scope/validation or MCP filter controls. In Homebrew
relationships, `s`, `Shift+V`, `t` and Space retain evidence, keg, tree and expansion
controls; diagnostic `c/o/Space` retain check/output/evidence actions. MCP `Shift+V`
selects its source. Configuration `s` switches source/settings lists; `e` opens an
available source. These controls are also described in contextual help.
