//! Claude Code settings, MCP servers, hooks and plugins, from local files only.
//!
//! Rules follow Claude Code's documentation (code.claude.com/docs): settings precedence is
//! managed > command line > local > project > user; lists from several files combine;
//! MCP names resolve managed > local > project > user > plugin. Command-line flags and
//! what a running session loaded are not observed.
//!
//! Privacy: `~/.claude.json` also holds account and session state. Only `mcpServers`, and
//! this project's MCP approvals and trust flag, are taken from it; nothing else is kept.
//! MCP values are summarised (program name, counts, variable and header names); URLs,
//! arguments and values are withheld. Commands (hooks, status line) are shown through
//! the same best-effort redaction as instruction previews; credential helpers are not.
use super::{Context, read_limited};
use crate::{codex::privacy, inventory::safe_text, skills::filesystem};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const SETTINGS_LIMIT: usize = 1024 * 1024;
/// `~/.claude.json` grows with usage history; it is parsed, then dropped.
const CLAUDE_JSON_LIMIT: usize = 16 * 1024 * 1024;
const MAX_PLUGINS: usize = 256;

/// One row in a Claude section: name, source column, summary and panel text.
#[derive(Clone, Debug, Default)]
pub struct Item {
    pub name: String,
    pub source: String,
    pub summary: String,
    pub detail: String,
    /// Stable identity for selection across refreshes.
    pub key: String,
    /// Shown first and called out: a problem or something that runs commands.
    pub warning: bool,
    /// MCP only: transport and program name, to compare with another tool's server.
    pub endpoint: Option<(String, Option<String>)>,
}

#[derive(Default)]
pub struct Config {
    pub settings: Vec<Item>,
    pub mcp: Vec<Item>,
    pub hooks: Vec<Item>,
    pub plugins: Vec<Item>,
    /// Which settings files exist, e.g. "User, Local".
    pub files: String,
    pub coverage: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    User,
    Project,
    Local,
    Managed,
}
impl Layer {
    fn label(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Project => "Project",
            Self::Local => "Local",
            Self::Managed => "Managed",
        }
    }
}

struct Settings {
    layer: Layer,
    path: PathBuf,
    value: Map<String, Value>,
}

pub fn collect(context: &Context, cancel: &AtomicBool) -> io::Result<Config> {
    let mut config = Config::default();
    let (layers, mut problems) = settings_files(context);
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("collection cancelled"));
    }
    let mut found: Vec<_> = layers.iter().map(|l| l.layer.label()).collect();
    found.dedup();
    config.files = if found.is_empty() {
        "No settings files".into()
    } else {
        format!("Files: {}", found.join(", "))
    };
    let claude_json = claude_json(context, &mut problems);
    config.settings = problems;
    config.settings.extend(settings(&layers, context));
    let plugins = plugins(context, &layers);
    config.hooks = hooks(&layers, &plugins, context);
    config.mcp = mcp(context, &layers, &claude_json, &plugins);
    config.plugins = plugins.into_iter().map(|p| p.item).collect();
    config.coverage = vec![
        "Settings: managed-settings.json and managed-settings.d, ~/.claude/settings.json, and this project's .claude/settings.json and settings.local.json. Command-line --settings, MDM/registry and server-managed policy are not observed.".into(),
        "MCP: managed-mcp.json, ~/.claude.json (user and this project's local servers), the project's .mcp.json and enabled plugins' .mcp.json. allowedMcpServers/deniedMcpServers are not applied.".into(),
        "Hooks: settings files and enabled plugins' hooks/hooks.json. Skill and subagent frontmatter hooks are not listed.".into(),
        "Plugins: installed_plugins.json and known_marketplaces.json; manifest path overrides are not applied.".into(),
        "Nothing is run: hooks, status line, credential helpers and MCP servers are read as data.".into(),
    ];
    Ok(config)
}

/// "1 key", "2 keys".
fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}

fn short(context: &Context, path: &Path) -> String {
    super::short(context, path)
}

fn parse(context: &Context, path: &Path, limit: usize) -> Result<Option<Value>, &'static str> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Ok(m) if m.file_type().is_symlink() => return Err("link; not read"),
        _ => {}
    }
    let text = read_limited(context, path, limit).map_err(|_| "unreadable or too large")?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|_| "invalid JSON")
}

fn problem(context: &Context, path: &Path, why: &str) -> Item {
    Item {
        name: short(context, path),
        source: "File".into(),
        summary: format!("Not used: {why}"),
        detail: format!(
            "{}\n\nThis file could not be used: {why}. Values from it are not shown, so the settings below may be incomplete.",
            short(context, path)
        ),
        key: format!("file:{}", path.display()),
        warning: true,
        endpoint: None,
    }
}

