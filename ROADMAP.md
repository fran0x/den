# Den roadmap

Den is an inspector, not a package manager or administration tool. Each stage should
remain read-only, independently demonstrable and covered by fixtures and tests.
Later stages are proposals, not authorization to implement them now.

The current implementation includes Stages 0–3C1, UX-2B, Stages 3D1–3D2 (Claude Code
skills, instructions, settings, MCP, hooks and plugins) UX-3 (one Agents area), Stage 6A (Cloud › AWS profiles) and Stage 6B (Docker). Stage descriptions record
their delivery history; later UX changes supersede earlier controls. Use the
[keyboard guide](docs/KEYBOARD.md) for current behavior. In particular, UX-2A removed
the Compact/Detailed toggle introduced in UX-1.
The subsequent navigation refinement makes Tab/Shift+Tab switch top-level areas,
keeps browsing input in the inventory and shows a passive information panel. Explicit
inspectors use `[` / `]` for sections/content; older pane-switching instructions are historical.
Homebrew's passive information now shows both dependency directions, automatically
opened to two levels with bounded rows, explicit omissions and recorded/current/keg labels.
It reuses the existing projection and does not change graph, intent or collector semantics.
Homebrew inventory starts with **Tools** (recorded as requested); `a` shows All. Rows show
intent and a recorded **required by N** count, which never reclassifies unknown intent or
establishes that other packages are unused. The view survives navigation and refresh.
Evidence fallback applies to one package at a time.
The latest refinement replaces the `g` switcher: Tab switches areas, Shift+Tab sections,
and the top bar highlights the active area and section. Homebrew packages have no
inspector; the information panel is the package view and Diagnostics is a section. Each
inventory has a single filter line, and each section keeps its own Back history. Earlier
stage notes about `g`, package inspectors and dependency-tree keys are historical.
Codex and Claude no longer have inspectors either: the information panel stacks every
former inspector section and scrolls with PgUp/PgDn; `e` jumps to configuration sources. Only Homebrew Diagnostics keeps a report list.

## Stage 0 — Foundation (implemented)

- Typed inventory and versioned JSON snapshots; embedded synthetic demo.
- Searchable, filterable two-pane TUI, focus, scrolling, help and resize handling.
- Sanitized rendering and reliable terminal cleanup on supported exit paths.
- State, validation, rendering and pseudo-terminal tests.

## Stage 1 — Homebrew inventory (implemented)

- Default live collection via one `brew info --json=v2 --installed` invocation.
- Formula/cask metadata, all installed kegs, recorded cask versions and conservative intent.
- Bounded subprocess output, timeout, cancellation and worker-thread collection.
- Responsive refresh with stable selection, retained filters and explicit stale/error states.
- Offline raw Homebrew snapshots, original Den snapshots and explicit `--demo` mode.
- Type/intent filters and source-specific list rows/details without replacing Stage 0.
- Fixture, process, state and PTY tests; opt-in real inventory verification.
- `just live reconcile` gates inventory accuracy: separate formula/cask counts, every
  recorded installed version, per-package intent and an explanation for every Unknown.
- Homebrew may fetch metadata despite disabled auto-update/analytics. Snapshot mode is offline.
- Unknown intent remains common when modern Homebrew omits explicit dependency flags.

## Stage 2A — Homebrew dependency explorer (implemented)

- In-memory forward/reverse graph from the same inventory, without extra collection commands.
- Recorded runtime direct/transitive/unknown evidence separated from current declarations.
- Keg-specific relationships, linked-keg preference, aliases and type/tap-aware resolution.
- Overview, dependencies, required-by and requested-root paths within the details pane.
- Bounded trees, cycle detection, related-package navigation and context-preserving Back.
- Missing/ambiguous references and unavailable target versions remain explicit uncertainties.
- Installation intent is unchanged. Failed refresh preserves both inventory and graph.
- Synthetic graph/UI tests and opt-in `just live dependencies` against representative Homebrew commands.

## Stage 2B — Dependency diagnostics and cleanup preview (implemented)

