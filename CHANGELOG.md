# Changelog

All notable changes to Den are recorded here. Versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

- **Fixed: casks missing from Tools.** Homebrew keeps no installed-on-request record for
  casks, so every cask showed as Unknown and appeared only under All. A cask now counts as
  a tool unless another installed package needs it, and says **No request record**
  instead of Unknown.
- **Write actions, opt-in:** start Den with `--allow-writes`, press `W` (the top bar shows
  WRITE), then `x` in Homebrew: upgrade or uninstall the selected package, update
  Homebrew's data, remove unneeded dependencies, or clean up. Each action shows its exact
  command and a dry run where Homebrew has one, and runs only on `y`. Uninstall is refused
  while something needs the package; pinned packages are not upgraded. Esc or Ctrl-C
  stops a running action, giving Homebrew 10 seconds to clean up. Results are logged to
  `~/.local/state/den/actions.log` (time, command, result). Without the flag, `x` only
  copies commands to the clipboard.
- **Updates, offline:** Agents names a newer Codex or Claude Code when Homebrew installed
  it, from Homebrew's data or Codex's own update check (`~/.codex/version.json`).
  Toolchains marks commands Homebrew can update (`↑ 22.12.0`), and Homebrew's status line
  says how old its data is. Nothing is fetched.
- **Cloud › AWS:** the active profile (from `AWS_PROFILE`, or `default`) comes first, starred
  (`*`), and its status says `active`. An `AWS_PROFILE` naming a profile that doesn't
  exist is a Problem row, and the status line names the variable that chose the profile.
  The current Kubernetes context is starred too.

## [0.1.0] - 2026-11-12

First public release. Den is a read-only terminal inspector with five areas:

- **Homebrew:** Packages (tools you asked for, dependency trees, newer versions) and
  Diagnostics (unresolved links, missing dependencies, leaves, cleanup preview).
- **Agents:** Codex and Claude Code side by side: skills, instructions, settings, MCP
  servers, hooks and plugins.
- **Cloud:** AWS profiles and SSO sessions; Kubernetes contexts from kubeconfig files.
- **Docker:** containers, images and volumes, refreshed every 5 seconds while open.
- **Toolchains:** which commands win on PATH, Rust toolchains and `cargo install` crates,
  and global npm, uv, pipx, bun and go packages.

[Unreleased]: https://github.com/fran0x/den/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/fran0x/den/releases/tag/v0.1.0
