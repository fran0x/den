# Den

[![CI](https://github.com/fran0x/den/actions/workflows/ci.yml/badge.svg)](https://github.com/fran0x/den/actions/workflows/ci.yml)

**A read-only terminal inspector for your developer machine.**

Den shows what is installed and how it is configured: Homebrew packages and their
dependencies, Codex and Claude Code side by side, cloud profiles, Docker and language
toolchains. It reads; it never installs, removes, logs in or runs what it finds.

## What it shows

| Area | Sections | At a glance |
| --- | --- | --- |
| **Homebrew** | Packages · Diagnostics | Tools you asked for, what depends on what, newer versions, missing and leftover packages |
| **Agents** | Skills · Instructions · Settings · MCP · Hooks · Plugins | Codex and Claude Code compared row by row, with one status vocabulary |
| **Cloud** | AWS · Kubernetes | AWS profiles and SSO sessions; kubeconfig contexts, how each signs in, and what is broken |
| **Docker** | Containers · Images · Volumes | State, usage and reclaimable space, refreshed while you watch |
| **Toolchains** | On PATH · Rust · Packages | Which `node`, `python3` or `cargo` wins and from where; rustup toolchains, `cargo install`, npm, uv, pipx, bun and go globals |

Every row is **Ready**, **Problem** or a plainer state, and the panel beside it explains
why: a role chain that loops, a context whose cluster is gone, a `java` that is only a
macOS stub, a system `python3` hiding Homebrew's.

## Read-only by design

- **Fixed commands only.** `brew info`, `missing`, `leaves` and `autoremove --dry-run`;
  `docker container ls`, `image ls`, `volume ls` and `system df`. Each is bounded in time
  and output. Everything else is read from files.
- **Nothing is called.** No AWS API, cluster, `kubectl`, exec plugin, credential helper,
  MCP server, hook or program found on PATH is ever run.
- **Secrets stay put.** Credential values, tokens, keys, server hosts and URLs are dropped
  as they are read or kept only as "set". Untrusted text is sanitized before display.
- **Unknown stays unknown.** Den reports evidence, never guesses: no count means "not
  recorded", not "safe to remove".

Homebrew itself may refresh its metadata when Den asks it for information. Details:
[evidence and privacy](docs/EVIDENCE.md).

## Run

Requires stable Rust (edition 2024) on macOS or Linux. `brew` and `docker` are optional;
an area whose tool is missing says so.

```sh
cargo run --release              # or: just run
cargo install --path .           # then: den
```

| Flag | Effect |
| --- | --- |
| `--project DIR` | Project for Agents (default: the current folder) |
| `--codex-profile NAME` | Inspect `$CODEX_HOME/NAME.config.toml` |
| `--skills` | Start in Agents; Homebrew loads when opened |
| `--help` | All flags, without starting the interface |

## Keys

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Next area / next section (each keeps its place) |
| `↑` `↓` `j` `k` · `Home` `End` | Move through the list |
| `PgUp` / `PgDn` | Scroll the information panel |
| `/` | Search; `Enter` or `Esc` keeps the query |
| `Esc` | Back within the section |
| `r` | Refresh |
| `?` | Help for where you are |
| `q` · `Ctrl-C` | Quit |

The full guide, area by area: [keyboard and behaviour](docs/KEYBOARD.md).

## Try it offline

Synthetic fixtures exercise every area without touching your machine:

```sh
just fixtures demo          # embedded Homebrew inventory
just fixtures claude        # Codex and Claude Code side by side
just fixtures aws           # AWS profiles
just fixtures kube          # Kubernetes contexts
just fixtures docker        # Docker listings
just fixtures toolchains    # PATH, Rust and global packages
```

`just fixtures` lists them all; without `just`, use the matching `--*-fixture DIR` flag.

## Develop

```sh
just check    # format, Clippy, tests, release build
just smoke    # offline terminal checks in a real PTY
```

CI runs both on Ubuntu and macOS, and `cargo deny` checks dependencies for advisories,
licenses and sources. Live checks against your own machine are opt-in
(`just live`). More in [development](docs/DEVELOPMENT.md), [roadmap](ROADMAP.md) and
[contributor rules](AGENTS.md).
