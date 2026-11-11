# Keyboard guide

Den uses **Area › Section › Inventory**. Navigation changes presentation only;
collectors, recorded dependency semantics and privacy rules are unchanged.

## Moving around

The top bar shows the five areas. The active area is highlighted (bold and underlined
without colour) and is followed by its section, for example `Homebrew › Packages`.

Colours follow the terminal: Catppuccin when it reports 24-bit colour (`COLORTERM=truecolor`),
Latte on a light background (`COLORFGBG`), the 16 basic colours otherwise and none with
`NO_COLOR`. The selected row has a soft background, or reverse video without colour.
When a tool or its configuration is missing (no `brew` or `docker` on PATH, no
`~/.codex`, `~/.claude` or `~/.aws` files), its list says so once and `r` retries; the
status line stays empty instead of showing zero counts.

In Agents, Cloud and Docker, states are coloured: Ready and Running green, Needs approval
yellow, Problem red (bold without colour).

| Key | Action |
| --- | --- |
| Tab | Switch area: Homebrew → Agents → Cloud → Docker → Toolchains. Each area keeps its place |
| Shift+Tab | Switch section: Homebrew Packages / Diagnostics; Agents Skills / Instructions / Settings / MCP / Hooks / Plugins; Cloud AWS / Kubernetes; Docker Containers / Images / Volumes; Toolchains On PATH / Rust / Packages |
| Up/Down or j/k | Select in the inventory |
| PageUp/PageDown | Scroll the information panel (Agents, Cloud, Docker) |
| Home/End | First / last row |
| / | Search the inventory, or find a Diagnostics report |
| Esc | Back within the current section; close help; finish search editing first |
| [ / ] | In Diagnostics, select the report list / content |
| ? | Context-aware, scrollable help; Esc closes it without losing position |
| r | Refresh the configured collection; snapshots stay offline, fixtures reread only their roots |
| q | Quit while browsing; text while editing search |
| Ctrl-C | Quit in every context, including search editing |

F2 is an alias for Tab and F3 for Shift+Tab. Backspace is an alias for Back when not
editing search. There is no section switcher and no display mode.

Tab and Shift+Tab also work during search: they end editing and keep the query. Help is
modal; close it before switching.

## Search and the filter line

Each inventory has one line above the list showing its view, filters and search, for
example `[Tools 5] · All 26 · Search: rip`. There is no separate header line.
Search editing uses Backspace to delete and Ctrl-U to clear; Enter or Esc ends editing
and keeps the query. Esc is navigation, not a filter-reset command; Back may restore an
earlier query/filter state.

Browsing has one input target: the inventory. The information panel updates with the
selection and never takes focus. At 100 columns or more it sits beside the inventory;
below 100 it sits underneath.

## Homebrew Packages

The inventory starts with **Tools**: packages Homebrew recorded as installed on request.
`a` switches between **Tools** and **All**; the active view is bracketed in the filter line.
Tools are bold, so they stand out in All. Casks are bracketed, for example `[firefox]`,
here and in the panel's trees; there is no type filter. Counts follow the search. During
search, `a` is text. Area switching, Back and refresh preserve the view and search.

Outdated packages start their row with the newer version in yellow, for example `↑ 15.0`;
the status line counts them and the panel says whether a formula is pinned. This is
Homebrew's own `outdated` flag from `brew info`, as current as your last `brew update`;
Den never runs `brew update`.

Missing, conflicting or mixed keg records stay **Unknown**, so those packages appear only
in All. Rows label **Requested**, **Dependency** or **Unknown** and add **required by N**:
the number of other installed packages with recorded runtime references to an installed
version, including transitive/unknown-directness records. Current declarations, self
references and unresolved or missing target versions do not count. When a row is narrow,
a non-requested package shows only "required by N". No count does not establish that a
package is unused or safe to remove; the count is marked partial if the graph is bounded.

Packages have no inspector: the information panel is the package view. It shows
**Depends on** and **Required by** trees, opened automatically to two levels with at most
eight rows per direction (fewer when space is limited). Recorded branch versions appear as
`@version`. Unknown directness, transitive records, optional/build/test declarations and
unresolved keg links remain labeled leaves; they never become inferred parent links.
`PARTIAL` and `…` indicate bounds or clipping; very small panels show both direction
labels only.

The panel uses recorded runtime evidence. A package without recorded runtime metadata
falls back to current declarations for that package only, labeled
`Current declarations · not historical`; the next selection starts from recorded evidence.

## Homebrew Diagnostics

