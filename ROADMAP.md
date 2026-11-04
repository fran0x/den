# Den roadmap

Den is an inspector, not a package manager or administration tool. Each stage should
remain read-only, independently demonstrable and covered by fixtures and tests.
Later stages are proposals, not authorization to implement them now.

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
- `just reconcile` gates inventory accuracy: separate formula/cask counts, every
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
- Synthetic graph/UI tests and opt-in `just dependency-check` against representative Homebrew commands.

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
- Offline diagnostic fixtures and tests; `just diagnostic-check` prints aggregate live
  comparisons without saving machine-specific inventories or raw reports.

## Next dependency work — requires separate scope

- Validate diagnostic output formats and metadata differences on Linux and more Homebrew versions.
- Review metadata coverage and external-change freshness before strengthening conclusions.
- A historical keg mismatch, missing path, leaf or dry-run eligibility is not proof of safe removal.
- Package removal, automatic cleanup, installation/repair and cache deletion remain out of scope.

## Stage 3A — Codex Skills explorer (implemented)

- F2 module navigation preserves Homebrew and Skills state; project context defaults to CWD.
- Documented user/project/admin roots; bounded ancestor probing to a Git directory/file marker.
- Explicit legacy and system-cache candidates under CODEX_HOME, with incomplete plugin/session coverage.
- Separate skill domain model, shared cancellable worker and literal search; no plugin framework.
- Real YAML parsing with budgets, required frontmatter, optional openai.yaml and unresolved MCP declarations.
- Search, scope/status filters, full bounded instructions, text file previews, findings and coverage.
- Duplicate names are informational. Discovery, validation, filesystem access and configuration status stay distinct.
- Descriptor-relative reads, bounded symlinks/traversal, no script execution or external supporting-file reads.
- Isolated fixture discovery and temporary-filesystem tests; just skills-check reports local aggregate evidence.
- No private instructions or machine-specific paths are saved as fixtures; existing snapshots stay isolated.

## Stage 3B — Codex configuration and instructions (implemented)

- Separate TOML parser/provenance, instruction-chain construction, privacy and UI state.
- Existing worker and bounded filesystem primitives; configuration and skills refresh together.
- Explicit `--codex-profile` separate-file scenario; no inferred session profile or legacy profile application.
- System, user, selected profile and nested project sources, with per-field supported resolution.
- User trust entries matched at canonical project root/selected directory: trusted, inactive untrusted,
  or conditional unknown. Root markers select the bounded ancestor chain without Git execution.
- Unknown tables and arrays remain unresolved; raw unknown names/values and parser diagnostics withheld.
- Ordered global/project instructions, override/fallback priority, empty/shadowed/unavailable states,
  UTF-8-safe inclusion accounting and bounded, opt-in, redacted previews.
- Skills retain discovery diagnostics; canonical directory/SKILL.md enablement references have source jumps.
- F3 cycles Codex views; search, selection, source inspection, back and refresh preserve context.
- Isolated Codex fixtures, configuration/security/state/render tests and aggregate `just codex-config-check`.
- No auth/session access, Codex execution, configuration modification or private committed fixtures.

### Deliberate modeling limits

- CLI flags, cloud defaults, enforced requirements and active session state are unobserved.
- Legacy profile structures are flagged, not applied. Unknown model identifiers and custom/granular
  approval policies are withheld rather than guessed. Only classified settings are resolved.
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
  are discarded from the presentation model. Debug and live-check output are aggregate only.
- F3 adds MCP; six detail sections, search, four intersecting filters, source jumps/back and refresh.
- Synthetic fixtures, deterministic security/state/rendering tests, PTY checks and `just mcp-check`.

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
- Global Compact/Detailed toggle; full-screen inspectors with Back and preserved search/selection.
- Below 100 columns, switchable full-width panes; longer reading columns capped at 92 cells.
- Shared scrollable help, focused-view action hints, explicit F2 modules/F3 views, Shift+V for keg/source selection.
- Source searches stay in the inventory; native Back actions cannot leave inventory focus hidden behind an inspector.
- Skills status reflects evaluated eligibility without the former contradictory global message.
- Deterministic synthetic before/after captures, UI privacy/navigation tests and three-size PTY checks.
- Deeper relationship evidence and diagnostics retain their specialist section controls.
  Mouse support, configurable keymaps and semantic Markdown rendering remain outside this stage.

## Stage 3C2 — MCP security evidence (requires separate authorization)

- Establish authoritative plugin metadata/version mapping and precise MCP merge contracts first.
- Expand current-version/Linux fixtures and define executable-provenance/privacy boundaries.
- Review launcher supply chains, filesystem permissions and environment exposure only within new scope.
- Keep local declarations separate from live connectivity, authentication and tool metadata.
- No server execution, connection checks, remediation or configuration edits are authorized by 3C1.

## Stage 4 — Executables, runtimes, libraries and caches

- Add focused collectors incrementally; define identity and deduplication rules.
- Explain PATH precedence, runtime ownership and overlapping installations.
- Make costly filesystem scans explicit and bounded. Report cache sizes without deletion.

## Stage 5 — Trust and security evidence

- Define a threat model and evidence vocabulary before assigning assessments.
- Separate provenance, permissions, signatures and vulnerability evidence.
- State freshness, uncertainty and limitations; unknown must never imply safe.
- Any external advisory/network integration needs an explicit consent and privacy design.

## After UX-1

Stop here unless another stage is explicitly requested. Keep `just reconcile`,
`just dependency-check`, `just diagnostic-check`, `just skills-check` and
`just codex-config-check` and `just mcp-check` as opt-in local checks.
Graphs and diagnostic reports remain in memory; snapshot checks never run Homebrew.
