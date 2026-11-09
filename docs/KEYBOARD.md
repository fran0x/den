# Keyboard guide

Den uses **Area → Inventory → Information**, with explicit inspectors for deeper exploration.
Navigation changes presentation only;
collectors, recorded dependency semantics and privacy rules are unchanged.

## Common controls

| Key | Action |
| --- | --- |
| Tab / Shift+Tab | Switch Homebrew/Codex, restoring the last view in each area |
| g | Open Sections for subviews and specialist destinations |
| Up/Down or j/k | Select in the focused list; scroll focused text; j/k are text while editing search |
| / | Search the current inventory, switcher, or inspector section list |
| Enter | Inspect the selected inventory item; open an inspector section; finish inventory/inspector search first |
| Esc | Back; close help/switcher, or finish inventory/inspector search editing first |
| [ / ] | In an explicit inspector, select sections / content |
| PageUp/PageDown | Page inventories, inspector sections or focused text |
| Home/End | Move to the start/end of the focused list or text |
| ? | Context-aware, scrollable help; Esc closes without losing position |
| r | Refresh the configured collection; snapshots stay offline, fixtures reread only their roots |
| q | Quit while browsing, including the Sections switcher; text while editing search |
| Ctrl-C | Quit in every context, including search editing |

In the **Sections switcher**, press `/`, type a name and Enter to jump directly.
Up/Down can select matches during search; j/k also select while browsing. Enter opens
the selected destination directly, and Esc cancels the switcher even during search.
The switcher advertises Ctrl-C rather than q for quitting while editing a query.
Search editing uses Backspace to delete and Ctrl-U to clear. An empty query shows all
matches. In inventory/inspector searches, Enter or Esc ends editing and keeps the query;
another Enter opens the resource/section. Esc is navigation, not a filter-reset command;
Back may restore an earlier query/filter state.

Browsing has one input target: the inventory. Up/Down, paging, Home/End and search
select resources; the information panel updates automatically and never takes focus.
At 100 columns or more it sits beside the inventory; below 100 it sits underneath.
Clipped information offers Enter to open the full inspector.
Homebrew information reserves space for both **Depends on** and **Required by** trees.
They open automatically to two levels, with at most eight rows per direction (fewer
when space is limited). They share the displayed recorded/current evidence and selected
keg; recorded branch versions are shown as `@version`. Unknown directness, transitive
records, optional/build/test declarations and unresolved keg links remain labeled leaves.
Unavailable metadata stays explicit. `PARTIAL` and `…` indicate bounds or clipping;
very small panels show both direction labels and require the inspector to read the trees.
The panel never accepts tree selection or expansion input.

Homebrew inventory rows label **Requested**, **Dependency** or **Unknown** from installation
records. `i` cycles **All / Explicitly requested / Dependencies / Unknown**. Missing,
conflicting or mixed keg intent stays Unknown. `u` independently toggles **Used by others**;
requested and unknown packages can match it too. It uses recorded runtime references
from another installed package to an installed version, including transitive/unknown-directness
records. Current declarations, self references and unresolved or missing target versions
do not qualify. This filter does not follow the inspector's recorded/current or keg selection.
Absence of matching evidence does not establish that a package is unused or safe to remove.

Counts reflect the current search and package type, before intent and usage filtering.
The three intent counts sum to All; Used by others overlaps those categories and is
marked partial if graph coverage is bounded. Selected filters are bracketed or marked ON.
`f` cycles All / Formula / Cask, and filters intersect. During search, `u` is text;
Ctrl-U still clears the query. Area switching, Back and refresh preserve filters.

Tab and Shift+Tab also switch areas from an inspector or search, retaining the query
and ending search editing. Help and Sections remain modal; close them before switching areas.

## The resource inspector

Enter from the inventory opens **Overview**. `[` exposes the inspector's sections;
select with Up/Down or j/k and Enter. `]` selects content. Below 100 columns these
controls switch full-width inspector panes.
At 100 columns or more both are visible, with a single focus marker. `/` finds a section;
Enter finishes that search, then Enter opens the selected result.