Shift+Tab from Packages opens Diagnostics. `[` selects a report (Unresolved, Missing,
Leaves, Cleanup preview) or use `5`–`8`; `]` returns to its content. Enter drills into a
group; Enter on an affected package selects it in Packages, and Esc returns to the
diagnostic. Space shows the full finding, `o` the original Homebrew report and `c` runs
the selected read-only check (live mode only). Diagnostics never establish safe removal.

## Agents: Codex and Claude Code side by side

There is no inspector. The information panel shows everything about the selection, each
tool under its own heading, with bold section headings; PgUp/PgDn scroll it and its last
row says when there is more. A new selection starts at the top; switching areas or
sections keeps the place. A header row names the columns.

Every list has the same three columns: name, **Tools** and **Status**.

| Section | One row per | Status |
| --- | --- | --- |
| Skills | skill folder | Ready, Off, Problem, Unknown |
| Instructions | Managed, User, each project folder | Used, Not used, Problem, Unknown |
| Settings | setting name | the value |
| MCP | server name | Ready, Off, Needs approval, Problem, Unknown |
| Hooks, Plugins | hook or plugin (Claude Code only) | Ready, Off, Problem, then the command or details |

**Tools** says Both, Codex only or Claude only (MCP: **Both, differ** when the same name
is a different kind of server or program). When both tools have a row and agree, Status
shows one word; otherwise `Codex x · Claude y`. The words mean the same for both tools;
the panel adds each tool's own words (for example Eligible or Pending approval). The
status line starts with any collection problem, then the count and how rows split
between the tools.

- **Skills:** Ready = valid and nothing found that turns it off (Codex: Eligible or
  Enabled in config; Den does not model Claude rules that could turn a skill off); Off =
  disabled in config; Problem = invalid, unreadable or duplicate; Unknown = Den cannot
  tell (unclear config, bundled or legacy copy). Supporting files are listed by name.
  Raw `agents/openai.yaml` previews are withheld; Evidence shows
  normalized MCP declarations with URL fields represented as `[URL withheld]`. Codex reads
  `~/.agents/skills`; Claude Code reads `~/.claude/skills` (`CLAUDE_CONFIG_DIR` if set) and
  the project's `.claude/skills`. Plugin skills and nested `.claude/skills` are not listed.
- **Instructions:** Codex's chain entries with bounded, redacted previews, and Claude
  Code's `CLAUDE.md`, `.claude/CLAUDE.md` and `CLAUDE.local.md` in reading order. `t` shows
  Codex's chain as if the project were trusted. `@imports` are counted, not followed;
  linked files are reported with their target, not read.
- **Settings:** Codex values with Evidence (precedence and provenance, no raw TOML); `s`
  shows Codex source files. Invalid or unavailable applicable Codex layers make values
  provisional. Claude Code: effective values with their file and what they override,
  combined lists, `env` names only, credential helpers withheld, unreadable files first.
- **MCP:** Codex's sources with transport, authentication, tools and findings (`V` picks
  the next source), then Claude Code's entries with scope and status: Configured,
  Shadowed, Pending approval, Approved, Rejected, Turned off or Blocked. Program names and
  variable/header names only; no server or credential helper is executed.
- **Hooks:** event, matcher, file or plugin, and the redacted command; Off or Blocked when
  `disableAllHooks` or `allowManagedHooksOnly` applies.
- **Plugins:** enabled state and the settings file that decides it, version, marketplace,
  and skills, commands, agents, hooks and MCP servers.

The panel names the file each setting or MCP server comes from. `/` searches and `r`
refreshes both tools.

## Cloud › AWS

One row per profile (`[default]`, `[profile x]` in the config file, `[x]` in the
credentials file), then each `[sso-session x]`, as Profile · Auth · Status.

- **Auth**, in the order the AWS SDKs check: Role (`role_arn` with `source_profile`,
  `credential_source` or a web identity token), SSO, Process (`credential_process`),
  Static keys, None set.
- **Status:** Ready with the region, or Problem with what is wrong. SSO sessions are Ready
  when a profile uses them, Off otherwise.
- **Panel:** region and output, account ID and role, role ARN and chain (`dev → base`),
  MFA device, where the profile is defined, and other setting names. `AWS_PROFILE` (or the
  default profile) is marked active.
- **Privacy:** the credentials file keeps profile and key names only; values are dropped
  as each line is read. SSO start URLs, `endpoint_url`, `external_id`,
  `web_identity_token_file` and `credential_process` are withheld. Linked files are
  reported, not read. Nothing is called, so whether credentials work is not checked.

`/` searches, PgUp/PgDn scroll the panel and `r` refreshes.

## Cloud › Kubernetes

Shift+Tab from AWS. One row per context from the kubeconfig files (`KUBECONFIG`, or
`~/.kube/config`), current context first, as Context · Auth · Status. With several
`KUBECONFIG` files, for each name the first file wins, as in kubectl.