- Dependency health groups existing graph edges by cause and evidence source; no new edges
  or installation-intent classifications are inferred.
- Package-level gaps remain separate from missing historical target-keg versions.
- Drill-down, full evidence, related-package jumps and context-preserving Back.
- Separate observations for all graph evidence, recorded runtime and current runtime leaves.
- On-demand `brew missing`, `brew leaves` and `brew autoremove --dry-run` use the existing
  bounded process runner, worker cancellation and fixed arguments. No removal capability.
- Original sanitized stdout/stderr, exit status, check time and stale/unchecked/failed states.
  Unexpected output keeps an unknown structured count; exact identity correlation only.
- Offline diagnostic fixtures and tests; `just live diagnostics` prints aggregate live
  comparisons without saving machine-specific inventories or raw reports.

## Next dependency work — requires separate scope

- Validate diagnostic output formats and metadata differences on Linux and more Homebrew versions.
- Review metadata coverage and external-change freshness before strengthening conclusions.
- A historical keg mismatch, missing path, leaf or dry-run eligibility is not proof of safe removal.
- Package removal, automatic cleanup, installation/repair and cache deletion remain out of scope.

## Stage 3A — Codex Skills explorer (implemented)

- The `g` switcher preserves Homebrew and Skills state; F2 remains a compatibility shortcut.
  Project context defaults to CWD.
- Documented user/project/admin roots; bounded ancestor probing to a Git directory/file marker.
- Explicit legacy and system-cache candidates under CODEX_HOME, with incomplete plugin/session coverage.
- Separate skill domain model, shared cancellable worker and literal search; no plugin framework.
- Real YAML parsing with budgets, required frontmatter, optional openai.yaml and unresolved MCP declarations.
- Search, scope/status filters, full bounded instructions, text file previews, findings and coverage.
- Skill-declared MCP URL fields retain presence as `[URL withheld]`. Raw `agents/openai.yaml`
  previews are withheld, including malformed metadata, before inventory publication.
  Required metadata validity and configured names/transports remain separate from URL semantics.
- Duplicate names are informational. Discovery, validation, filesystem access and configuration status stay distinct.
- Descriptor-relative reads, bounded symlinks/traversal, no script execution or external supporting-file reads.
- Isolated fixture discovery and temporary-filesystem tests; just live skills reports local aggregate evidence.
- No private instructions or machine-specific paths are saved as fixtures; existing snapshots stay isolated.

## Stage 3B — Codex configuration and instructions (implemented)

- Separate TOML parser/provenance, instruction-chain construction, privacy and UI state.
- Existing worker and bounded filesystem primitives; configuration and skills refresh together.
- Explicit `--codex-profile` separate-file scenario; no inferred session profile or legacy profile application.
- System, user, selected profile and nested project sources, with per-field supported resolution.
- User trust entries matched at canonical project root/selected directory: trusted, inactive untrusted,
  or conditional unknown. Root markers select the bounded ancestor chain without Git execution.
- Unknown tables and arrays remain unresolved; raw unknown names/values and parser diagnostics withheld.
- Incomplete applicable layers (invalid/unavailable files or a missing selected profile) produce
  visible inventory warnings and provisional value labels in Overview and Evidence. Existing
  values/defaults and provenance remain inspectable; inactive untrusted layers do not cause this warning.
- Ordered global/project instructions, override/fallback priority, empty/shadowed/unavailable states,
  UTF-8-safe inclusion accounting and bounded, opt-in, redacted previews.
- Skills retain discovery diagnostics; canonical directory/SKILL.md enablement references have source jumps.
- The `g` switcher exposes Codex views; F3 remains a compatibility cycle. Search, selection,
  source inspection, Back and refresh preserve context.
- Isolated Codex fixtures, configuration/security/state/render tests and aggregate `just live codex`.
- No auth/session access, Codex execution, configuration modification or private committed fixtures.

### Deliberate modeling limits