- **Homebrew:** Dependencies, Required by, Dependency paths, Diagnostics and Evidence.
  Visible actions select recorded/current evidence, the installed keg and the existing
  direct/tree view. Enter inspects a related package; Esc restores the original tree.
- **Diagnostics:** Enter drills into groups or affected packages. Inspector actions
  show full findings, original output or run the selected available read-only check.
- **Skills:** Instructions, Supporting files, Findings, Coverage, Configuration source,
  Evidence. Enter on a supporting file opens its text; Esc restores the file selection.
  Raw `agents/openai.yaml` previews are withheld; Evidence shows normalized MCP declarations
  with URL fields represented as `[URL withheld]`.
- **Configuration:** Configuration source and Evidence retain precedence and provenance.
  Invalid/unavailable applicable layers or a missing selected profile make displayed values
  provisional; malformed inactive untrusted layers do not cause this warning.
- **Instructions:** Instruction preview, Evidence and the conditional trusted scenario.
  Enter in inspector content also toggles the bounded, best-effort redacted preview.
- **MCP:** Transport, Authentication, Tools/approvals, Configuration source, Findings,
  Evidence and source selection. No server or credential helper is executed.

Each Esc restores one explicit context, including search, filters, selection, focused
pane and scroll. History holds 64 contexts. Refresh matches stable resource identities;
removed resources are clamped, and outdated graph expansion indices are not restored.
Configuration/Skills/MCP refresh together. Source jumps use the same Back history.

## Dependency trees (UX-2B)

Dependencies and Required by default to trees. The header identifies the root package
and selected evidence/keg; connectors show branches, `[+]` marks collapsed branches,
`[-]` marks expanded branches and `!` marks cycles.

| Key | Action with tree content focused |
| --- | --- |
| Right | Expand the selected branch; if already expanded, select its first visible child |
| Left | Collapse the selected branch; otherwise select its parent occurrence |
| Space | Toggle the selected branch, retaining nested expansion for reopening |
| Enter | Inspect the related package |
| Esc / Backspace | Restore the previous navigation context and tree position |
| t | Switch between the tree and direct-list views |

At the top level, Left stays in the current tree. `[` selects inspector sections;
Left in other inspector sections or the direct list is Back; outside an inspector it
has no effect on the passive information panel. Dependency paths stay a separate list.
Transitive/unknown-directness records and optional/build/test declarations do not become
inferred parent-child relationships. Missing or ambiguous targets, unresolved keg links
and cycles stop expansion. Partial results remain explicit when bounds are reached.
Deep trees shorten ancestor guides with `…` to keep the selected package readable;
this follows selection and terminal width without changing the tree or its parent links.

`s` switches recorded/current evidence and `V` (Shift+v) cycles installed kegs. Changing
evidence, keg or direct/tree mode resets the current relationship selection/expansion.
The direct list hides recorded transitive/unknown-directness records; current declarations
retain their labeled kinds. Back from an inspected package restores the saved tree context.

## Compatibility shortcuts

F2/F3, numbered detail sections and existing specialist keys remain available but
are not required for section navigation. `v` opens Evidence; `z` opens the inspector.
Neither toggles a display mode. In resource views and inspectors, Backspace is an alias
for Back when not editing search. F2 is an alias for Tab; F3 cycles the
current module's views. Configuration Sources is also available directly through `g`.

Inventory `f/i` retain type/intent, scope/validation or MCP filter controls. In Homebrew
relationships, tree controls act when content is focused; diagnostic `c/o/Space`
run the selected read-only check, toggle original output or toggle full findings.
MCP `V` selects a configuration definition. Configuration `s` switches source/settings
lists; `e` opens an available configuration source from Configuration, MCP or Skills.
The inspector's Configuration source section provides the same jump when available.
