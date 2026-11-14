# Den

[![CI](https://github.com/fran0x/den/actions/workflows/ci.yml/badge.svg)](https://github.com/fran0x/den/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/den-tui.svg)](https://crates.io/crates/den-tui)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**A terminal inspector for your developer machine.** Two commands:

- **`den`** shows what is installed and how it is configured. It never changes anything.
- **`dew`** is the same, plus a few clean-up actions you choose and confirm.

## Install

```sh
brew install fran0x/tap/den      # Homebrew: installs den and dew
cargo install --locked den-tui   # or from crates.io (Rust 1.90+)
```

Or download a binary for macOS or Linux from
[Releases](https://github.com/fran0x/den/releases). `brew` and `docker` are optional: an
area whose tool is missing says so.

## den: read-only

![den showing which commands win on PATH](docs/images/toolchains.svg)

| Area | What you see |
| --- | --- |
| **Homebrew** | Packages you asked for, what depends on what, newer versions, leftovers |
| **Agents** | Codex and Claude Code side by side: skills, instructions, settings, MCP, hooks, plugins |
| **Cloud** | AWS profiles (active one first) and Kubernetes contexts |
| **Docker** | Containers, images, volumes and reclaimable space |
| **Toolchains** | Which `node`, `python3` or `cargo` wins on PATH; rustup, cargo, npm, uv, pipx, bun and go installs |

When something is wrong, the panel beside the row says why.

**What it promises:**

- Runs only a fixed set of read commands (`brew info`, `docker container ls`, …); the
  rest comes from files.
- Never calls AWS, a cluster, an MCP server, a hook or anything it finds on PATH.
- Never shows secrets: tokens, keys and credential values are dropped as they are read.

**Keys:** `Tab` areas · `Shift+Tab` sections · `/` search · `?` help · `q` quit.
Try it without reading your machine: `den --demo`.

## dew: with changes

![dew showing ripgrep's actions and the upgrade's dry run](docs/images/dew.svg)

`dew` shows WRITE in the top bar. Select a row, press `x` to see its actions, then `Enter`
to run the selected one.

| Area | Actions |
| --- | --- |
| **Homebrew** | Upgrade, uninstall, update Homebrew's data, remove unneeded dependencies, clean up |
| **Docker** | Stop a running container; remove a stopped container, or an image or volume nothing uses |
| **Toolchains** | Update or uninstall a Rust toolchain; upgrade or uninstall a `cargo install` crate |

**What it promises:**

- Shows the exact command first, and Homebrew's dry run where there is one.
- Refuses what is in use: no uninstall while something needs the package, no removing a
  used image or volume, the default toolchain or one a folder override uses.
- Never uses `--force`, `prune`, a shell or sudo. `Esc` stops a running action.
- Logs each action to `~/.local/state/den/actions.log` (time, command, result).

## More

- [Guide](docs/GUIDE.md): every area, key and action in detail.
- [Security policy](SECURITY.md): what Den may read and run; report issues privately.
- Develop: `just check`, `just smoke`, `just ci`. Rules are in [AGENTS.md](AGENTS.md).

## License

[Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
Contributions are accepted under the same terms.
