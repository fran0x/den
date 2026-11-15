# Security policy

Den reads sensitive places on purpose: AWS and kubeconfig files, Codex and Claude Code
settings, PATH and package metadata. Its promise is that it never runs what it finds,
contacts no service unless you ask it to check online, and never shows a secret. A break
in that promise is a security bug.

**Online checks** run only when you press `U` and then Enter. Den sends the names of your
Homebrew formulas and casks, crates.io crates, npm, bun, uv and pipx packages and Docker
images (only those named with a namespace or registry, such as `grafana/loki:3.5`) over
HTTPS to formulae.brew.sh, index.crates.io, registry.npmjs.org, pypi.org, Docker Hub
(registry-1.docker.io and auth.docker.io for an anonymous read token) and ghcr.io, and the
names of the languages it compares (rust, python, nodejs, ruby, go) to endoflife.date, and
for skills `npx skills` installed from GitHub, the repository and folder its lock file
records (such as `vercel-labs/skills/skills/find-skills`) to api.github.com without a
login, and to no other host: redirects are not followed, and a proxy set in `HTTPS_PROXY` is used.
Docker's saved logins are never used. Installed versions, paths and settings stay on the
machine; the comparison happens locally. The registries learn which packages you use.
The answers are kept in `$XDG_STATE_HOME/den/online.json`, private like the snapshot.

`den` is always read-only. Changes need the `dew` command (the top bar then shows WRITE),
and even there each action runs only when you choose it and confirm it (Enter, or the same
`x` or `u` again), after its dry run where the tool has one. Actions are a fixed list of Homebrew, Docker, rustup, cargo, npm,
uv and pipx commands built from checked names and IDs, run without a shell, sudo or a
terminal for input, and never with force. Toolchain programs run as the copy Den shows
winning on PATH, by its full path.

`den` changes nothing on the machine. It writes only its own files: the online results
above, and its snapshot, only when you press `S` and confirm:
`$XDG_STATE_HOME/den/snapshot.json` (`~/.local/state/den/snapshot.json`), private to you
(folder 0700, file 0600), replaced in one step and never written through a link. It holds
what the lists show: each row's name and its version or status. Some rows are named by a
local path, such as an instruction file or a PATH folder; what Den withholds is never in
it. `dew` also keeps `actions.log` there.

## Reporting

Please report vulnerabilities privately through
[GitHub's private vulnerability reporting](https://github.com/fran0x/den/security/advisories/new),
not in a public issue. Include the Den version, your OS, and the smallest configuration or
fixture that reproduces it. Replace real secrets with placeholders.

You can expect an acknowledgement within a week. Fixes are released as a patch version
with an advisory crediting you, unless you prefer otherwise.

## In scope

- A secret, token, credential value, private host or URL shown on screen or kept in memory
  beyond the parser, where the docs say it is withheld.
- Den running a program, helper, hook, plugin or MCP server, other than the fixed Homebrew
  and Docker listing commands and, in `dew`, the action you chose.
- Den contacting a network service without `U` and Enter, contacting a host other than the
  registries above, or sending anything but package names.
- A write action running in `den`, or in `dew` without being chosen and confirmed (Enter,
  or `x` or `u` pressed again in its dialog); or running anything other than the command
  its dialog showed.
- Den writing any file other than its snapshot (after `S` and Enter), its online results
  (after `U` and Enter) and, in `dew`, its action log; writing any of them through a link;
  or putting withheld values in them.
- Reading outside the files and folders documented for each area, following a link the
  docs say is rejected, or escaping a fixture root.
- Terminal escape sequences or other untrusted text reaching the terminal unsanitized.
- Den leaving the terminal broken after an error or panic.

## Supported versions

Only the latest release receives fixes.
