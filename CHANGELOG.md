# Changelog

All notable changes to Den are recorded here. Versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

- **Docker clean-up in `dew`:** `x` in Docker stops a running container, or removes a
  stopped container or an image or volume nothing uses (in Images, also every unused
  untagged image at once). One `docker` command on an exact ID or name, never `-f` or
  `prune`. Docker has no dry run, so the dialog opens with nothing chosen: choose with ↑↓,
  read what goes, then Enter. Docker is listed again afterwards.

## [0.2.0] - 2026-11-13

- **Write actions, opt-in:** run `dew` instead of `den` (`den` stays read-only and offers
  no actions; `dew` shows WRITE in the top bar). In Homebrew, `x` shows the selected
  package's actions: upgrade it, update Homebrew's data, remove unneeded dependencies,
  clean up, and, always last, uninstall it. The chosen action shows its exact command
  and a dry run where Homebrew has one, and `Enter` runs it. Uninstall is refused while
  something needs the package; pinned packages are not upgraded. A spinner shows a dry
  run or an action at work; Esc or Ctrl-C stops a running action, giving Homebrew 10
  seconds to clean up. Results are logged to `~/.local/state/den/actions.log` (time,
  command, result).
- **Updates, offline:** Agents names a newer Codex or Claude Code when Homebrew installed
  it, from Homebrew's data or Codex's own update check (`~/.codex/version.json`).
  Toolchains marks commands Homebrew can update (`↑ 22.12.0`), and Homebrew's status
  line says how old its data is. Nothing is fetched.
- **Cloud › AWS:** the active profile (from `AWS_PROFILE`, or `default`) comes first,
  starred (`*`), and its status says `active`. An `AWS_PROFILE` naming a profile that
  doesn't exist is a Problem row, and the status line names the variable that chose the
  profile. The current Kubernetes context is starred too.
- **Fixed: casks missing from Tools.** Homebrew keeps no installed-on-request record for
  casks, so every cask showed as Unknown and appeared only under All. A cask now counts
  as a tool unless another installed package needs it, and says **No request record**
  instead of Unknown.
- **Colours:** casks have their own colour in Homebrew lists (italic without colour), and
  the active AWS profile and current Kubernetes context are highlighted (underlined
  without colour), alongside their `*`.
- **Homebrew Diagnostics:** the Unresolved header says how many references are needed at
  run time; the rest (build, test and optional dependencies, upgraded versions) are
  normal. Counts read naturally too: "2 entries", not "2 entrys".
- **Toolchains:** the first section is called **PATH** (was On PATH).

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

[Unreleased]: https://github.com/fran0x/den/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/fran0x/den/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fran0x/den/releases/tag/v0.1.0