/// Settings files from lowest to highest precedence.
fn settings_files(context: &Context) -> (Vec<Settings>, Vec<Item>) {
    let mut candidates = vec![];
    if let Some(home) = &context.claude_home {
        candidates.push((Layer::User, home.join("settings.json")));
    }
    candidates.push((
        Layer::Project,
        context.project.join(".claude/settings.json"),
    ));
    candidates.push((
        Layer::Local,
        context.project.join(".claude/settings.local.json"),
    ));
    if let Some(dir) = &context.managed {
        candidates.push((Layer::Managed, dir.join("managed-settings.json")));
        for name in json_names(context, &dir.join("managed-settings.d")) {
            candidates.push((Layer::Managed, dir.join("managed-settings.d").join(name)));
        }
    }
    let mut layers = vec![];
    let mut problems = vec![];
    for (layer, path) in candidates {
        if !context.permits(&path) {
            continue;
        }
        match parse(context, &path, SETTINGS_LIMIT) {
            Ok(None) => {}
            Ok(Some(Value::Object(value))) => layers.push(Settings { layer, path, value }),
            Ok(Some(_)) => problems.push(problem(context, &path, "not a JSON object")),
            Err(why) => problems.push(problem(context, &path, why)),
        }
    }
    (layers, problems)
}

/// Sorted `*.json` names in a folder; absent or unreadable folders give none.
fn json_names(context: &Context, dir: &Path) -> Vec<String> {
    dir_names(context, dir)
        .into_iter()
        .filter(|n| n.ends_with(".json") && !n.starts_with('.'))
        .collect()
}

fn dir_names(context: &Context, dir: &Path) -> Vec<String> {
    let Ok(resolved) = filesystem::resolve(dir) else {
        return vec![];
    };
    if !context.permits(&resolved) {
        return vec![];
    }
    #[cfg(unix)]
    {
        filesystem::Directory::open(&resolved)
            .and_then(|d| d.names(512))
            .map(|(names, _)| {
                names
                    .into_iter()
                    .map(|n| n.to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(not(unix))]
    vec![]
}

fn get<'a>(map: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut parts = path.split('.');
    let mut value = map.get(parts.next()?)?;
    for part in parts {
        value = value.as_object()?.get(part)?;
    }
    Some(value)
}

/// A scalar setting value for display, redacted like instruction text.
fn scalar(value: &Value) -> String {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Bool(_) | Value::Number(_) | Value::Null => value.to_string(),
        Value::Array(a) => return format!("{} entries", a.len()),
        Value::Object(o) => return format!("{} keys", o.len()),
    };
    let mut text = privacy::preview(&text);
    if text.chars().count() > 120 {
        text = text.chars().take(119).collect::<String>() + "…";
    }
    text
}

/// The highest-precedence layer that sets `key`, and its value.
fn effective<'a>(layers: &'a [Settings], key: &str) -> Option<(&'a Settings, &'a Value)> {
    layers
        .iter()
        .rev()
        .find_map(|l| get(&l.value, key).map(|v| (l, v)))
}
fn effective_bool(layers: &[Settings], key: &str) -> Option<(Layer, bool)> {
    effective(layers, key).and_then(|(l, v)| v.as_bool().map(|b| (l.layer, b)))
}
/// Every string in a list setting across layers, with its layer.
fn combined(layers: &[Settings], key: &str) -> Vec<(Layer, String)> {
    layers
        .iter()
        .flat_map(|l| {
            get(&l.value, key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(|s| (l.layer, s.to_owned())))
        })
        .collect()
}

