# Using Den

Den has five areas: **Homebrew, Agents, Cloud, Docker and Toolchains**. Each area has a few
sections, and each section is a list. Select a row and the panel next to it tells you
everything Den knows about it.

## Keys

| Key | Does |
| --- | --- |
| `Tab` | Next area |
| `Shift+Tab` | Next section in this area |
| `↑` `↓` or `j` `k` | Move through the list |
| `Home` `End` | First or last row |
| `PgUp` `PgDn` | Scroll the panel |
| `/` | Search the list |
| `Esc` | Go back |
| `r` | Refresh |
| `?` | Help for where you are |
| `q` | Quit |

Every area and section remembers where you were, so you can switch freely. A few areas
add their own keys; they are listed below.

## Reading the screen

- **Top bar:** the five areas, with the one you are in highlighted and followed by its
  section, for example `Homebrew › Packages`.
- **Filter line:** above the list, it shows the current view and search, for example
  `[Tools 5] · All 26 · Search: rip`.
- **Panel:** beside the list on wide terminals (100 columns or more), underneath on narrow
  ones. When it has more than fits, its last line says so.
- **Status line:** at the bottom, the counts for this section and any problem collecting
  them.

Statuses are coloured: **Ready** and **Running** in green, **Needs approval** in yellow,
**Problem** in red. If a tool is not installed or not configured (no `brew`, no `docker`,
no `~/.aws`, …), its list says so and `r` tries again.

## Homebrew

**Packages** starts with your **Tools**: what you installed on purpose. Homebrew keeps no
record of why a cask was installed, so a cask counts as a tool unless another installed
package needs it; its panel says **No request record**. Press `a` to see **All** packages,
dependencies included; your tools stay in bold. Casks appear in brackets and their own
colour, like `[firefox]`.

Each row says whether you asked for the package, whether it came in as a dependency, and
how many other packages need it (**required by 3**). A newer version shows at the start of
the row in yellow, like `↑ 15.0`. That comes from Homebrew's own data, and the status line
says how old it is (**data from 2 h ago**). Den never runs `brew update` itself.

The panel shows two small trees: what the package **depends on** and what it is
**required by**. When Homebrew's records are incomplete, Den says **Unknown** rather than
guess, and a package with no dependents is not necessarily safe to remove.

**Diagnostics** (`Shift+Tab` from Packages) collects Homebrew's own reports: unresolved
links, missing dependencies, leaves and a cleanup preview.

| Key | Does |
| --- | --- |
| `[` or `5`–`8` | Pick a report |
| `]` | Back to its content |
| `Enter` | Open a group, or jump to a package in Packages (`Esc` comes back) |
| `Space` | Show the full finding |
| `o` | Show Homebrew's original report |
| `c` | Run the selected check now (live only, read-only) |

