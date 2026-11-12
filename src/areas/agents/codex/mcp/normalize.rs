use super::*;
use crate::areas::agents::codex::Context;
use std::{
    collections::BTreeMap,
    io,
    net::{IpAddr, Ipv6Addr},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use toml::Value;

const FIELDS: &[&str] = &[
    "command",
    "args",
    "cwd",
    "env",
    "env_vars",
    "experimental_environment",
    "url",
    "auth",
    "bearer_token_env_var",
    "http_headers",
    "env_http_headers",
    "http_headers_helper",
    "oauth",
    "oauth_resource",
    "scopes",
    "enabled",
    "required",
    "enabled_tools",
    "disabled_tools",
    "default_tools_approval_mode",
    "tools",
    "startup_timeout_sec",
    "startup_timeout_ms",
    "tool_timeout_sec",
];
const PLUGIN_FIELDS: &[&str] = &[
    "enabled",
    "enabled_tools",
    "disabled_tools",
    "default_tools_approval_mode",
    "tools",
];
fn label(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    if !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-@".contains(&b))
        && !["sk-", "ghp_", "github_pat_", "bearer", "-----"]
            .iter()
            .any(|p| lower.starts_with(p))
    {
        s.into()
    } else {
        "[withheld name]".into()
    }
}
fn error(d: &mut Definition, message: impl Into<String>) {
    d.finding(Severity::Error, message);
}
fn optional_bool(t: &toml::Table, key: &str, d: &mut Definition) -> Option<bool> {
    let v = t.get(key)?;
    if let Some(v) = v.as_bool() {
        Some(v)
    } else {
        error(d, format!("{key}: expected boolean; value withheld"));
        None
    }
}
fn string<'a>(t: &'a toml::Table, key: &str, d: &mut Definition) -> Option<&'a str> {
    let v = t.get(key)?;
    if let Some(s) = v
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= 8192 && !s.chars().any(char::is_control))
    {
        Some(s)
    } else {
        error(
            d,
            format!("{key}: expected bounded nonempty text without controls; value withheld"),
        );
        None
    }
}
fn array_names(t: &toml::Table, key: &str, d: &mut Definition) -> Option<Vec<String>> {
    let v = t.get(key)?;
    let Some(values) = v.as_array().filter(|a| {
        a.len() <= MAX_ITEMS && a.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty()))
    }) else {
        error(
            d,
            format!("{key}: invalid list or item limit exceeded; values withheld"),
        );
        return Some(vec!["[unresolved list]".into()]);
    };
    Some(values.iter().filter_map(Value::as_str).map(label).collect())
}
fn approval(v: &Value, d: &mut Definition) -> Option<Approval> {
    match v.as_str() {
        Some("auto") => Some(Approval::Auto),
        Some("prompt") => Some(Approval::Prompt),
        Some("writes") => Some(Approval::Writes),
        Some("approve") => Some(Approval::Approve),
        _ => {
            error(
                d,
                "Unsupported approval mode; value withheld, semantics unknown",
            );
            None
        }
    }
}
fn seconds(t: &toml::Table, key: &str, d: &mut Definition) -> Option<f64> {
    let v = t.get(key)?;
    let n = v.as_float().or_else(|| v.as_integer().map(|n| n as f64));
    if let Some(n) = n.filter(|n| n.is_finite() && *n >= 0.0 && *n <= u32::MAX as f64) {
        Some(n)
    } else {
        error(d, format!("{key}: unsupported timeout value; withheld"));
        None
    }
}
fn table_count(t: &toml::Table, key: &str, d: &mut Definition) -> Option<usize> {
    let v = t.get(key)?;
    match v.as_table() {
        Some(t) if t.len() <= MAX_ITEMS && t.values().all(Value::is_str) => Some(t.len()),
        _ => {
            error(
                d,
                format!("{key}: expected bounded string map; values withheld"),
            );
            None
        }
    }
}
fn variable(s: &str) -> String {
    if !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .enumerate()
            .all(|(n, b)| b.is_ascii_uppercase() || b == b'_' || n > 0 && b.is_ascii_digit())
    {
        s.into()
    } else {
        "[withheld variable name]".into()
    }
}