const SCALARS: &[(&str, &str)] = &[
    ("model", "Model Claude Code starts with"),
    (
        "permissions.defaultMode",
        "Default permission mode for new sessions",
    ),
    (
        "permissions.disableBypassPermissionsMode",
        "Whether bypass-permissions mode is blocked",
    ),
    ("outputStyle", "Output style"),
    ("effortLevel", "Reasoning effort"),
    ("language", "Response language"),
    ("alwaysThinkingEnabled", "Extended thinking by default"),
    ("autoUpdatesChannel", "Update channel"),
    ("cleanupPeriodDays", "Days to keep session transcripts"),
    ("includeCoAuthoredBy", "Co-authored-by line in commits"),
    ("forceLoginMethod", "Allowed login method"),
    (
        "enableAllProjectMcpServers",
        "Approve every server in project .mcp.json files",
    ),
    (
        "disableAllHooks",
        "Turn off hooks and the custom status line",
    ),
    ("allowManagedHooksOnly", "Only managed hooks may run"),
    (
        "allowManagedMcpServersOnly",
        "Only managed MCP servers may run",
    ),
    (
        "allowManagedPermissionRulesOnly",
        "Only managed permission rules apply",
    ),
];
const LISTS: &[(&str, &str)] = &[
    ("permissions.allow", "Tool uses approved without a prompt"),
    ("permissions.ask", "Tool uses that always prompt"),
    ("permissions.deny", "Tool uses that are blocked"),
    (
        "permissions.additionalDirectories",
        "Extra folders Claude may access",
    ),
    (
        "enabledMcpjsonServers",
        "Project .mcp.json servers approved",
    ),
    (
        "disabledMcpjsonServers",
        "Project .mcp.json servers rejected",
    ),
];
/// Settings that run a command. Credential helpers are never shown.
const HELPERS: &[(&str, &str)] = &[
    ("apiKeyHelper", "Generates the API credential"),
    ("awsAuthRefresh", "Refreshes Bedrock credentials"),
    ("awsCredentialExport", "Exports AWS credentials"),
    ("otelHeadersHelper", "Generates telemetry headers"),
];
/// Shown in their own sections or items; not repeated under "Other settings".
const HANDLED: &[&str] = &[
    "permissions",
    "env",
    "hooks",
    "statusLine",
    "enabledPlugins",
    "$schema",
];

