# Den

[![CI](https://github.com/fran0x/den/actions/workflows/ci.yml/badge.svg)](https://github.com/fran0x/den/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/den-tui.svg)](https://crates.io/crates/den-tui)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**A read-only terminal inspector for your developer machine.**

Den shows what is installed and how it is configured: Homebrew packages and their
dependencies, Codex and Claude Code side by side, cloud profiles, Docker and language
toolchains. It reads; it never installs, removes, logs in or runs what it finds.

![Den comparing Codex and Claude Code MCP servers](docs/images/agents-mcp.svg)

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

![Den showing which commands win on PATH](docs/images/toolchains.svg)

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

Homebrew itself may refresh its metadata when Den asks it for information. Per-area
details are in the [guide](docs/KEYBOARD.md).

## Run

Den runs on macOS and Linux. `brew` and `docker` are optional;
an area whose tool is missing says so.

```sh
cargo install den-tui            # installs the `den` command
den
```

From a checkout: `cargo run --release` or `just run`. Den needs Rust 1.90 or newer.

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
(`just live`). Contributor rules, including what Den may run and read, are in
[AGENTS.md](AGENTS.md).

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at
your option. Contributions are accepted under the same terms. Report vulnerabilities
privately; see the [security policy](SECURITY.md).
