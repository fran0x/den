# Evidence and privacy

How Den models what it reads, and what it deliberately withholds. Per-area behaviour is in
the [keyboard guide](KEYBOARD.md); the rules contributors follow are in
[AGENTS.md](../AGENTS.md).

## Homebrew, Codex and MCP

Homebrew intent uses installation records; unknown stays unknown. Runtime relationships
and current declarations stay separate. Diagnostics never establish safe removal.
Homebrew may fetch metadata.

Codex models supported settings from [local configuration layers](https://learn.chatgpt.com/docs/config-file/config-advanced):
trusted project, selected profile, user and system files. Untrusted project layers are
inactive; unknown trust is conditional. Runtime overrides remain unobserved;
unsupported values and unestablished merge/path semantics remain unresolved. When
applicable configuration is malformed, unavailable or a selected profile is missing,
inventory and Overview mark retained values/defaults as **provisional**. Named permission
profiles (`default_permissions` / `[permissions]`) are not currently modeled.

[Instruction chains](https://learn.chatgpt.com/docs/agent-configuration/agents-md) combine one selected
file per directory, global-to-project, with byte accounting and best-effort redacted previews.

MCP inspection models a supported subset of the [configuration schema](https://learn.chatgpt.com/docs/config-file/config-reference).
Stdio/HTTP definitions retain source and trust evidence. Multi-layer merges remain unresolved;
plugin overrides do not establish an installed server. Policies restrict configured tool names,
not a discovered tool inventory. Authentication availability is never checked.

TOML uses a display allowlist. Arguments, inline environment/header values, OAuth secrets,
helper commands and raw endpoint URLs are withheld. MCP debug output is aggregate only;
configured endpoints show scheme, network category and port. Credential stores and
sessions are not queried.

Skill-declared MCP URL fields show `[URL withheld]`, retaining presence without URL
validation or connection checks. Raw `agents/openai.yaml` previews are withheld; normalized
declarations remain available in Evidence. Instruction-chain previews use best-effort
redaction. `SKILL.md` and ordinary supporting-text previews are terminal-sanitized,
but may contain private prose; their contents are not universally secret-redacted.

Reads and traversal are bounded; configuration file symlinks are rejected. MCP limits:
1,024 definitions, 256 entries per collection field. `url` parses endpoints without DNS.
System/plugin coverage remains partial; eligible never means connected or invoked.
See the [roadmap](../ROADMAP.md) for filesystem limits and deliberate modeling gaps.

## Claude Code

Claude Code skills come from `~/.claude/skills` and the project's `.claude/skills`.
Claude Code instructions are `CLAUDE.md` files in reading order:
managed, user (`~/.claude/CLAUDE.md`), then `CLAUDE.md`, `.claude/CLAUDE.md` and
`CLAUDE.local.md` from the outermost parent folder down. Previews are redacted; `@imports`
are counted, not followed; linked files are reported, not read.

Claude **Settings** resolves managed > local > project > user files (command-line
`--settings` is not observed); lists such as `permissions.allow` combine, and unreadable
files are flagged. **MCP** lists servers from `managed-mcp.json`, `~/.claude.json`, the
project's `.mcp.json` and enabled plugins, with collisions, approvals and trust; URLs,
arguments and values are withheld. **Hooks** lists every hook from settings and enabled
plugins, with commands redacted. **Plugins** lists installed plugins, whether settings
enable them, and their components. From `~/.claude.json` Den keeps only MCP servers, this
project's MCP approvals and its trust flag; account and session data are discarded.
Nothing is run.