- CLI flags, cloud defaults, enforced requirements and active session state are unobserved.
- Legacy profile structures are flagged, not applied. Unknown model identifiers and custom/granular
  approval policies are withheld rather than guessed. Only classified settings are resolved.
- Named permission profiles (`default_permissions` and `[permissions]`) are not modeled;
  this inspector does not represent the entire current configuration schema.
- `skills.config` across multiple applicable layers has unestablished array merge semantics;
  non-project relative skill paths have unestablished base semantics. Both remain undetermined.
- Exact user trust entries are supported; ambiguous ancestor/worktree relationships remain unknown.
- Root discovery uses system/user/profile markers. Project marker overrides do not retroactively
  change discovered layers. Instructions have baseline and if-trusted scenarios when trust is unknown.
- At most 64 ancestors, 4096 trust entries, 1024 skill rules/layer, 256 KiB per configuration/instruction
  file, 16 simple Markdown fallback names, 16 MiB instruction reads and 2 MiB previews/scenario.
  Instruction collection has a cooperative 30-second deadline; blocking OS calls remain uninterruptible.
- File symlinks are not followed. Explicit Codex/system root aliases are allowed; fixture paths remain confined.
  Preview secret detection is best effort and can over-redact. No natural-language conflict interpretation.

## Stage 3C1 — Codex MCP server explorer (implemented)

- Normalizes MCP definitions from Stage 3B's already parsed layers; no new parser, resolver,
  client, worker or runtime. Configuration, skills and MCP refresh atomically.
- Separate MCP identity/model and validation; standalone and plugin-qualified names never merge.
- Stdio and Streamable HTTP, local/remote environment forwarding, authentication mechanisms,
  startup/timeouts, allowlist-before-denylist and default/per-tool approval evidence.
- Eligibility, disabled, invalid, conditional and unresolved-plugin states remain distinct from runtime status.
- Same-server definitions across applicable layers remain conditional: merge contracts are not guessed.
  Inactive project sources stay inspectable. Unsupported values produce findings, not invented defaults.
- URL parsing uses the `url` crate; no DNS, endpoint requests or protocol client. Only scheme,
  network category and port survive normalization; hosts and all other URL components are withheld.
- Arguments, helper commands, inline env/header values, OAuth IDs/secrets and unknown values
  are discarded from the presentation model. Debug and local MCP check output are aggregate only.
- MCP is available through `g` and the compatibility F3 cycle. Six domain detail sections plus
  shared Evidence, search, four intersecting filters, source jumps/Back and refresh are implemented.
- Synthetic fixtures, deterministic security/state/rendering tests, PTY checks and `just live mcp`.

### MCP coverage and limits

- Only filesystem configuration layers are modeled. CLI overrides, enterprise enforcement,
  cloud/session state, live tools, authentication and server availability remain unobserved.
- Plugin overrides are recognized. No authoritative installed-plugin/version mapping was established
  by the inspected documentation; plugin cache folders are not assumed to be active installations.
  Actual bundled definitions remain unavailable; no broad plugin scanning occurs.
- 1,024 definitions across the inventory; 256 entries per env/header/tool/argument list/map.
  Existing file, ancestor, cancellation and symlink limits remain in force.
- Absolute local launcher/cwd checks inspect bounded filesystem metadata only. PATH, shell expansion,
  package targets and remote executable origins remain unresolved. No executable or helper is run.
- Names are conservatively sanitized/withheld. Scope values are counted rather than displayed.
  Endpoint categories do not establish DNS resolution, reachability or safety.
- Known fields are normalized separately per source; overlays lacking a complete transport are not
  misreported as complete servers. Unestablished merge behavior remains a Stage 3B limitation.

## UX-1 — Interface redesign (implemented)

- Shared compact navigation, status and pane primitives; no collector or domain-model changes.
- Single-row inventories, concise overviews, one accent, terminal-default backgrounds and explicit focus markers.
- Introduced a global Compact/Detailed toggle (removed in UX-2A); full-screen inspectors with
  Back and preserved search/selection remain.