In `dew`, `x` opens the actions for the selected package and for Homebrew; see
[Making changes](#making-changes).

## Agents

Codex and Claude Code, side by side. Every list has the same three columns: the name,
which **Tools** have it (Both, Codex only, Claude only) and its **Status**. When the two
tools disagree, the status shows both, for example `Codex Ready · Claude Off`.

| Section | What you see |
| --- | --- |
| **Skills** | Each skill folder: Ready, Off, Problem (invalid, unreadable or duplicate) or Unknown |
| **Instructions** | `AGENTS.md` and `CLAUDE.md` files, in the order each tool reads them |
| **Settings** | Each setting with its value, and the file it comes from |
| **MCP** | Each MCP server: Ready, Off, Needs approval or Problem |
| **Hooks** | Claude Code hooks: the event, matcher and command |
| **Plugins** | Claude Code plugins: whether they are on, and what they add |

The panel shows each tool under its own heading, with the details that only that tool has.

| Key | Does |
| --- | --- |
| `s` | Settings: show Codex's source files |
| `t` | Instructions: show what Codex would read if this project were trusted |
| `V` | MCP: switch to the next Codex source |

When Homebrew installed Codex or Claude Code and a newer version is known, the status line
starts with it, for example **↑ Codex 0.167.2 available**. The newer version comes from
Homebrew's data or from Codex's own update check (`~/.codex/version.json`). Den never checks
online, and a tool installed another way shows nothing, because Den can't tell its version.

Den shows what the configuration files say, not what a running Codex or Claude Code
session has loaded.

## Cloud

**AWS** lists your profiles and SSO sessions. For each: how it gets credentials (Role, SSO,
Process, Static keys or None set) and whether it is **Ready**, with its region, or has a
**Problem**, with what is wrong. The panel shows the account, role, role chain and the file
that defines it.

The **active** profile, the one the AWS CLI uses when you don't pass `--profile`, comes
first, highlighted with a `*` before its name and **active** in its status. It is the profile named
by `AWS_PROFILE`, or `default` when that is not set. If `AWS_PROFILE` names a profile that
doesn't exist, that shows up as a Problem. Den sees the `AWS_PROFILE` of the terminal you
started it from.

**Kubernetes** lists your contexts from `KUBECONFIG` or `~/.kube/config`, the current one
first, highlighted with a `*` as in kubectl. For each: how it signs in (Exec, an auth provider, Client
cert, Token, Basic auth) and whether it is **Ready** or has a **Problem**, such as a
missing cluster or user, TLS verification turned off, a password stored in the file, or a
file other people can read.

Den never contacts AWS or a cluster, so it cannot tell you whether credentials still work.

## Docker

**Containers** show whether they are running, their image and Compose project; the panel
adds ports and mounts. **Images** show their size and how many containers use them, or
**Unused**. **Volumes** show their driver and which containers use them. The status line
shows disk use and how much `docker system df` says you could reclaim.

The lists refresh every 5 seconds while Docker is open; `r` refreshes now. If Docker is not
running, the status line says so and the last results stay.

## Toolchains

- **On PATH:** for about twenty common commands (`node`, `python3`, `go`, `java`, `cargo`,
  …), the copy your shell runs, who installed it (Homebrew, rustup, uv, nvm, mise, the
  system, …), its version, and any copies it hides. Problems include a manager's folder
  missing from PATH, a system copy hiding yours, and macOS placeholders such as `java`
  with no JDK behind it.
- **Rust:** your rustup toolchains, with targets and components, then the crates you
  installed with `cargo install`.
- **Packages:** global packages from npm, uv, pipx, bun and `go install`.

Den reads all of this from files and runs none of these tools. Commands that Homebrew
installed show its newer version, like `↑ 22.12.0`; nothing else is checked for updates.
It follows the usual variables, such as `CARGO_HOME`, `RUSTUP_HOME`,
`NPM_CONFIG_PREFIX` and `JAVA_HOME`.

## Making changes

`den` is always read-only. To make changes, run `dew` instead: the same Den, with write
actions allowed. The top bar shows a red **WRITE** the whole time. Then:

1. In Homebrew, press `x` for the actions. The selected package can be upgraded (when a
   newer version is known) or uninstalled; Homebrew as a whole can update its package data,
   remove dependencies nothing needs, and clean up old versions and downloads.
2. `Enter` shows what an action will do: the exact command and, where Homebrew has one,
   a dry run of it.
3. `y` runs it. Nothing else does: not `Enter`, not a second `x`.

While an action runs, `Esc` or `Ctrl-C` stops it, giving Homebrew 10 seconds to clean up.
Afterwards Den reloads Homebrew and logs the action to `~/.local/state/den/actions.log`:
the time, the command and whether it worked, never its output.

What Den will not do:

- Uninstall a package something else needs, or upgrade a pinned one.
- Run `brew update` on its own: refreshing Homebrew's data is an action you choose.
- Use `--force`, `--ignore-dependencies` or `--zap`, a shell, or sudo. If a cask needs
  your password, the action fails quickly and you can run the command yourself.

In `den` there are no actions at all: `x` does nothing. In `dew`, `c` in the actions copies
a command to the clipboard, if your terminal allows it, so you can run it yourself.

## What Den reads, and what it never does

Den only looks. The [README](../README.md#read-only-by-design) lists the few commands it
runs; everything else comes from files. By area:

- **Agents:** URLs, command arguments, environment values and credential helpers are
  withheld; only their names are shown. No MCP server, hook or helper is ever started.
- **AWS:** from the credentials file, only profile and key names are kept; the secret
  values are dropped as the file is read. SSO caches are never opened.
- **Kubernetes:** tokens, passwords, certificates and keys are noted as present, never
  kept. Server addresses are reduced to their kind (this machine, private network, public
  IP or host name) and port.
- **Docker:** container commands, environment and logs are not shown, and saved registry
  logins are dropped.
- **Toolchains:** private registry and git URLs, and local source paths, are withheld.

Files that link elsewhere are reported with their target, not followed.

## Small print

- **Making changes:** only in `dew`; see [Making changes](#making-changes).
- **Searching:** `Backspace` deletes, `Ctrl-U` clears, `Enter` or `Esc` finishes and keeps
  the search. `Tab` and `Shift+Tab` also finish it.
- **Going back:** `Esc` (or `Backspace`) steps back through your earlier selections and
  searches in this section, and returns from a jump such as Diagnostics → Packages.
  Switching areas or sections is not a step.
- **Aliases:** `F2` is `Tab`, `F3` is `Shift+Tab`. `Ctrl-C` quits from anywhere, even while
  searching.
- **Colours:** Den follows your terminal. It uses Catppuccin when the terminal reports
  24-bit colour (`COLORTERM=truecolor`), the light variant on a light background
  (`COLORFGBG`), the 16 basic colours otherwise, and none at all with `NO_COLOR`.
