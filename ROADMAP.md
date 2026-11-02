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

## Stage 3C — MCP inspection (requires separate authorization)

- Establish exact supported path/array merge contracts and additional trust/worktree fixtures before
  strengthening configuration claims. Validate Linux and additional current Codex versions.
- Reuse safe provenance and redaction for declarative MCP metadata; define an explicit display allowlist.
- Keep runtime activation, connectivity and credential retrieval separate from local declarations.
- No server execution, configuration edits or agent launches without new scope.

## Stage 4 — Executables, runtimes, libraries and caches

- Add focused collectors incrementally; define identity and deduplication rules.
- Explain PATH precedence, runtime ownership and overlapping installations.
- Make costly filesystem scans explicit and bounded. Report cache sizes without deletion.

## Stage 5 — Trust and security evidence

- Define a threat model and evidence vocabulary before assigning assessments.
- Separate provenance, permissions, signatures and vulnerability evidence.
- State freshness, uncertainty and limitations; unknown must never imply safe.
- Any external advisory/network integration needs an explicit consent and privacy design.

## After Stage 3B

Stop here unless another stage is explicitly requested. Keep `just reconcile`,
`just dependency-check`, `just diagnostic-check`, `just skills-check` and
`just codex-config-check` as opt-in live checks.
Graphs and diagnostic reports remain in memory; snapshot checks never run Homebrew.