- Below 100 columns, switchable full-width panes; longer reading columns capped at 92 cells.
- Shared scrollable help, focused-view action hints, explicit F2 modules/F3 views, Shift+V for keg/source selection.
- Source searches stay in the inventory; native Back actions cannot leave inventory focus hidden behind an inspector.
- Skills status reflects evaluated eligibility without the former contradictory global message.
- Deterministic synthetic before/after captures, UI privacy/navigation tests and three-size PTY checks.
- Deeper relationship evidence and diagnostics retain their specialist section controls.
  Mouse support, configurable keymaps and semantic Markdown rendering remain outside this stage.

## UX-2A — Navigation and interaction (implemented)

- Searchable `g` switcher exposes every Homebrew/Codex destination without function keys.
- One inspector: brief Overview, visible section list, progressive Evidence and context actions.
- Tab switches sections/content; narrow terminals show one focused pane at a time.
- One typed, bounded history replaces separate Homebrew/source/inspector return mechanisms.
  Snapshots retain UI state and stable identities, never old inventories or worker state.
- Esc restores origin, resource, detail section, focus, filters, query, scroll and dependency expansion.
  Refresh revalidates identities and discards stale graph-indexed expansion state.
- Per-destination return positions; context-aware help; repeated MCP coverage moved to Evidence.
- Existing function/number keys remain secondary; `v` opens Evidence, with no display-mode toggle.
- Focused Enter/Esc hints distinguish browsing, inspector sections and search editing. Esc is
  Back, not filter reset; switcher Enter opens a destination directly. `q` quits switcher browsing
  and stays text during search; Ctrl-C quits from either state.
- Interaction tests, synthetic walkthroughs and three-size PTY checks. No collector changes.
- No persistent sidebar: a single switcher keeps the model consistent at all terminal sizes.

## UX-2B — Hierarchical dependency tree (implemented)

- Dependencies and Required by open as trees with root/evidence context, branch connectors,
  collapsed/expanded markers and visible controls. The direct-list alternative remains available.
- Right expands, then enters the first visible child. Left collapses, then selects the
  occurrence's parent; top-level Left stays in the tree. Space toggles, Enter inspects, Esc goes Back.
- Shared packages keep separate branches. Cycles, uncertain directness, transitive records,
  unavailable kegs and build/test/optional declarations remain labeled non-traversable leaves.
- Existing graph semantics, keg resolution, bounds and collectors are unchanged. History keeps
  branch selection, expansion, filters and viewport; refresh discards old graph indices while
  preserving evidence/view preferences against the refreshed inventory.
- Synthetic state, rendering and bounds regression tests; three-size PTY navigation checks.

## Stage 3C2 — MCP security evidence (requires separate authorization)

- Establish authoritative plugin metadata/version mapping and precise MCP merge contracts first.
- Expand current-version/Linux fixtures and define executable-provenance/privacy boundaries.
- Review launcher supply chains, filesystem permissions and environment exposure only within new scope.
- Keep local declarations separate from live connectivity, authentication and tool metadata.
- No server execution, connection checks, remediation or configuration edits are authorized by 3C1.

## Stage 3D1 — Claude Code skills and instructions (implemented)

- A third area, Claude, after Homebrew and Codex; Shift+Tab switches Skills / Instructions.
- Skills reuse the bounded skill collector on `~/.claude/skills` (or `CLAUDE_CONFIG_DIR`) and
  the project's `.claude/skills`. Skill symlinks resolve; **Also in Codex** compares resolved
  folders with Codex's collected skills and claims nothing before Codex is collected.
- Instructions list managed, user, project, `.claude/` and local `CLAUDE.md` files in reading
  order with sizes, `@import` counts and redacted previews. Linked files are reported, not read.
- No inspector: Enter shows the item full screen. Isolated `--claude-fixture` / `just fixtures claude`.
- Not covered: plugin skills, nested subfolder `CLAUDE.md`/skills, and what a session loaded.

## Stage 3D2 — Claude Code settings, MCP, hooks and plugins (implemented)