- **Auth**, in the order kubectl uses them: Exec (with the program name, for example
  `Exec · aws`), an auth provider (`gcp provider`), Client cert, Token, Basic auth, None set.
- **Status:** Ready, then `current` for current-context and the namespace; or Problem: a
  missing cluster or user, TLS verification off, plain http, a password stored in the file,
  the `gcp`/`azure` auth providers removed in kubectl 1.26, or a file other users can read.
  A `current-context` that names no context and an unreadable or invalid file are rows too.
- **Panel:** namespace, cluster with its server (scheme, kind of address — this machine,
  private network, public IP, host name — and port), certificate authority, user and auth,
  exec plugin details, and the file that defines the context. Coverage counts clusters and
  users no context uses and entries ignored because an earlier file defines the name.
- **Privacy:** tokens, passwords, certificate and key data are parsed as presence only and
  never kept. Server hosts, proxy URLs, file paths for certificates, keys and tokens, exec
  arguments and env values are withheld; env variable names are shown. Linked files are
  reported, not read. Nothing is called: no cluster, `kubectl` or exec plugin.

## Docker

Containers / Images / Volumes, each with a header row:

- **Containers:** Container · State (Running, Stopped, Paused, Restarting, Created, Dead)
  · Image and Compose project. The panel adds status text, ports, mounts and ID.
- **Images:** Image (`repo:tag`, or `<none> id` when dangling) · Size · Used by (N
  containers, Unused, or Unused · dangling). Containers are matched by tag, by
  `repo` meaning `:latest`, or by image ID.
- **Volumes:** Volume (anonymous volumes shortened) · Driver · Used by.

The status line shows counts and `docker system df` disk use and reclaimable space (Images
also build cache). Den runs only four listing commands, each with a 10-second limit and an
output cap, once this area is opened and again every 5 seconds while it stays open (5
seconds after the last run ends; fixtures are not refreshed); `r` runs them at once. A stopped daemon or a
missing CLI is explained in the status line and earlier results stay. Not shown:
container commands, environment, labels except the Compose project, logs, and saved
registry logins.

## Toolchains

On PATH / Rust / Packages, each Command or Name · From or Kind · Status. Everything is read
from files: no toolchain, package manager or program found on PATH is run.

- **On PATH:** for about twenty common commands (`node`, `npm`, `python3`, `pip3`, `uv`,
  `ruby`, `go`, `java`, `rustc`, `cargo`, …) the copy the shell runs, who provides it
  (Homebrew, rustup, uv, nvm, pyenv, mise, asdf, Volta, the system, or its folder) and its
  version when the install path or the package's `package.json` shows it; the panel lists
  the copies it hides. Problems: a relative PATH entry, a manager folder with programs that
  is not on PATH (`~/.cargo/bin`, `~/go/bin`, `~/.bun/bin`, `~/.local/bin`), a system copy
  hiding a managed one, and macOS stubs with nothing behind them (`java` without a JDK,
  `python3` without the Command Line Tools).
- **Rust:** rustup toolchains, the default first, with folder overrides, targets and
  components (`Installed` when neither the default nor an override selects one; a project's
  `rust-toolchain.toml` may still pin it), then `cargo install` crates from
  `~/.cargo/.crates2.json` with version, source, programs and the rustc that built them. A
  crate whose program is missing is a Problem.
- **Packages:** `npm -g` (from the winning npm's `lib/node_modules`, or
  `NPM_CONFIG_PREFIX`), `uv tool` installs and uv-managed Pythons, `pipx`, `bun` globals
  and `go install` programs (names only).
- **Privacy:** git and private-registry URLs and local source paths of crates are withheld.
  Newer versions, disk use and per-folder selections (`rust-toolchain.toml`, `.nvmrc`,
  `.python-version`) are not checked.

`RUSTUP_HOME`, `CARGO_HOME`, `UV_TOOL_DIR`, `UV_PYTHON_INSTALL_DIR`, `PIPX_HOME`,
`BUN_INSTALL`, `GOBIN`/`GOPATH`, `NPM_CONFIG_PREFIX` and `JAVA_HOME` are respected. The
area is read when opened; `r` reads it again.

## Back history

Each section (Packages, Diagnostics, Skills, Instructions, Settings, MCP, Hooks, Plugins) keeps its
own Back history of up to 64 contexts, including search, filters, selection, focused pane
and scroll. Tab and Shift+Tab never add a Back step. A deliberate jump to another section,
such as a configuration source or a package from Diagnostics, returns with Esc. Refresh
matches stable resource identities; removed resources are clamped.