/// Scheme, host class and port of an http(s) URL; the host itself is never kept.
pub(crate) fn endpoint(raw: &str) -> Option<Endpoint> {
    if raw.len() > 8192
        || raw.chars().any(|c| c.is_control() || c.is_whitespace())
        || raw.contains('\\')
    {
        return None;
    }
    if !raw
        .get(..8)
        .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
        && !raw
            .get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("http://"))
    {
        return None;
    }
    let url = url::Url::parse(raw).ok()?;
    let kind = match url.host()? {
        url::Host::Domain(name)
            if name.eq_ignore_ascii_case("localhost")
                || name.trim_end_matches('.').eq_ignore_ascii_case("localhost") =>
        {
            EndpointKind::Loopback
        }
        url::Host::Domain(_) => EndpointKind::RemoteHostname,
        url::Host::Ipv4(ip) => classify(IpAddr::V4(ip)),
        url::Host::Ipv6(ip) => classify(IpAddr::V6(ip)),
    };
    // Even host labels can embed credentials. Store only scheme/class/port, never
    // a raw URL or hostname. URL parsing errors are also discarded, not formatted.
    Some(Endpoint {
        identity: format!(
            "{}://<{:?}>:{} (host, userinfo, path, query and fragment withheld)",
            url.scheme(),
            kind,
            url.port_or_known_default()?
        ),
        kind,
        plaintext: url.scheme() == "http",
    })
}
fn classify(ip: IpAddr) -> EndpointKind {
    match ip {
        IpAddr::V4(ip) if ip.is_loopback() => EndpointKind::Loopback,
        IpAddr::V4(ip) if ip.is_private() || ip.is_link_local() => EndpointKind::PrivateIp,
        IpAddr::V6(ip) if ip.to_ipv4_mapped().is_some() => classify(IpAddr::V4(
            ip.to_ipv4_mapped()
                .unwrap_or(std::net::Ipv4Addr::UNSPECIFIED),
        )),
        IpAddr::V6(ip) if ip == Ipv6Addr::LOCALHOST => EndpointKind::Loopback,
        IpAddr::V6(ip) if ip.is_unique_local() || ip.is_unicast_link_local() => {
            EndpointKind::PrivateIp
        }
        _ => EndpointKind::OtherIp,
    }
}
fn inspect_path(
    context: &Context,
    path: &str,
    directory: bool,
) -> (&'static str, Option<Severity>) {
    let path = Path::new(path);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return (
            "Relative/PATH origin unresolved; no expansion or search performed",
            Some(Severity::Unknown),
        );
    }
    if !context.permits(path) {
        return (
            "Outside fixture boundary; not inspected",
            Some(Severity::Unknown),
        );
    }
    // Bounded symlink resolution, no file contents or installed binaries opened.
    let resolved = match crate::areas::agents::skills::filesystem::resolve(path) {
        Ok(p) => p,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return (
                "Absolute path missing (local observation)",
                Some(Severity::Error),
            );
        }
        Err(_) => {
            return (
                "Path inaccessible or symlink resolution limit",
                Some(Severity::Unknown),
            );
        }
    };
    if !context.permits(&resolved) {
        return (
            "Resolved target outside fixture boundary; not inspected",
            Some(Severity::Unknown),
        );
    }
    match std::fs::symlink_metadata(&resolved) {
        Ok(m) if directory && m.is_dir() => {
            #[cfg(unix)]
            if crate::areas::agents::skills::filesystem::Directory::open(&resolved).is_err() {
                return ("Working directory inaccessible", Some(Severity::Error));
            }
            ("Absolute directory accessible; path withheld", None)
        }
        Ok(m) if !directory && m.is_file() => (
            "Absolute file exists; executable provenance/permissions not assessed; path withheld",
            None,
        ),
        Ok(_) => (
            "Unexpected filesystem structure (not a directory/regular launcher)",
            Some(Severity::Error),
        ),
        Err(_) => ("Path metadata unavailable", Some(Severity::Unknown)),
    }
}
fn normalize(
    context: &Context,
    source: usize,
    layer: &Layer,
    value: &Value,
    plugin: bool,
) -> Definition {
    let mut d = Definition {
        source,
        activity: layer.activity,
        scope: layer.scope,
        transport: Transport::Unknown,
        enabled: None,
        required: None,
        environment: "local (default)".into(),
        launcher: "Not configured".into(),
        args: if plugin { None } else { Some(0) },
        cwd: "Not explicitly set".into(),
        endpoint: None,
        inline_env: if plugin { None } else { Some(0) },
        forwarded: vec![],
        auth: vec![],
        auth_details: vec![],
        tools: Tools::default(),
        startup_seconds: None,
        tool_seconds: None,
        findings: vec![],
        transport_present: false,
        present: std::collections::BTreeSet::new(),
        plugin_override: plugin,
    };
    let Some(t) = value.as_table() else {
        error(&mut d, "MCP definition must be a table; value withheld");
        return d;
    };
    let unsupported_plugin = if plugin {
        t.keys()
            .filter(|k| !PLUGIN_FIELDS.contains(&k.as_str()))
            .count()
    } else {
        0
    };
    let filtered;
    let t = if plugin {
        filtered = t
            .iter()
            .filter(|(k, _)| PLUGIN_FIELDS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<toml::Table>();
        &filtered
    } else {
        t
    };
    if unsupported_plugin > 0 {
        d.finding(Severity::Unknown,format!("{unsupported_plugin} unsupported plugin override fields withheld; transport definitions are not accepted from overrides"));
    }
    d.present = t
        .keys()
        .filter(|k| FIELDS.contains(&k.as_str()))
        .cloned()
        .collect();
    if t.contains_key("startup_timeout_sec") || t.contains_key("startup_timeout_ms") {
        d.present.insert("startup_timeout".into());
    }
    if plugin {
        d.environment = "Plugin execution context unavailable".into();
    }
    d.enabled = optional_bool(t, "enabled", &mut d);
    d.required = optional_bool(t, "required", &mut d);
    if t.contains_key("experimental_environment") {
        d.environment = "unresolved".into();
    }
    if let Some(v) = string(t, "experimental_environment", &mut d) {
        match v {
            "local" | "remote" => d.environment = v.into(),
            _ => {
                d.environment = "unresolved".into();
                error(&mut d, "Unsupported execution environment; value withheld");
            }
        }
    }
    let command = string(t, "command", &mut d);
    let url = string(t, "url", &mut d);
    d.transport_present = t.contains_key("command") || t.contains_key("url");
    d.transport = match (t.contains_key("command"), t.contains_key("url")) {
        (true, false) => Transport::Stdio,
        (false, true) => Transport::Http,
        (true, true) => {
            error(&mut d, "Ambiguous transport: command and URL both defined");
            Transport::Ambiguous
        }
        _ => Transport::Unknown,
    };
    if let Some(command) = command {
        let name = Path::new(command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        d.launcher = if [
            "npx", "uvx", "bunx", "node", "python", "python3", "uv", "docker", "bun", "deno", "go",
            "ruby", "java", "dotnet",
        ]
        .contains(&name)
        {
            format!("{name} (configured; never executed)")
        } else {
            "Configured launcher; text withheld".into()
        };
        if !matches!(d.environment.as_str(), "local" | "local (default)") {
            d.finding(
                Severity::Unknown,
                "Remote launcher origin not inspected on local filesystem",
            );
        } else {
            let (note, severity) = inspect_path(context, command, false);
            d.launcher.push_str(&format!("\n{note}"));
            if let Some(severity) = severity {
                d.finding(severity, format!("Launcher: {note}"));
            }
        }
    }
    if let Some(args) = t.get("args") {
        d.args = None;
        match args.as_array() {
            Some(a) if a.len() <= MAX_ITEMS && a.iter().all(Value::is_str) => {
                d.args = Some(a.len())
            }
            _ => error(
                &mut d,
                "args: invalid string array or item limit; all argument values withheld",
            ),
        }
    }
    if let Some(cwd) = string(t, "cwd", &mut d) {
        if !matches!(d.environment.as_str(), "local" | "local (default)") {
            d.cwd = "Remote working directory configured; local existence not checked".into();
        } else {
            let absolute = if !Path::new(cwd).is_absolute() && layer.scope == Scope::Project {
                layer.path.parent().map(|p| p.join(cwd))
            } else {
                None
            };
            let path = absolute.as_ref().and_then(|p| p.to_str()).unwrap_or(cwd);
            let (note, severity) = inspect_path(context, path, true);
            d.cwd = note.into();
            if let Some(s) = severity {
                d.finding(s, format!("Working directory: {note}"));
            }
        }
    }
    if t.contains_key("env") {
        d.inline_env = None;
    }
    if let Some(n) = table_count(t, "env", &mut d) {
        d.inline_env = Some(n);
        if n > 0 {
            d.finding(
                Severity::Review,
                "Inline environment values present; may contain credentials; all values withheld",
            );
        }
    }
    if let Some(v) = t.get("env_vars") {
        if let Some(a) = v.as_array().filter(|a| a.len() <= MAX_ITEMS) {
            for e in a {
                let pair = if let Some(name) = e.as_str() {
                    Some((name, "local"))
                } else if let Some(t) = e.as_table() {
                    if t.keys().any(|k| !matches!(k.as_str(), "name" | "source")) {
                        d.finding(
                            Severity::Unknown,
                            "Unknown environment-forwarding entry fields withheld",
                        );
                    }
                    t.get("name")
                        .and_then(Value::as_str)
                        .zip(t.get("source").and_then(Value::as_str))
                } else {
                    None
                };
                if let Some((name, src)) =
                    pair.filter(|(name, src)| !name.is_empty() && ["local", "remote"].contains(src))
                {
                    d.forwarded.push(Forwarded {
                        name: variable(name),
                        source: src.into(),
                    });
                    if src == "remote" && d.environment != "remote" {
                        error(
                            &mut d,
                            "Remote environment forwarding requires remote stdio execution",
                        );
                    }
                } else {
                    error(
                        &mut d,
                        "Invalid environment-forwarding entry; value withheld",
                    );
                }
            }
        } else {
            error(
                &mut d,
                "env_vars: invalid or oversized list; values withheld",
            );
        }
    }
    if let Some(raw) = url {
        d.endpoint = endpoint(raw);
        if d.endpoint.is_none() {
            error(&mut d, "Invalid HTTP endpoint syntax; URL withheld");
        } else if d
            .endpoint
            .as_ref()
            .is_some_and(|e| e.plaintext && e.kind != EndpointKind::Loopback)
        {
            d.finding(
                Severity::Review,
                "Non-loopback HTTP endpoint uses plaintext transport; no network probe performed",
            );
        }
    }
    if d.transport == Transport::Http && d.environment == "remote" {
        error(
            &mut d,
            "Remote placement for Streamable HTTP is not supported by documented schema",
        );
    }
    authentication(t, &mut d);
    d.tools.allow = array_names(t, "enabled_tools", &mut d);
    d.tools.deny = array_names(t, "disabled_tools", &mut d);
    if let Some(v) = t.get("default_tools_approval_mode") {
        d.tools.approval = approval(v, &mut d);
    }
    if let Some(v) = t.get("tools") {
        if let Some(tools) = v.as_table().filter(|t| t.len() <= MAX_ITEMS) {
            for (name, v) in tools {
                let Some(t) = v.as_table() else {
                    error(&mut d, "Per-tool policy must be a table; value withheld");
                    continue;
                };
                let policy = t.get("approval_mode").and_then(|v| approval(v, &mut d));
                let output = t
                    .get("output_token_limit")
                    .and_then(|v| v.as_integer())
                    .filter(|n| *n > 0)
                    .map(|n| n as u64);
                if t.contains_key("output_token_limit") && output.is_none() {
                    error(&mut d, "Tool output_token_limit must be a positive integer");
                }
                if t.keys()
                    .any(|k| !matches!(k.as_str(), "approval_mode" | "output_token_limit"))
                {
                    d.finding(Severity::Unknown, "Unknown per-tool fields withheld");
                }
                if plugin && t.contains_key("output_token_limit") {
                    d.finding(Severity::Unknown,"Plugin per-tool output_token_limit override is not established by current documentation");
                }
                d.tools.overrides.push(ToolPolicy {
                    name: label(name),
                    approval: policy,
                    output_token_limit: output,
                    approval_present: t.contains_key("approval_mode"),
                    limit_present: t.contains_key("output_token_limit"),
                });
            }
        } else {
            error(&mut d, "Invalid or oversized per-tool policy table");
        }
    }
    let secs = seconds(t, "startup_timeout_sec", &mut d);
    let millis = seconds(t, "startup_timeout_ms", &mut d);
    d.startup_seconds = secs.or_else(|| millis.map(|n| n / 1000.0));
    if secs.is_some() && millis.is_some() {
        d.startup_seconds = None;
        d.finding(
            Severity::Unknown,
            "Both startup timeout aliases present; selected timeout unresolved",
        );
    }
    d.tool_seconds = seconds(t, "tool_timeout_sec", &mut d);
    let unknown = t
        .keys()
        .filter(|k| !FIELDS.contains(&k.as_str()) || plugin && !PLUGIN_FIELDS.contains(&k.as_str()))
        .count();
    if unknown > 0 {
        d.finding(Severity::Unknown,format!("{unknown} unsupported fields or plugin overrides withheld; application not established"));
    }
    if plugin {
        d.transport = Transport::Unknown;
        d.endpoint = None;
        d.launcher = "Plugin definition unavailable; override is not a launcher definition".into();
        d.finding(Severity::Unknown,"Plugin override observed; authoritative installed-plugin mapping unavailable. Cache directories are not treated as installed definitions.");
    }
    if d.transport == Transport::Http
        && ["command", "args", "cwd", "env", "env_vars"]
            .iter()
            .any(|k| t.contains_key(*k))
    {
        error(
            &mut d,
            "Stdio-specific fields are present on an HTTP definition",
        );
    }
    d
}
fn authentication(t: &toml::Table, d: &mut Definition) {
    let errors_before = d
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    if let Some(auth) = string(t, "auth", d) {
        match auth {
            "oauth" => d.auth.push(Auth::OAuth),
            "chatgpt" => d.auth.push(Auth::ChatGpt),
            _ => error(d, "Unsupported auth fallback; value withheld"),
        }
    }
    if let Some(name) = string(t, "bearer_token_env_var", d) {
        d.auth.push(Auth::BearerEnvironment);
        d.auth_details.push(format!(
            "Bearer variable: {} (value never read)",
            variable(name)
        ));
    }
    if let Some(n) = table_count(t, "http_headers", d) {
        if n > 0 {
            d.auth.push(Auth::StaticHeaders);
            d.finding(
                Severity::Review,
                "Static HTTP header values present; all values withheld",
            );
        }
        if t.get("http_headers")
            .and_then(Value::as_table)
            .is_some_and(|t| t.keys().any(|k| k.eq_ignore_ascii_case("authorization")))
        {
            d.auth.push(Auth::StaticAuthorization);
        }
        d.auth_details
            .push(format!("{n} static headers; names and values withheld"));
    }
    if let Some(n) = table_count(t, "env_http_headers", d) {
        if n > 0 {
            d.auth.push(Auth::EnvironmentHeaders);
        }
        d.auth_details.push(format!(
            "{n} environment-sourced headers; header names/values withheld"
        ));
        if let Some(t) = t.get("env_http_headers").and_then(Value::as_table) {
            for v in t.values().filter_map(Value::as_str) {
                d.auth_details.push(format!(
                    "Header variable: {} (value never read)",
                    variable(v)
                ));
            }
        }
    }
    if string(t, "http_headers_helper", d).is_some() {
        d.auth.push(Auth::HeaderHelper);
        d.auth_details.push("Credential helper configured; command and output withheld; supported context: local HTTP".into());
        if d.transport == Transport::Stdio || d.environment == "remote" {
            error(
                d,
                "http_headers_helper requires local HTTP; helper is not executed",
            );
        }
    }
    if let Some(v) = t.get("oauth") {
        d.auth.push(Auth::OAuth);
        if let Some(o) = v.as_table() {
            let client = string(o, "client_id", d);
            let secret = string(o, "client_secret", d);
            d.auth_details.push(format!(
                "OAuth client ID present: {}; client secret present: {} (values withheld)",
                o.contains_key("client_id"),
                o.contains_key("client_secret")
            ));
            if o.contains_key("client_secret") {
                d.finding(
                    Severity::Review,
                    "Inline OAuth client secret configuration present; value withheld",
                );
                if client.is_none() || secret.is_none() {
                    error(
                        d,
                        "OAuth client secret requires a nonempty client ID and secret",
                    );
                }
            }
            if let Some(raw) = string(o, "callback_url", d) {
                if let Some(e) = endpoint(raw) {
                    d.auth_details
                        .push(format!("OAuth callback: {}", e.identity));
                } else {
                    error(d, "Invalid OAuth callback URL; value withheld");
                }
            }
            if let Some(v) = o.get("callback_port") {
                if let Some(n) = v.as_integer().filter(|n| *n > 0 && *n <= 65535) {
                    d.auth_details.push(format!("OAuth callback port: {n}"));
                } else {
                    error(d, "Invalid OAuth callback port");
                }
            }
            if o.keys().any(|k| {
                ![
                    "client_id",
                    "client_secret",
                    "callback_url",
                    "callback_port",
                ]
                .contains(&k.as_str())
            }) {
                d.finding(Severity::Unknown, "Unknown OAuth fields withheld");
            }
        } else {
            error(d, "oauth must be a table; value withheld");
        }
    }
    if string(t, "oauth_resource", d).is_some() {
        d.auth.push(Auth::OAuth);
        d.auth_details
            .push("OAuth resource configured; value withheld".into());
    }
    if let Some(scopes) = array_names(t, "scopes", d) {
        d.auth.push(Auth::OAuth);
        d.auth_details.push(format!("OAuth scopes: {} entries configured; values withheld (empty list does not imply no scopes)",scopes.len()));
    }
    if d.transport == Transport::Stdio
        && [
            "auth",
            "bearer_token_env_var",
            "http_headers",
            "env_http_headers",
            "http_headers_helper",
            "oauth",
            "oauth_resource",
            "scopes",
        ]
        .iter()
        .any(|k| t.contains_key(*k))
    {
        error(d, "HTTP authentication fields on a stdio definition");
    }
    if d.transport == Transport::Stdio {
        d.auth_details.push("Server-specific stdio authentication may use environment values or other mechanisms; not established by HTTP auth fields.".into());
    }
    if d.findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count()
        > errors_before
    {
        d.auth.push(Auth::Undetermined);
    }
    d.auth.sort();
    d.auth.dedup();
    if d.auth.is_empty() {
        d.auth.push(Auth::NoneIdentified);
    }
    if d.transport == Transport::Http && !t.contains_key("auth") {
        d.auth_details.push("No explicit auth fallback; documented default is stored OAuth if available. Credential availability was not checked.".into());
    }
}

pub(crate) fn collect(
    context: &Context,
    raw: &[(Layer, Option<Value>)],
    incomplete: bool,
    cancel: &AtomicBool,
) -> io::Result<Inventory> {
    let mut result = Inventory {
        partial: incomplete,
        ..Inventory::default()
    };
    let mut servers: BTreeMap<Identity, Server> = BTreeMap::new();
    let mut count = 0;
    let mut active_gap = incomplete;
    for (source, (layer, value)) in raw.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("MCP normalization cancelled"));
        }
        let Some(value) = value else {
            continue;
        };
        let mut groups = vec![];
        if let Some(v) = value.get("mcp_servers") {
            groups.push((None, v));
        }
        if let Some(v) = value.get("plugins")
            && !v.is_table()
        {
            result.partial = true;
            active_gap |= layer.activity != Activity::Inactive;
            result.findings.push(Finding {
                severity: Severity::Error,
                message: format!(
                    "Source {}: invalid plugin table; value withheld",
                    source + 1
                ),
            });
        }
        if let Some(p) = value.get("plugins").and_then(Value::as_table) {
            if p.len() > MAX_DEFINITIONS {
                result.partial = true;
                active_gap |= layer.activity != Activity::Inactive;
                result.findings.push(Finding {
                    severity: Severity::Unknown,
                    message: "Plugin override container limit reached".into(),
                });
            }
            for (plugin, v) in p.iter().take(MAX_DEFINITIONS) {
                if let Some(v) = v.get("mcp_servers") {
                    groups.push((Some(plugin.as_str()), v));
                }
            }
        }
        for (plugin, v) in groups {
            let Some(table) = v.as_table() else {
                result.partial = true;
                active_gap |= layer.activity != Activity::Inactive;
                result.findings.push(Finding {
                    severity: Severity::Error,
                    message: format!(
                        "Source {}: MCP server collection is not a table; value withheld",
                        source + 1
                    ),
                });
                continue;
            };
            for (name, v) in table {
                if cancel.load(Ordering::Relaxed) {
                    return Err(io::Error::other("MCP normalization cancelled"));
                }
                if count >= MAX_DEFINITIONS {
                    result.partial = true;
                    active_gap |= layer.activity != Activity::Inactive;
                    break;
                }
                count += 1;
                let id = Identity(plugin.map(str::to_owned), name.clone());
                let server = servers.entry(id.clone()).or_insert_with(|| Server {
                    id,
                    name: label(name),
                    plugin: plugin.map(label),
                    definitions: vec![],
                    status: Status::Configured,
                    selected: None,
                    findings: vec![],
                });
                server
                    .definitions
                    .push(normalize(context, source, layer, v, plugin.is_some()));
            }
        }
    }
    for mut s in servers.into_values() {
        let applicable: Vec<_> = s
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, d)| d.activity != Activity::Inactive)
            .map(|(n, _)| n)
            .collect();
        if s.plugin.is_some() {
            s.status = Status::PluginUnresolved;
        } else if applicable.is_empty() {
            s.status = Status::Configured;
            s.findings.push(Finding {
                severity: Severity::Info,
                message:
                    "Project definitions are inactive because the selected project is untrusted"
                        .into(),
            });
        } else if applicable.len() > 1
            || applicable
                .iter()
                .any(|&n| s.definitions[n].activity == Activity::Conditional)
            || active_gap
        {
            s.status = Status::Conditional;
            s.findings.push(Finding{severity:Severity::Unknown,message:"Trust, incomplete layers or cross-layer MCP merge semantics prevent selecting a complete definition. All source definitions retained in layer order; no guessed winner or merged transport.".into()});
        } else {
            let n = applicable[0];
            let d = &mut s.definitions[n];
            if !d.transport_present {
                error(
                    d,
                    "Missing transport: configure command for stdio or url for HTTP",
                );
            }
            s.selected = Some(n);
            s.status = if d.invalid() {
                Status::Invalid
            } else if d.enabled == Some(false) {
                Status::Disabled
            } else if d.findings.iter().any(|f| {
                f.severity == Severity::Unknown
                    && !f.message.starts_with("Launcher:")
                    && !f.message.starts_with("Remote launcher")
            }) {
                Status::Conditional
            } else {
                Status::Eligible
            };
        }
        result.servers.push(s);
    }
    if result.partial {
        result.findings.push(Finding {
            severity: Severity::Unknown,
            message: "MCP inventory is partial because sources, shapes or collection limits are incomplete".into(),
        });
    }
    if incomplete {
        result.findings.push(Finding{severity:Severity::Unknown,message:"Configuration sources incomplete; an empty MCP list is not proof of no configured servers".into()});
    }
    Ok(result)
}
