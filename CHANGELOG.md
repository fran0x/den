# Changelog

All notable changes to Den are recorded here. Versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.7.3] - 2026-11-15

- **Clearer scope:** the footer groups keys by what they apply to: this area (the view's
  keys, `d`, `r`) │ all areas (`S`, `u`) │ moving around. The actions dialog separates
  the row's actions from area-wide ones under headings, such as **This package** and
  **All of Homebrew**, and never opens on uninstall.

## [0.7.2] - 2026-11-15

- **Shorter help:** `?` is now a one-screen reference: short lines per area, where `u`
  looks, and what `dew` changes. The details stay in the guide.
- **README:** mentions where skills came from, what `u` sends to GitHub, every `dew` dry
  run, and links the guide's table of where updates come from.

## [0.7.1] - 2026-11-15

- **Keys in one place:** the bottom line, under a rule, lists every key that works where
  you are, in three groups: this view's │ Den's (`d`, `S`, `u`, `r`) │ moving around.
  `u:check online` moved there from the status line, and keys that were only in `?`
  (`S`, `u`, `r`, `PgUp/PgDn`, `V` in MCP, Diagnostics' `Enter`, `Space`, `o` and `c`)
  are listed too.
- **Help is a reference now:** `?` no longer repeats the keys. It explains each area and
  lists **where updates come from** for each kind of package, and what is never checked,
  so a missing `↑` is not mistaken for up to date. The guide has the same table.

## [0.7.0] - 2026-11-15

- **Agents › Skills:** each skill's panel says how it was **Installed**: with `npx skills`
  (source and date, from its lock file), by claude.ai sync, with `git clone`, or by hand
  when there is no record; and its **Author** when its `SKILL.md` names one
  (`metadata.author`). A **Git** line names the
  repository that keeps the folder, such as your dotfiles, with its remote as
  host/owner/repo; credentials in the URL are never shown.
- **Skills from GitHub online:** `u` also asks api.github.com whether a skill `npx skills`
  installed has changed there since, by comparing the folder's fingerprint (git's tree
  SHA) with the one in its lock file. A changed one shows **↑ newer on GitHub**.
- **Docker pull in `dew`:** on an image marked **↑ newer build**, `x` offers
  `docker image pull` for its tag. Running containers keep their build until recreated.
- **Help:** in `dew`, `?` lists everything `dew` can change, by area; in `den` it says
  how to get them. Toolchains help now mentions the endoflife.date checks.

## [0.6.1] - 2026-11-15

- **Agents:** skills Claude Code syncs from your claude.ai account
  (`~/.claude/skills/synced/<id>`) are listed one by one as **claude.ai sync** and checked
  against the sync's manifest. Before, the `synced` folder itself showed as a skill
  missing its `SKILL.md`.
- **README:** says plainly that `dew` changes your machine, and what `u` sends.

## [0.6.0] - 2026-11-14

- **Newer versions online:** `u` checks for newer versions and marks them `↑` with when
  they were checked. Den asks first, sends only names over HTTPS to fixed hosts, and keeps
  the answers in `~/.local/state/den/online.json` for the next run. `Esc` stops a check.
  This is the first time Den contacts a website itself, and only after `u` and `Enter`.
  - **Homebrew** formulas and casks (formulae.brew.sh). The answer replaces Homebrew's own
    data, which is only as current as the last `brew update`, in the Homebrew list, the
    Agents status line and Toolchains.
  - **Packages:** `cargo install` crates (index.crates.io), global npm and bun packages
    (registry.npmjs.org), and uv and pipx packages (pypi.org).
  - **Docker images** named with a namespace or registry (`grafana/loki`, `ghcr.io/…`)
    show **↑ newer build** when their tag points to a new build on Docker Hub or ghcr.io. Den
    asks without a login, and single names such as `postgres`, which may be local builds,
    are never sent. The image listing now runs with `--digests` for this.
  - **Release lines** (endoflife.date): Rust's `stable` and line toolchains (`1.98`) and
    the `node`, `python3`, `ruby` and `go` on PATH get the newest patch of their line, and
    **End of life** when the line is no longer supported.
- **Notices** now clear the whole status line instead of leaving the end of the old one.

## [0.5.0] - 2026-11-14

- **What changed:** after a refresh, new rows are marked `+` and changed ones `~`, with
  what they were before in the panel, and the status line counts them. `d` lists only the
  changed rows, then the removed ones, struck through. Covers Homebrew, Agents, AWS,
  Kubernetes, Docker and Toolchains; a container starting or stopping is not a change.
- **Snapshots:** `S` saves what Den has read, after asking, to
  `~/.local/state/den/snapshot.json` (private to you, never written through a link).
  The next run compares with it: `d` shows what changed since the snapshot, then since
  Den started. A file Den can't use is reported and left alone. This is the only file
  `den` writes.
- **Docker:** containers and images are now identified by name and tag rather than ID,
  so a recreated container counts as the same one.
- **`--inventory <file>` replaces `--snapshot <file>`** for loading a saved Den
  inventory, so it isn't confused with `S`. `--snapshot` is no longer accepted.

## [0.4.0] - 2026-11-14

- **Rust clean-up in `dew`:** in Toolchains › Rust, `x` on a toolchain offers
  `rustup update` (with `rustup check` as its dry run) and uninstalling it; on a crate,
  `cargo install --locked` to upgrade it and `cargo uninstall`. rustup and cargo run as the
  copy that wins on PATH, by its full path. Den refuses to uninstall the default toolchain
  or one a folder override uses, and only upgrades crates.io crates installed without
  chosen features.
- **Package clean-up in `dew`:** in Toolchains › Packages, `x` upgrades or uninstalls a
  global npm package (after npm's own `--dry-run`), a uv tool or a pipx package, with the
  manager that wins on PATH. npm and corepack are left to Node's own upgrade.
- **PATH and Homebrew:** in Toolchains › PATH, `x` on a command Homebrew installed offers
  that package's Homebrew actions, with Homebrew's dry run; both areas are read again
  afterwards.

## [0.3.1] - 2026-11-14

- **Homebrew:** `brew install fran0x/tap/den` installs `den` and `dew` from the release
  binaries.
- **Faster loading:** Homebrew and Docker output is read as soon as it arrives instead of
  every 20 ms. Homebrew now loads about as fast as `brew info` runs on its own (about
  0.55 s instead of 0.7 s on a machine with 625 KB of package data).

## [0.3.0] - 2026-11-13

- **Docker clean-up in `dew`:** `x` in Docker stops a running container, or removes a
  stopped container or an image or volume nothing uses (in Images, also every unused
  untagged image at once). One `docker` command on an exact ID or name, never `-f` or
  `prune`. Docker has no dry run, so the dialog says exactly what goes before Enter runs
  it. Docker is listed again afterwards.
- **Actions dialog:** opens with the first action selected in every area, so `x` then
  `Enter` runs it.
- **Loading:** Docker and Toolchains show the same spinner as Homebrew while they load.
- **README:** shorter, with one section for `den` and one for `dew`; the list of commands
  `den` runs moved to the guide.

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

[Unreleased]: https://github.com/fran0x/den/compare/v0.7.3...HEAD
[0.7.3]: https://github.com/fran0x/den/compare/v0.7.2...v0.7.3
[0.7.2]: https://github.com/fran0x/den/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/fran0x/den/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/fran0x/den/compare/v0.6.1...v0.7.0
[0.6.1]: https://github.com/fran0x/den/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/fran0x/den/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/fran0x/den/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/fran0x/den/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/fran0x/den/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/fran0x/den/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/fran0x/den/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fran0x/den/releases/tag/v0.1.0