- Rules from Claude Code's documentation: settings precedence managed > command line > local >
  project > user, combined lists, MCP name resolution Managed > Local > Project > User > Plugin.
- Settings: interpreted keys with effective value, source and overrides; combined lists;
  `env` names only; credential helpers withheld; other keys by name; invalid files flagged.
- MCP: `managed-mcp.json`, `~/.claude.json` (user and this project's local servers), `.mcp.json`
  and enabled plugins, with shadowing, approvals, trust, toggles and managed blocking. Classified
  summaries only. Only MCP state and project trust are kept from `~/.claude.json`.
- Hooks from settings and enabled plugins, with redacted commands and disable/managed-only effects.
- Plugins from `installed_plugins.json` and `known_marketplaces.json` with enabled state and components.
- Not covered: command-line and MDM/registry/server-managed settings, allowedMcpServers/
  deniedMcpServers, frontmatter hooks, plugin manifest path overrides, and what a session loaded.

## UX-3 — One Agents area (implemented)

- Codex and Claude Code areas merged into **Agents**; Tab switches Homebrew / Agents.
- Skills match by resolved folder, MCP servers by name (with **differs** when transport or
  program differ), instructions and settings by folder or name.
- Every list has name · Tools (Both / Codex only / Claude only) · Status, with one status
  vocabulary for both tools: Ready, Off, Needs approval, Problem, Unknown (Instructions:
  Used, Not used). The panel keeps each tool's own words.
- No filters: the Tools column replaced the tool filter, the Skills status and scope
  filters and Codex's four MCP filters. `o` (supporting file contents) was removed; files
  are listed in the panel.
- Collectors, privacy rules and per-tool semantics are unchanged; `src/agents.rs` only matches.

## Stage 6A — Cloud › AWS profiles (implemented)

- A third area, **Cloud**, with an AWS section: profiles from the config and credentials
  files (or `AWS_CONFIG_FILE` / `AWS_SHARED_CREDENTIALS_FILE`) and SSO sessions.
- Auth in SDK order (Role, SSO, Process, Static keys, None set); Ready / Problem status
  (missing source profile or SSO session, role loops, incomplete SSO, open credentials file).
- Credentials file: names only, values dropped while parsing. Config: allowlisted values;
  URLs, external IDs, token files and `credential_process` withheld. No caches, no AWS calls.
- Next: Kubernetes contexts from `~/.kube/config` as a second Cloud section, with the
  same allowlist approach; link EKS `exec` entries to AWS profiles.

## Stage 6B — Docker (implemented)

- A fourth area, **Docker**: Containers / Images / Volumes from `docker container ls --all`,
  `image ls`, `volume ls` and `system df`, run through the bounded Homebrew runner only
  once the area is opened.
- Images and volumes say which containers use them; dangling images and anonymous volumes
  are marked; disk use and reclaimable space come from `system df`.
- Withheld: container commands, environment, labels other than the Compose project, logs,
  Docker's error output and saved registry logins.
- Not covered: Docker contexts other than the current one, networks, build cache entries.

## Stage 4 — Executables, runtimes, libraries and caches (planned)

- Add focused collectors incrementally; define identity and deduplication rules.
- Explain PATH precedence, runtime ownership and overlapping installations.
- Make costly filesystem scans explicit and bounded. Report cache sizes without deletion.

## Stage 5 — Trust and security evidence (planned)

- Define a threat model and evidence vocabulary before assigning assessments.
- Separate provenance, permissions, signatures and vulnerability evidence.
- State freshness, uncertainty and limitations; unknown must never imply safe.
- Any external advisory/network integration needs an explicit consent and privacy design.

## After UX-2B

Stop here unless another stage is explicitly requested. Keep `just live reconcile`,
`just live dependencies`, `just live diagnostics`, `just live skills`,
`just live codex`, `just live mcp` and `just live terminal` as opt-in local checks.
Graphs and diagnostic reports remain in memory. Den/raw Homebrew snapshots are loadable
inputs; snapshot creation/comparison and external-change detection are not implemented.
Snapshot checks never run Homebrew or collect live Codex data.
