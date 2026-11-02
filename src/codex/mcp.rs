//! Read-only MCP configuration evidence, normalized before raw TOML is discarded.
//! No MCP client, command runner, environment lookup or credential access exists here.
mod normalize;
pub mod state;

use super::config::{Activity, Layer, Scope};
pub(crate) use normalize::collect;
use std::{collections::BTreeMap, fmt};

pub const LIMITATIONS: &str = "Configuration only. Runtime, connection, authentication and advertised tools: not checked. CLI overrides, managed policy and session state are unobserved. Tool names are configured restrictions, not discovered tools. Unknown values, arguments and credentials are withheld. Plugin installation/activation is not established from overrides or cache directories.";
pub const MAX_DEFINITIONS: usize = 1024;
pub const MAX_ITEMS: usize = 256;

/// Exact identities are retained only for matching; never formatted as diagnostics.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Identity(Option<String>, String);
impl Identity {
    pub(crate) fn selection_key(&self) -> String {
        format!(
            "mcp:{}:{}:{}:{}",
            self.0.is_some(),
            self.0.as_ref().map_or(0, |p| p.len()),
            self.0.as_deref().unwrap_or(""),
            self.1
        )
    }
}
impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Identity([withheld])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Transport {
    Stdio,
    Http,
    Unknown,
    Ambiguous,
}
impl Transport {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stdio => "Stdio",
            Self::Http => "HTTP",
            Self::Unknown => "Unknown",
            Self::Ambiguous => "Ambiguous",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Configured,
    Disabled,
    Eligible,
    Conditional,
    Invalid,
    PluginUnresolved,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Configured => "Configured (inactive)",
            Self::Disabled => "Explicitly disabled",
            Self::Eligible => "Locally eligible",
            Self::Conditional => "Conditional",
            Self::Invalid => "Invalid configuration",
            Self::PluginUnresolved => "Unresolved plugin",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Review,
    Info,
    Unknown,
}
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Auth {
    OAuth,
    ChatGpt,
    BearerEnvironment,
    StaticHeaders,
    StaticAuthorization,
    EnvironmentHeaders,
    HeaderHelper,
    NoneIdentified,
    Undetermined,
}
impl Auth {
    pub fn label(self) -> &'static str {
        match self {
            Self::OAuth => "OAuth configuration",
            Self::ChatGpt => "ChatGPT session fallback",
            Self::BearerEnvironment => "Bearer environment reference",
            Self::StaticHeaders => "Static headers",
            Self::StaticAuthorization => "Static Authorization",
            Self::EnvironmentHeaders => "Environment headers",
            Self::HeaderHelper => "Credential helper",
            Self::NoneIdentified => "No explicit auth source",
            Self::Undetermined => "Auth configuration unresolved",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointKind {
    Loopback,
    PrivateIp,
    OtherIp,
    RemoteHostname,
}
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub identity: String,
    pub kind: EndpointKind,
    pub plaintext: bool,
}
#[derive(Debug, Clone)]
pub struct Forwarded {
    pub name: String,
    pub source: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Auto,
    Prompt,
    Writes,
    Approve,
}
impl Approval {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Prompt => "prompt",
            Self::Writes => "writes",
            Self::Approve => "approve",
        }
    }
}
#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub name: String,
    pub approval: Option<Approval>,
    pub output_token_limit: Option<u64>,
    pub approval_present: bool,
    pub limit_present: bool,
}
#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub allow: Option<Vec<String>>,
    pub deny: Option<Vec<String>>,
    pub approval: Option<Approval>,
    pub overrides: Vec<ToolPolicy>,
}
#[derive(Clone)]
pub struct Definition {
    pub source: usize,
    pub activity: Activity,
    pub scope: Scope,
    pub transport: Transport,
    pub enabled: Option<bool>,
    pub required: Option<bool>,
    pub environment: String,
    pub launcher: String,
    pub args: Option<usize>,
    pub cwd: String,
    pub endpoint: Option<Endpoint>,
    pub inline_env: Option<usize>,
    pub forwarded: Vec<Forwarded>,
    pub auth: Vec<Auth>,
    pub auth_details: Vec<String>,
    pub tools: Tools,
    pub startup_seconds: Option<f64>,
    pub tool_seconds: Option<f64>,
    pub findings: Vec<Finding>,
    pub transport_present: bool,
    pub present: std::collections::BTreeSet<String>,
    pub plugin_override: bool,
}
impl Definition {
    pub(crate) fn finding(&mut self, severity: Severity, message: impl Into<String>) {
        self.findings.push(Finding {
            severity,
            message: message.into(),
        });
    }
    pub fn invalid(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }
    fn value_or(&self, key: &str, value: Option<String>, default: &str) -> String {
        value.unwrap_or_else(|| {
            if self.present.contains(key) || self.plugin_override {
                "Unresolved (not a default)".into()
            } else {
                default.into()
            }
        })
    }
    pub fn authentication(&self) -> String {
        self.auth
            .iter()
            .map(|a| a.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}
#[derive(Clone)]
pub struct Server {
    pub id: Identity,
    pub name: String,
    pub plugin: Option<String>,
    pub definitions: Vec<Definition>,
    pub status: Status,
    pub selected: Option<usize>,
    pub findings: Vec<Finding>,
}
impl Server {
    pub fn transport(&self) -> Transport {
        let active: Vec<_> = self
            .definitions
            .iter()
            .filter(|d| d.activity != Activity::Inactive)
            .collect();
        let defs = if active.is_empty() {
            self.definitions.iter().collect()
        } else {
            active
        };
        let kinds: std::collections::BTreeSet<_> = defs.iter().map(|d| d.transport).collect();
        if kinds.len() == 1 {
            *kinds.first().unwrap_or(&Transport::Unknown)
        } else {
            Transport::Ambiguous
        }
    }
    pub fn validation(&self) -> &'static str {
        if self.definitions.iter().any(Definition::invalid) {
            "Errors"
        } else if self
            .definitions
            .iter()
            .flat_map(|d| &d.findings)
            .chain(&self.findings)
            .any(|f| matches!(f.severity, Severity::Review | Severity::Unknown))
        {
            "Review / unknown"
        } else {
            "No detected errors"
        }
    }
    pub fn summary(&self) -> String {
        let scopes = self
            .definitions
            .iter()
            .map(|d| format!("{:?}", d.scope))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join("/");
        let auth = self
            .definitions
            .iter()
            .flat_map(|d| &d.auth)
            .map(|a| a.label())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "{}{} · {} · {} · {} · {} · {}",
            self.plugin
                .as_ref()
                .map_or(String::new(), |p| format!("{p} / ")),
            self.name,
            self.transport().label(),
            self.status.label(),
            scopes,
            auth,
            self.validation()
        )
    }
    pub fn text(&self, section: usize, layers: &[Layer]) -> String {
        let mut text = format!(
            "{}\n{}\nConfigured: yes · {}\n\n",
            self.name,
            self.plugin
                .as_ref()
                .map_or("Standalone configuration".into(), |p| format!(
                    "Plugin override: {p}; actual definition unavailable"
                )),
            self.status.label()
        );
        if section == 0 {
            text.push_str(&format!("Transport: {}\nValidation: {}\nDefinitions: {}\nSelected complete definition: {}\n\n",self.transport().label(),self.validation(),self.definitions.len(),self.selected.map_or("Unresolved or inactive".into(),|i|format!("Source {}",i+1))));
        }
        for (n, d) in self.definitions.iter().enumerate() {
            let Some(layer) = layers.get(d.source) else {
                continue;
            };
            text.push_str(&format!(
                "Source {}: {:?} · {:?}\n{}\n",
                n + 1,
                d.scope,
                d.activity,
                super::privacy::path(&layer.path)
            ));
            if self.selected == Some(n) {
                text.push_str("Selected local definition\n");
            } else if d.activity != Activity::Inactive {
                text.push_str("Definition retained; cross-layer values not merged\n");
            }
            match section {
                0=>text.push_str(&format!("Enabled: {} · Required startup: {}\nExecution context: {}\nAuthentication: {}\n",d.value_or("enabled",d.enabled.map(|v|v.to_string()),"default enabled (modeled)"),d.value_or("required",d.required.map(|v|v.to_string()),"optional (documented default)"),d.environment,d.authentication())),
                1=>text.push_str(&format!("Transport: {}\nLauncher: {}\nArguments: {} configured; all values withheld\nWorking directory: {}\nEndpoint: {}\nExecution context: {}\nInline environment: {} entries; values withheld\nForwarded names (validated entries only): {}\nStartup timeout: {} seconds\nTool timeout: {} seconds\n",d.transport.label(),d.launcher,d.args.map_or("Unresolved".into(),|n|n.to_string()),d.cwd,d.endpoint.as_ref().map_or("Not established",|e|e.identity.as_str()),d.environment,d.inline_env.map_or("Unresolved".into(),|n|n.to_string()),d.forwarded.iter().map(|e|format!("{} ({})",e.name,e.source)).collect::<Vec<_>>().join(", "),d.value_or("startup_timeout",d.startup_seconds.map(|v|v.to_string()),"10 (default)"),d.value_or("tool_timeout_sec",d.tool_seconds.map(|v|v.to_string()),"60 (default)"))),
                2=>{text.push_str(&format!("{}\n{}\n",d.authentication(),d.auth_details.join("\n")));text.push_str("Configured bearer/authorization sources precede auth fallback. Explicit bearer tokens and OAuth credentials take precedence over helper Authorization. Availability and the final credential source are unknown. ChatGPT fallback is restricted to the trusted first-party origin by Codex. Helpers are local HTTP only; never run by Den.\n");},
                3=>{text.push_str(&format!("Allowlist first: {}\nDenylist second: {}\nDefault approval: {}\n",list(&d.tools.allow),list(&d.tools.deny),d.value_or("default_tools_approval_mode",d.tools.approval.map(|a|a.label().into()),"Not explicitly set")));for t in &d.tools.overrides{text.push_str(&format!("{}: approval {}; output token limit {}\n",t.name,t.approval.map_or(if t.approval_present { "Unresolved" } else { "inherits configured default" },|a|a.label()),t.output_token_limit.map_or(if t.limit_present { "Unresolved".into() } else { "not set".into() },|n|n.to_string())));}text.push_str("These are configured restrictions only. Advertised tools and tool metadata are not available. 'writes' depends on server read-only metadata; other unknown policy values are not interpreted.\n");},
                4=>text.push_str("Enter/e: inspect this server's source; v: next source; Backspace: return. Safe summaries only; raw TOML is never displayed.\n"),
                _=>for f in &d.findings{text.push_str(&format!("{:?}: {}\n",f.severity,f.message));},
            }
            text.push('\n');
        }
        for f in &self.findings {
            text.push_str(&format!("{:?}: {}\n", f.severity, f.message));
        }
        text.push_str(&format!("\n{LIMITATIONS}"));
        text
    }
}
fn list(v: &Option<Vec<String>>) -> String {
    v.as_ref().map_or("not configured".into(), |v| {
        if v.is_empty() {
            "[] (explicitly empty)".into()
        } else {
            v.join(", ")
        }
    })
}
#[derive(Clone, Default)]
pub struct Inventory {
    pub servers: Vec<Server>,
    pub findings: Vec<Finding>,
    pub partial: bool,
}
impl Inventory {
    pub fn definition_count(&self) -> usize {
        self.servers.iter().map(|s| s.definitions.len()).sum()
    }
    pub fn counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::from([
            ("Unresolved plugin references".into(), 0),
            ("Explicitly disabled definitions".into(), 0),
            ("Conditional definitions".into(), 0),
            ("Findings: Error".into(), 0),
            ("Findings: Review".into(), 0),
            ("Findings: Info".into(), 0),
            ("Findings: Unknown".into(), 0),
        ]);
        counts.insert("Servers".into(), self.servers.len());
        counts.insert("Definitions".into(), self.definition_count());
        for s in &self.servers {
            *counts
                .entry(format!("Server status: {}", s.status.label()))
                .or_default() += 1;
            for d in &s.definitions {
                if d.enabled == Some(false) {
                    *counts
                        .entry("Explicitly disabled definitions".into())
                        .or_default() += 1;
                }
                if d.activity == Activity::Conditional {
                    *counts.entry("Conditional definitions".into()).or_default() += 1;
                }
                *counts
                    .entry(format!("Definition transport: {}", d.transport.label()))
                    .or_default() += 1;
                *counts
                    .entry(format!("Definition scope: {:?}", d.scope))
                    .or_default() += 1;
                for a in &d.auth {
                    *counts
                        .entry(format!("Auth mechanism (definitions): {}", a.label()))
                        .or_default() += 1;
                }
                for f in &d.findings {
                    *counts
                        .entry(format!("Findings: {:?}", f.severity))
                        .or_default() += 1;
                }
            }
            for f in &s.findings {
                *counts
                    .entry(format!("Findings: {:?}", f.severity))
                    .or_default() += 1;
            }
            if s.plugin.is_some() {
                *counts
                    .entry("Unresolved plugin references".into())
                    .or_default() += 1;
            }
        }
        for f in &self.findings {
            *counts
                .entry(format!("Findings: {:?}", f.severity))
                .or_default() += 1;
        }
        counts
    }
}

// Debug output deliberately contains aggregate/classified facts only, even when
// configured server/tool identifiers themselves are private.
impl fmt::Debug for Definition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpDefinition")
            .field("source", &self.source)
            .field("transport", &self.transport)
            .field("findings", &self.findings.len())
            .finish()
    }
}
impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpServer")
            .field("definitions", &self.definitions.len())
            .field("status", &self.status)
            .finish()
    }
}
impl fmt::Debug for Inventory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpInventory")
            .field("counts", &self.counts())
            .field("partial", &self.partial)
            .finish()
    }
}