fn settings(layers: &[Settings], context: &Context) -> Vec<Item> {
    let mut items = vec![];
    for (key, meaning) in SCALARS {
        let Some((winner, value)) = effective(layers, key) else {
            continue;
        };
        let mut detail = format!("{key}\n\n{meaning}\n\nEffective: {}", scalar(value));
        detail.push_str(&format!(
            "\nFrom: {} ({})",
            winner.layer.label(),
            short(context, &winner.path)
        ));
        let overridden: Vec<_> = layers
            .iter()
            .filter(|l| l.layer != winner.layer)
            .filter_map(|l| {
                get(&l.value, key).map(|v| format!("{}: {}", l.layer.label(), scalar(v)))
            })
            .collect();
        if !overridden.is_empty() {
            detail.push_str(&format!("\nOverridden: {}", overridden.join(" · ")));
        }
        if *key == "permissions.defaultMode"
            && matches!(winner.layer, Layer::Project | Layer::Local)
            && matches!(value.as_str(), Some("auto" | "bypassPermissions"))
        {
            detail.push_str("\nNote: auto and bypassPermissions do not take effect from project or local settings.");
        }
        items.push(Item {
            name: (*key).into(),
            source: winner.layer.label().into(),
            summary: scalar(value),
            detail,
            key: format!("setting:{key}"),
            warning: false,
            endpoint: None,
        });
    }
    for (key, meaning) in LISTS {
        let entries = combined(layers, key);
        if entries.is_empty() {
            continue;
        }
        let mut sources: Vec<_> = entries.iter().map(|(l, _)| l.label()).collect();
        sources.dedup();
        let detail = format!(
            "{key}\n\n{meaning}\n\nLists from every settings file combine.\n\n{}",
            entries
                .iter()
                .map(|(l, e)| format!("{} · {}", l.label(), privacy::preview(e)))
                .collect::<Vec<_>>()
                .join("\n")
        );
        items.push(Item {
            name: (*key).into(),
            source: if sources.len() == 1 {
                sources[0].into()
            } else {
                "Combined".into()
            },
            summary: if entries.len() == 1 {
                "1 entry".into()
            } else {
                format!("{} entries", entries.len())
            },
            detail,
            key: format!("setting:{key}"),
            warning: false,
            endpoint: None,
        });
    }
    for (key, meaning) in HELPERS {
        if let Some((winner, _)) = effective(layers, key) {
            items.push(Item {
                name: (*key).into(),
                source: winner.layer.label().into(),
                summary: "Runs a command · withheld".into(),
                detail: format!(
                    "{key}\n\n{meaning}. Claude Code runs this command; it handles credentials, so Den does not show it.\n\nFrom: {} ({})",
                    winner.layer.label(),
                    short(context, &winner.path)
                ),
                key: format!("setting:{key}"),
                warning: true,
                endpoint: None,
            });
        }
    }
    if let Some((winner, value)) = effective(layers, "statusLine") {
        let command = value
            .get("command")
            .and_then(Value::as_str)
            .map(privacy::preview)
            .unwrap_or_else(|| "not set".into());
        items.push(Item {
            name: "statusLine".into(),
            source: winner.layer.label().into(),
            summary: "Runs a command".into(),
            detail: format!(
                "statusLine\n\nClaude Code runs this command to draw its status line. Den does not run it.\n\nCommand (redacted): {command}\nFrom: {} ({})",
                winner.layer.label(),
                short(context, &winner.path)
            ),
            key: "setting:statusLine".into(),
            warning: true,
            endpoint: None,
        });
    }
    let mut env: BTreeMap<String, Layer> = BTreeMap::new();
    for l in layers {
        if let Some(map) = l.value.get("env").and_then(Value::as_object) {
            for name in map.keys() {
                env.insert(safe_text(name), l.layer);
            }
        }
    }
    if !env.is_empty() {
        items.push(Item {
            name: "env".into(),
            source: "Combined".into(),
            summary: format!("{} · values withheld", count(env.len(), "variable")),
            detail: format!(
                "env\n\nEnvironment variables set for every session and its subprocesses. Values are withheld.\n\n{}",
                env.iter()
                    .map(|(name, layer)| format!("{} · {name}", layer.label()))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            key: "setting:env".into(),
            warning: false,
            endpoint: None,
        });
    }
    let shown: Vec<&str> = SCALARS
        .iter()
        .chain(LISTS)
        .chain(HELPERS)
        .map(|(k, _)| k.split('.').next().unwrap_or(k))
        .chain(HANDLED.iter().copied())
        .collect();
    let mut other: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for l in layers {
        for name in l.value.keys().filter(|k| !shown.contains(&k.as_str())) {
            other
                .entry(safe_text(name))
                .or_default()
                .push(l.layer.label());
        }
    }
    if !other.is_empty() {
        items.push(Item {
            name: "Other settings".into(),
            source: "Combined".into(),
            summary: format!("{} · values not shown", count(other.len(), "key")),
            detail: format!(
                "Other settings\n\nKeys Den does not interpret. Values are not shown.\n\n{}",
                other
                    .iter()
                    .map(|(name, layers)| format!("{name} · {}", layers.join(", ")))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            key: "setting:other".into(),
            warning: false,
            endpoint: None,
        });
    }
    items
}

/// What Den keeps from `~/.claude.json`: user servers and this project's MCP state.
#[derive(Default)]
struct ClaudeJson {
    path: Option<PathBuf>,
    user: Map<String, Value>,
    local: Map<String, Value>,
    approved: Vec<String>,
    rejected: Vec<String>,
    turned_off: Vec<String>,
    trusted: Option<bool>,
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

fn claude_json(context: &Context, problems: &mut Vec<Item>) -> ClaudeJson {
    let mut kept = ClaudeJson::default();
    let Some(path) = context.claude_json.clone().filter(|p| context.permits(p)) else {
        return kept;
    };
    let value = match parse(context, &path, CLAUDE_JSON_LIMIT) {
        Ok(Some(Value::Object(value))) => value,
        Ok(None) => return kept,
        Ok(Some(_)) => {
            problems.push(problem(context, &path, "not a JSON object"));
            return kept;
        }
        Err(why) => {
            problems.push(problem(context, &path, why));
            return kept;
        }
    };
    if let Some(Value::Object(servers)) = value.get("mcpServers") {
        kept.user = servers.clone();
    }
    // Claude Code keys projects by absolute path. Fixtures cannot store one, so inside a
    // fixture the path relative to the fixture root is accepted too.
    let relative = context
        .boundary
        .as_ref()
        .and_then(|root| context.project.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().into_owned());
    let project = value
        .get("projects")
        .and_then(Value::as_object)
        .and_then(|p| {
            p.get(context.project.to_string_lossy().as_ref())
                .or_else(|| relative.as_deref().and_then(|r| p.get(r)))
        })
        .and_then(Value::as_object);
    if let Some(project) = project {
        if let Some(Value::Object(servers)) = project.get("mcpServers") {
            kept.local = servers.clone();
        }
        kept.approved = strings(project.get("enabledMcpjsonServers"));
        kept.rejected = strings(project.get("disabledMcpjsonServers"));
        kept.turned_off = strings(project.get("disabledMcpServers"));
        kept.trusted = project
            .get("hasTrustDialogAccepted")
            .and_then(Value::as_bool);
    }
    kept.path = Some(path);
    // Everything else in the file, including account data, is dropped here.
    kept
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Scope {
    Managed,
    Local,
    Project,
    User,
    Plugin,
}
impl Scope {
    fn label(self) -> &'static str {
        match self {
            Self::Managed => "Managed",
            Self::Local => "Local",
            Self::Project => "Project",
            Self::User => "User",
            Self::Plugin => "Plugin",
        }
    }
}

/// Transport and program name of an MCP entry, for matching across tools.
fn endpoint(entry: &Value) -> (String, Option<String>) {
    let kind = entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(if entry.get("url").is_some() {
            "url"
        } else {
            "stdio"
        })
        .to_ascii_lowercase();
    let program = entry
        .get("command")
        .and_then(Value::as_str)
        .and_then(|c| c.split_whitespace().next())
        .and_then(|c| Path::new(c).file_name())
        .map(|n| safe_text(&n.to_string_lossy()));
    (kind, program)
}

/// A classified summary of one MCP server entry: no URLs, arguments or values.
fn server_summary(entry: &Value) -> (String, Vec<String>) {
    let Some(entry) = entry.as_object() else {
        return ("Invalid entry".into(), vec!["Not a JSON object".into()]);
    };
    let kind = entry
        .get("type")
        .and_then(Value::as_str)
        .map(safe_text)
        .unwrap_or_else(|| {
            if entry.contains_key("url") {
                "url without type".into()
            } else {
                "stdio".into()
            }
        });
    let mut lines = vec![format!("Transport: {kind}")];
    let mut summary = kind.clone();
    if let Some(command) = entry.get("command").and_then(Value::as_str) {
        let program = Path::new(command.split_whitespace().next().unwrap_or(""))
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let program = if program.contains('=') || program.contains("://") || program.len() > 40 {
            "withheld".into()
        } else {
            safe_text(&program)
        };
        lines.push(format!("Program: {program}"));
        summary = format!("{kind} · {program}");
        let args = entry
            .get("args")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        lines.push(format!("Arguments: {args} (withheld)"));
    }
    if entry.contains_key("url") {
        lines.push("URL: withheld".into());
        if !entry.contains_key("type") {
            lines.push("A url without a type is skipped by Claude Code.".into());
        }
    }
    for (field, label) in [("env", "Environment variables"), ("headers", "Headers")] {
        if let Some(map) = entry.get(field).and_then(Value::as_object) {
            lines.push(format!(
                "{label}: {} (values withheld)",
                map.keys()
                    .map(|k| safe_text(k))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if entry.contains_key("headersHelper") {
        lines.push("headersHelper: runs a command to generate headers (withheld)".into());
    }
    if entry.contains_key("oauth") {
        lines.push("OAuth: configured (details withheld)".into());
    }
    if serde_json::to_string(entry).is_ok_and(|s| s.contains("${")) {
        lines.push("Uses ${VAR} expansion; values come from the environment at launch".into());
    }
    (summary, lines)
}

fn mcp(
    context: &Context,
    layers: &[Settings],
    claude_json: &ClaudeJson,
    plugins: &[Plugin],
) -> Vec<Item> {
    let mut servers: Vec<(Scope, String, Value, String)> = vec![];
    let mut managed_file = false;
    let file_servers = |path: &Path,
                        scope: Scope,
                        servers: &mut Vec<_>,
                        problems: &mut Vec<Item>|
     -> bool {
        if !context.permits(path) {
            return false;
        }
        match parse(context, path, SETTINGS_LIMIT) {
            Ok(Some(value)) => {
                if let Some(Value::Object(map)) = value.get("mcpServers") {
                    for (name, entry) in map {
                        servers.push((scope, name.clone(), entry.clone(), short(context, path)));
                    }
                }
                true
            }
            Ok(None) => false,
            Err(why) => {
                problems.push(problem(context, path, why));
                false
            }
        }
    };
    let mut problems = vec![];
    if let Some(dir) = &context.managed {
        managed_file = file_servers(
            &dir.join("managed-mcp.json"),
            Scope::Managed,
            &mut servers,
            &mut problems,
        );
    }
    file_servers(
        &context.project.join(".mcp.json"),
        Scope::Project,
        &mut servers,
        &mut problems,
    );
    let json_source = claude_json
        .path
        .as_ref()
        .map(|p| short(context, p))
        .unwrap_or_default();
    for (name, entry) in &claude_json.user {
        servers.push((
            Scope::User,
            name.clone(),
            entry.clone(),
            json_source.clone(),
        ));
    }
    for (name, entry) in &claude_json.local {
        servers.push((
            Scope::Local,
            name.clone(),
            entry.clone(),
            format!("{json_source} (this project)"),
        ));
    }
    for plugin in plugins.iter().filter(|p| p.enabled == Some(true)) {
        for (name, entry) in &plugin.mcp {
            servers.push((
                Scope::Plugin,
                name.clone(),
                entry.clone(),
                format!("plugin {}", plugin.item.name),
            ));
        }
    }
    servers.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut rejected: Vec<_> = combined(layers, "disabledMcpjsonServers")
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    rejected.extend(claude_json.rejected.iter().cloned());
    let approved_settings = combined(layers, "enabledMcpjsonServers");
    let approve_all = effective_bool(layers, "enableAllProjectMcpServers");
    let mut items = problems;
    let mut winners: BTreeMap<String, Scope> = BTreeMap::new();
    for (scope, name, entry, source) in &servers {
        let winner = *winners.entry(name.clone()).or_insert(*scope);
        let (summary, mut lines) = server_summary(entry);
        let endpoint = endpoint(entry);
        let status = if winner != *scope {
            format!("Shadowed by {}", winner.label())
        } else if managed_file && *scope != Scope::Managed {
            "Blocked: managed-mcp.json is present".into()
        } else if claude_json.turned_off.contains(name) {
            "Turned off for this project".into()
        } else if *scope == Scope::Project {
            let trusted = claude_json.trusted == Some(true);
            if rejected.contains(name) {
                "Rejected (disabledMcpjsonServers)".into()
            } else if claude_json.approved.contains(name) {
                "Approved for this project".into()
            } else if let Some(layer) = approved_settings
                .iter()
                .find(|(_, n)| n == name)
                .map(|(l, _)| *l)
                .or(approve_all.filter(|(_, on)| *on).map(|(l, _)| l))
            {
                if layer == Layer::Project && !trusted {
                    "Approved by project settings, if the folder is trusted".into()
                } else {
                    format!("Approved in {} settings", layer.label())
                }
            } else {
                "Pending approval".into()
            }
        } else {
            "Configured".into()
        };
        lines.insert(0, format!("Status: {status}"));
        lines.push(format!("Defined in: {source}"));
        let name = safe_text(name);
        items.push(Item {
            summary: format!("{status} · {summary}"),
            detail: format!(
                "{name}\n\n{}\n\nName collisions resolve Managed > Local > Project > User > Plugin; the whole winning entry is used. Den never starts or connects to servers.",
                lines.join("\n")
            ),
            key: format!("mcp:{}:{name}", scope.label()),
            source: scope.label().into(),
            name,
            warning: false,
            endpoint: Some(endpoint),
        });
    }
    items
}

const EVENTS: &[(&str, &str)] = &[
    ("PreToolUse", "Before a tool runs; can block it"),
    ("PostToolUse", "After a tool succeeds"),
    ("PostToolUseFailure", "After a tool fails"),
    ("PermissionRequest", "When a permission prompt would appear"),
    ("UserPromptSubmit", "When you submit a prompt"),
    ("SessionStart", "When a session starts or resumes"),
    ("SessionEnd", "When a session ends"),
    ("Stop", "When Claude finishes responding"),
    ("SubagentStop", "When a subagent finishes"),
    ("Notification", "When Claude Code sends a notification"),
    ("PreCompact", "Before the conversation is compacted"),
];

/// Hook handlers in one `hooks` object, as (event, matcher, handler) triples.
fn handlers(hooks: &Value) -> Vec<(String, String, Map<String, Value>)> {
    let mut found = vec![];
    for (event, groups) in hooks.as_object().into_iter().flatten() {
        for group in groups.as_array().into_iter().flatten() {
            let matcher = group
                .get("matcher")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty() && *m != "*")
                .map(safe_text)
                .unwrap_or_else(|| "all".into());
            for handler in group
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
            {
                found.push((safe_text(event), matcher.clone(), handler.clone()));
            }
        }
    }
    found
}

fn hook_item(
    event: &str,
    matcher: &str,
    handler: &Map<String, Value>,
    source: &str,
    origin: &str,
    status: Option<&str>,
    index: usize,
) -> Item {
    let kind = handler
        .get("type")
        .and_then(Value::as_str)
        .map(safe_text)
        .unwrap_or_else(|| "unknown".into());
    let mut lines = vec![
        format!(
            "When: {}",
            EVENTS
                .iter()
                .find(|(e, _)| *e == event)
                .map_or("A Claude Code lifecycle event", |(_, m)| m)
        ),
        format!("Matcher: {matcher}"),
        format!("Type: {kind}"),
    ];
    let what = match kind.as_str() {
        "command" => {
            let mut command = handler
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            for arg in handler
                .get("args")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                command.push(' ');
                command.push_str(arg.as_str().unwrap_or("?"));
            }
            let shown = privacy::preview(&command);
            lines.push(format!("Command (redacted): {shown}"));
            shown
        }
        "http" => {
            lines.push("URL: withheld".into());
            "HTTP request".into()
        }
        "prompt" | "agent" => {
            let text = handler.get("prompt").and_then(Value::as_str).unwrap_or("");
            lines.push(format!("Prompt (redacted): {}", privacy::preview(text)));
            format!("{kind} check")
        }
        "mcp_tool" => {
            let tool = handler.get("tool").and_then(Value::as_str).unwrap_or("?");
            lines.push(format!("MCP tool: {}", safe_text(tool)));
            format!("MCP tool {}", safe_text(tool))
        }
        _ => "unrecognised handler".into(),
    };
    if let Some(timeout) = handler.get("timeout").and_then(Value::as_u64) {
        lines.push(format!("Timeout: {timeout}s"));
    }
    if let Some(condition) = handler.get("if").and_then(Value::as_str) {
        lines.push(format!("Only if: {}", privacy::preview(condition)));
    }
    if let Some(status) = status {
        lines.insert(0, format!("Status: {status}"));
    }
    lines.push(format!("Defined in: {origin}"));
    let first_line = what.lines().next().unwrap_or("").to_owned();
    Item {
        name: format!("{event} · {matcher}"),
        source: source.into(),
        summary: match status {
            Some(status) => format!("{status} · {first_line}"),
            None => first_line,
        },
        detail: format!(
            "{event} · {matcher}\n\n{}\n\nHooks run automatically with your permissions. Den shows them and never runs them.",
            lines.join("\n")
        ),
        key: format!("hook:{origin}:{index}"),
        warning: true,
        endpoint: None,
    }
}

fn hooks(layers: &[Settings], plugins: &[Plugin], context: &Context) -> Vec<Item> {
    let all_off = effective_bool(layers, "disableAllHooks").filter(|(_, b)| *b);
    let managed_only = layers.iter().any(|l| {
        l.layer == Layer::Managed
            && get(&l.value, "allowManagedHooksOnly") == Some(&Value::Bool(true))
    });
    let mut items = vec![];
    for l in layers {
        let Some(hooks) = l.value.get("hooks") else {
            continue;
        };
        let status = if l.layer != Layer::Managed && all_off.is_some() {
            Some("Off (disableAllHooks)")
        } else if l.layer != Layer::Managed && managed_only {
            Some("Blocked (allowManagedHooksOnly)")
        } else {
            None
        };
        for (n, (event, matcher, handler)) in handlers(hooks).into_iter().enumerate() {
            items.push(hook_item(
                &event,
                &matcher,
                &handler,
                l.layer.label(),
                &short(context, &l.path),
                status,
                n,
            ));
        }
    }
    for plugin in plugins {
        let status = match plugin.enabled {
            Some(true) if all_off.is_some() => Some("Off (disableAllHooks)"),
            Some(true) if managed_only => Some("Blocked (allowManagedHooksOnly)"),
            Some(true) => None,
            _ => Some("Plugin not enabled"),
        };
        for (n, (event, matcher, handler)) in plugin.hooks.iter().enumerate() {
            items.push(hook_item(
                event,
                matcher,
                handler,
                "Plugin",
                &format!("plugin {}", plugin.item.name),
                status,
                n,
            ));
        }
    }
    items
}

struct Plugin {
    item: Item,
    enabled: Option<bool>,
    hooks: Vec<(String, String, Map<String, Value>)>,
    mcp: Map<String, Value>,
}

fn plugins(context: &Context, layers: &[Settings]) -> Vec<Plugin> {
    let Some(dir) = context.plugins.clone().filter(|d| context.permits(d)) else {
        return vec![];
    };
    let installed = parse(context, &dir.join("installed_plugins.json"), SETTINGS_LIMIT)
        .ok()
        .flatten();
    let marketplaces = parse(
        context,
        &dir.join("known_marketplaces.json"),
        SETTINGS_LIMIT,
    )
    .ok()
    .flatten();
    // enabledPlugins merges key by key; the highest file mentioning an id wins.
    let mut enabled: BTreeMap<String, (bool, Layer)> = BTreeMap::new();
    for l in layers {
        for (id, on) in l
            .value
            .get("enabledPlugins")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if let Some(on) = on.as_bool() {
                enabled.insert(id.clone(), (on, l.layer));
            }
        }
    }
    let mut found = vec![];
    let records = installed
        .as_ref()
        .and_then(|v| v.get("plugins"))
        .and_then(Value::as_object);
    for (id, installs) in records.into_iter().flatten().take(MAX_PLUGINS) {
        let Some(install) = installs
            .as_array()
            .and_then(|a| a.first())
            .and_then(Value::as_object)
        else {
            continue;
        };
        let version = install
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let scope = install
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let root = install.get("installPath").and_then(Value::as_str).map(|p| {
            let p = PathBuf::from(p);
            if p.is_absolute() { p } else { dir.join(p) }
        });
        let marketplace = id.rsplit_once('@').map_or("", |(_, m)| m);
        let source = marketplaces
            .as_ref()
            .and_then(|m| m.get(marketplace))
            .and_then(|m| m.get("source"))
            .map(|s| {
                match (
                    s.get("source").and_then(Value::as_str),
                    s.get("repo").and_then(Value::as_str),
                ) {
                    (Some("github"), Some(repo)) => format!("GitHub {}", safe_text(repo)),
                    (Some(kind), _) => format!("{} (location withheld)", safe_text(kind)),
                    _ => "unknown".into(),
                }
            })
            .unwrap_or_else(|| "unknown".into());
        let state = enabled.get(id).copied();
        let mut lines = vec![
            format!(
                "Status: {}",
                match state {
                    Some((true, layer)) => format!("Enabled in {} settings", layer.label()),
                    Some((false, layer)) => format!("Disabled in {} settings", layer.label()),
                    None => "Installed, not enabled in any settings file".into(),
                }
            ),
            format!("Version: {}", safe_text(version)),
            format!("Installed for: {}", safe_text(scope)),
            format!("Marketplace: {} · {source}", safe_text(marketplace)),
        ];
        let mut parts = vec![format!("v{}", safe_text(version))];
        let mut hooks = vec![];
        let mut mcp = Map::new();
        match root.as_ref().filter(|r| context.permits(r)) {
            Some(root) => {
                lines.push(format!("Folder: {}", short(context, root)));
                let skills: Vec<_> = dir_names(context, &root.join("skills"))
                    .into_iter()
                    .filter(|n| !n.starts_with('.'))
                    .map(|n| safe_text(&n))
                    .collect();
                let markdown = |sub: &str| {
                    dir_names(context, &root.join(sub))
                        .iter()
                        .filter(|n| n.ends_with(".md"))
                        .count()
                };
                let (commands, agents) = (markdown("commands"), markdown("agents"));
                if let Ok(Some(value)) =
                    parse(context, &root.join("hooks/hooks.json"), SETTINGS_LIMIT)
                    && let Some(h) = value.get("hooks")
                {
                    hooks = handlers(h);
                }
                if let Ok(Some(value)) = parse(context, &root.join(".mcp.json"), SETTINGS_LIMIT) {
                    // A plugin .mcp.json may list servers at the top level or under mcpServers.
                    let map = value.get("mcpServers").unwrap_or(&value);
                    if let Some(map) = map.as_object() {
                        mcp = map.clone();
                    }
                }
                for (n, label) in [
                    (skills.len(), "skill"),
                    (commands, "command"),
                    (agents, "agent"),
                    (hooks.len(), "hook"),
                ] {
                    if n > 0 {
                        parts.push(count(n, label));
                    }
                }
                if !mcp.is_empty() {
                    parts.push(format!("{} MCP", mcp.len()));
                }
                lines.push(String::new());
                lines.push(format!(
                    "Skills: {}",
                    if skills.is_empty() {
                        "none".into()
                    } else {
                        skills.join(", ")
                    }
                ));
                lines.push(format!("Commands: {commands} · Agents: {agents}"));
                lines.push(format!(
                    "Hooks: {}{}",
                    hooks.len(),
                    if hooks.is_empty() {
                        ""
                    } else {
                        " (see the Hooks section)"
                    }
                ));
                lines.push(format!(
                    "MCP servers: {}",
                    if mcp.is_empty() {
                        "none".into()
                    } else {
                        mcp.keys()
                            .map(|k| safe_text(k))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ));
            }
            None => lines.push("Folder: not recorded or outside the fixture".into()),
        }
        let name = safe_text(id);
        found.push(Plugin {
            item: Item {
                summary: parts.join(" · "),
                detail: format!(
                    "{name}\n\n{}\n\nPlugin skills and hooks apply only while the plugin is enabled. Den reads plugin files as data.",
                    lines.join("\n")
                ),
                key: format!("plugin:{name}"),
                source: match state {
                    Some((true, _)) => "Enabled",
                    Some((false, _)) => "Disabled",
                    None => "Not enabled",
                }
                .into(),
                name,
                warning: false,
                endpoint: None,
            },
            enabled: state.map(|(on, _)| on),
            hooks,
            mcp,
        });
    }
    for (id, (on, layer)) in &enabled {
        if records.is_some_and(|r| r.contains_key(id)) {
            continue;
        }
        let name = safe_text(id);
        found.push(Plugin {
            item: Item {
                summary: "Not installed".into(),
                detail: format!(
                    "{name}\n\n{} in {} settings, but not recorded in installed_plugins.json.",
                    if *on { "Enabled" } else { "Disabled" },
                    layer.label()
                ),
                key: format!("plugin:{name}"),
                source: if *on { "Enabled" } else { "Disabled" }.into(),
                name,
                warning: true,
                endpoint: None,
            },
            enabled: Some(false),
            hooks: vec![],
            mcp: Map::new(),
        });
    }
    found
}
