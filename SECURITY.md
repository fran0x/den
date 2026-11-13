# Security policy

Den reads sensitive places on purpose: AWS and kubeconfig files, Codex and Claude Code
settings, PATH and package metadata. Its promise is that it never runs what it finds,
never contacts a service, and never shows a secret. A break in that promise is a
security bug.

`den` is always read-only. Changes need the `dew` command, and even there write mode must be
switched on with `W` (the top bar then shows WRITE), and each action runs only after you
confirm it with `y`. Actions are a fixed list of Homebrew commands built from checked names,
run without a shell, sudo or a terminal for input.

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
- Den running a program, helper, hook, plugin or MCP server, or contacting a network
  service, other than the fixed Homebrew and Docker listing commands.
- A write action running in `den`, or in `dew` without write mode on or without its
  confirmation; or running anything other than the command its dialog showed.
- Reading outside the files and folders documented for each area, following a link the
  docs say is rejected, or escaping a fixture root.
- Terminal escape sequences or other untrusted text reaching the terminal unsanitized.
- Den leaving the terminal broken after an error or panic.

## Supported versions

Only the latest release receives fixes.
