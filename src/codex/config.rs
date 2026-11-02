//! Parsing is transient; returned objects contain only classified display values.
use super::{Context, LIMITATIONS, privacy, read};
use crate::skills::{self, filesystem};
use std::{
    collections::BTreeSet,
    io,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use toml::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    Trusted,
    Untrusted,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    System,
    User,
    Profile,
    Project,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    Active,
    Inactive,
    Conditional,
}
#[derive(Debug, Clone)]
pub struct Layer {
    pub path: PathBuf,
    pub scope: Scope,
    pub activity: Activity,
    pub status: String,
    pub present: bool,
    pub findings: Vec<String>,
    pub definitions: Vec<Definition>,
}
#[derive(Debug, Clone)]
pub struct Definition {
    pub key: String,
    pub value: String,
    pub supported: bool,
    pub ignored: bool,
    control: Option<Control>,
}
#[derive(Debug, Clone)]
enum Control {
    Names(Vec<String>),
    Bytes(usize),
}
#[derive(Debug, Clone)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub source: Option<usize>,
    pub conditional: Option<(String, usize)>,
    pub definitions: Vec<(usize, String)>,
}
#[derive(Debug, Clone)]
struct SkillRule {
    path: Option<PathBuf>,
    enabled: Option<bool>,
    source: usize,
    issue: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Enablement {
    pub label: String,
    pub explanation: String,
    pub source: Option<usize>,
}
#[derive(Debug, Clone)]
pub struct Configuration {
    pub layers: Vec<Layer>,
    pub settings: Vec<Setting>,
    pub trust: Trust,
    pub directories: Vec<PathBuf>,
    pub profile: Option<String>,
    pub findings: Vec<String>,
    rules: Vec<SkillRule>,
    rule_layers: BTreeSet<usize>,
    incomplete: bool,
}
#[derive(Debug, Clone)]
pub struct Controls {
    pub fallback: Vec<String>,
    pub max_bytes: usize,
    pub uncertain: bool,
}

const IGNORED: &[&str] = &[
    "openai_base_url",
    "chatgpt_base_url",
    "apps_mcp_product_sku",
    "model_provider",
    "model_providers",
    "notify",
    "profile",
    "profiles",
    "experimental_realtime_ws_base_url",
    "otel",
];
const KEYS: &[&str] = &[
    "model",
    "model_reasoning_effort",
    "model_reasoning_summary",
    "model_verbosity",
    "sandbox_mode",
    "approval_policy",
    "project_doc_max_bytes",
    "project_doc_fallback_filenames",
    "project_root_markers",
    "sandbox_workspace_write.network_access",
    "features.shell_tool",
    "features.web_search_request",
    "features.multi_agent",
    "features.unified_exec",
    "features.apply_patch_freeform",
    "features.skills",
];
fn at<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    key.split('.').try_fold(value, |v, k| v.get(k))
}
fn basename(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn names(v: &Value) -> Option<Vec<String>> {
    let a = v.as_array()?;
    if a.len() > 16 {
        return None;
    }
    a.iter()
        .map(|v| v.as_str().filter(|s| basename(s)).map(str::to_owned))
        .collect()
}
fn display(key: &str, value: &Value) -> Option<(String, Option<Control>)> {
    if matches!(
        key,
        "project_root_markers" | "project_doc_fallback_filenames"
    ) {
        let n = names(value)?;
        // Fallbacks are instruction files, never arbitrary credential or configuration files.
        if key == "project_doc_fallback_filenames"
            && n.iter()
                .any(|s| !s.ends_with(".md") || s.to_lowercase().contains("auth"))
        {
            return None;
        }
        return Some((format!("{n:?}"), Some(Control::Names(n))));
    }
    if key == "project_doc_max_bytes" {
        let n = usize::try_from(value.as_integer()?).ok()?;
        return Some((n.to_string(), Some(Control::Bytes(n))));
    }
    if key.starts_with("features.") || key == "sandbox_workspace_write.network_access" {
        return Some((value.as_bool()?.to_string(), None));
    }
    let s = value.as_str()?;
    let valid = match key {
        "model" => {
            let parts: Vec<_> = s.split('-').collect();
            let (version, suffix) = if parts.first() == Some(&"gpt") && parts.len() >= 2 {
                (parts[1], &parts[2..])
            } else {
                ("", &parts[..])
            };
            s.len() <= 40
                && !version.is_empty()
                && version.len() <= 8
                && version.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                && suffix.iter().all(|p| {
                    [
                        "codex", "mini", "nano", "pro", "preview", "chat", "latest", "sol",
                        "astra", "terra", "luna",
                    ]
                    .contains(p)
                })
                || ["o3", "o3-mini", "o3-pro", "o4-mini"].contains(&s)
        }
        "model_reasoning_effort" => {
            ["none", "minimal", "low", "medium", "high", "xhigh", "max"].contains(&s)
        }
        "model_reasoning_summary" => ["auto", "concise", "detailed", "none"].contains(&s),
        "model_verbosity" => ["low", "medium", "high"].contains(&s),
        "sandbox_mode" => ["read-only", "workspace-write", "danger-full-access"].contains(&s),
        "approval_policy" => ["on-request", "never"].contains(&s),
        _ => false,
    };
    valid.then(|| (s.to_owned(), None))
}
fn load(context: &Context, path: PathBuf, scope: Scope) -> (Layer, Option<Value>) {
    let mut layer = Layer {
        path,
        scope,
        activity: Activity::Active,
        status: "Absent".into(),
        present: false,
        findings: vec![],
        definitions: vec![],
    };
    let text = match read(context, &layer.path) {
        Ok(s) => {
            layer.present = true;
            s
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (layer, None),
        Err(_) => {
            layer.status = "Unavailable".into();
            layer.findings.push("Warning: unreadable, oversized, non-UTF-8 or unsafe filesystem structure; content withheld".into());
            return (layer, None);
        }
    };
    let Ok(value) = toml::from_str::<Value>(&text) else {
        layer.status = "Invalid TOML".into();
        layer
            .findings
            .push("Error: invalid TOML (parser detail withheld to protect values)".into());
        return (layer, None);
    };
    layer.status = "Parsed".into();
    for &key in KEYS {
        if let Some(v) = at(&value, key) {
            let d = display(key, v);
            if d.is_none() {
                layer.findings.push(format!(
                    "Warning: {key}: unsupported value or shape; redacted, resolution uncertain"
                ));
            }
            let (display, control) = d
                .clone()
                .unwrap_or_else(|| ("[redacted; resolution uncertain]".into(), None));
            layer.definitions.push(Definition {
                key: key.into(),
                value: display,
                supported: d.is_some(),
                ignored: false,
                control,
            });
        }
    }
    if value.get("profile").is_some() || value.get("profiles").is_some() {
        layer.findings.push("Warning: legacy profile configuration observed; not applied. Select a separate profile file explicitly.".into());
    }
    if scope == Scope::Project {
        for key in IGNORED {
            if value.get(*key).is_some() {
                layer.findings.push(format!("Informational: {key} ignored at project scope (documented restriction); values withheld"));
            }
        }
        if value.get("project_root_markers").is_some() {
            layer.findings.push("Warning: project root markers cannot retroactively select the layers used to discover this project; root uses system/user/selected-profile evidence.".into());
        }
    }
    if at(&value, "sandbox_mode").and_then(Value::as_str) == Some("danger-full-access")
        || at(&value, "approval_policy").and_then(Value::as_str) == Some("never")
    {
        layer.findings.push("Informational review: expanded access or disabled approval prompts; not a vulnerability assessment".into());
    }
    let count = value.as_table().map_or(0, |t| {
        t.keys()
            .filter(|k| {
                !KEYS
                    .iter()
                    .any(|known| known.split('.').next() == Some(k.as_str()))
                    && !["projects", "skills", "profile", "profiles"].contains(&k.as_str())
            })
            .count()
    });
    // No unknown key names or values escape, including names of arbitrary TOML tables.
    layer.findings.push(format!("Informational: {count} other top-level entries withheld; unknown keys, nested fields and merge semantics are not resolved"));
    (layer, Some(value))
}
fn documented_default(key: &str) -> Option<&'static str> {
    match key {
        "project_doc_max_bytes" => Some("32768"),
        "project_doc_fallback_filenames" => Some("[]"),
        "project_root_markers" => Some("[\".git\"]"),
        _ => None,
    }
}
fn resolve_settings(layers: &[Layer]) -> Vec<Setting> {
    KEYS.iter()
        .filter_map(|key| {
            let defs: Vec<_> = layers
                .iter()
                .enumerate()
                .flat_map(|(i, l)| {
                    l.definitions
                        .iter()
                        .filter(move |d| d.key == *key)
                        .map(move |d| (i, d))
                })
                .collect();
            if defs.is_empty() && documented_default(key).is_none() {
                return None;
            }
            let active = defs
                .iter()
                .rev()
                .find(|(i, d)| layers[*i].activity == Activity::Active && !d.ignored);
            let conditional = defs
                .iter()
                .rev()
                .find(|(i, d)| layers[*i].activity == Activity::Conditional && !d.ignored)
                .map(|(i, d)| (d.value.clone(), *i));
            Some(Setting {
                key: key.to_string(),
                value: active.map_or(
                    documented_default(key)
                        .unwrap_or("Not established locally")
                        .into(),
                    |(_, d)| d.value.clone(),
                ),
                source: active.map(|(i, _)| *i),
                conditional,
                definitions: defs.iter().map(|(i, d)| (*i, d.value.clone())).collect(),
            })
        })
        .collect()
}
fn root_dirs(context: &Context, markers: &[String]) -> Vec<PathBuf> {
    let ancestors: Vec<_> = context
        .project
        .ancestors()
        .take(64)
        .take_while(|p| context.permits(p))
        .collect();
    let count = ancestors
        .iter()
        .position(|p| {
            markers
                .iter()
                .any(|m| std::fs::symlink_metadata(p.join(m)).is_ok())
        })
        .map_or(1, |i| i + 1);
    ancestors
        .into_iter()
        .take(count)
        .rev()
        .map(|p| p.to_owned())
        .collect()
}
fn trust(value: Option<&Value>, directories: &[PathBuf]) -> Trust {
    let Some(projects) = value
        .and_then(|v| v.get("projects"))
        .and_then(Value::as_table)
    else {
        return Trust::Unknown;
    };
    let mut found = BTreeSet::new();
    for (path, settings) in projects.iter().take(4096) {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            continue;
        }
        if let Ok(path) = filesystem::resolve(&path)
            && (directories.first() == Some(&path) || directories.last() == Some(&path))
            && let Some(level) = settings.get("trust_level").and_then(Value::as_str)
        {
            found.insert(level);
        }
    }
    match found.into_iter().collect::<Vec<_>>().as_slice() {
        ["trusted"] => Trust::Trusted,
        ["untrusted"] => Trust::Untrusted,
        _ => Trust::Unknown,
    }
}

pub fn collect(context: &Context, cancel: &AtomicBool) -> io::Result<Configuration> {
    context.validate()?;
    let mut raw = vec![
        load(context, context.system.join("config.toml"), Scope::System),
        load(context, context.codex_home.join("config.toml"), Scope::User),
    ];
    if let Some(profile) = &context.profile {
        raw.push(load(
            context,
            context.codex_home.join(format!("{profile}.config.toml")),
            Scope::Profile,
        ));
    }
    let mut markers = vec![".git".into()];
    for (_, value) in &raw {
        if let Some(n) = value
            .as_ref()
            .and_then(|v| v.get("project_root_markers"))
            .and_then(names)
        {
            markers = n;
        }
    }
    let directories = root_dirs(context, &markers);
    let user_trust = trust(raw[1].1.as_ref(), &directories);
    // Trust written outside the documented user trust store is not silently merged.
    let other_trust = raw
        .iter()
        .enumerate()
        .any(|(n, (_, v))| n != 1 && v.as_ref().is_some_and(|v| v.get("projects").is_some()));
    let trust = if other_trust {
        Trust::Unknown
    } else {
        user_trust
    };
    for directory in &directories {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("collection cancelled"));
        }
        let (mut layer, value) = load(
            context,
            directory.join(".codex/config.toml"),
            Scope::Project,
        );
        layer.activity = match trust {
            Trust::Trusted => Activity::Active,
            Trust::Untrusted => Activity::Inactive,
            Trust::Unknown => Activity::Conditional,
        };
        if layer.present && trust != Trust::Trusted {
            layer.findings.push(format!(
                "Informational: project trust {trust:?}; configuration {}",
                if trust == Trust::Unknown {
                    "conditional"
                } else {
                    "inactive"
                }
            ));
        }
        raw.push((layer, value));
    }
    let mut rules = vec![];
    let mut rule_layers = BTreeSet::new();
    for (i, (layer, value)) in raw.iter_mut().enumerate() {
        let Some(v) = value.as_ref().and_then(|v| at(v, "skills.config")) else {
            continue;
        };
        rule_layers.insert(i);
        let Some(entries) = v.as_array().filter(|a| a.len() <= 1024) else {
            rules.push(SkillRule {
                path: None,
                enabled: None,
                source: i,
                issue: Some("Invalid skills.config array".into()),
            });
            continue;
        };
        for entry in entries {
            let enabled = entry.get("enabled").and_then(Value::as_bool);
            let mut issue = None;
            let path = entry.get("path").and_then(Value::as_str).and_then(|p| {
                if p.len() > 4096 {
                    return None;
                }
                let path = PathBuf::from(p);
                let path = if path.is_absolute() {
                    path
                } else if layer.scope == Scope::Project {
                    layer.path.parent()?.join(path)
                } else {
                    issue = Some(
                        "Relative path outside project layer: base semantics not established"
                            .into(),
                    );
                    return None;
                };
                // Resolve identity only; never read the referenced directory's contents.
                let path = filesystem::resolve(&path).ok()?;
                if !context.permits(&path) {
                    return None;
                }
                Some(if path.file_name().is_some_and(|s| s == "SKILL.md") {
                    path.parent()?.to_owned()
                } else {
                    path
                })
            });
            if path.is_none() || enabled.is_none() {
                issue.get_or_insert(
                    "Missing, inaccessible or unsupported skill reference/value".into(),
                );
            }
            if issue.is_some() {
                layer
                    .findings
                    .push("Warning: unresolved skills.config reference (value withheld)".into());
            }
            rules.push(SkillRule {
                path,
                enabled,
                source: i,
                issue,
            });
        }
    }
    let incomplete = raw.iter().any(|(l, _)| {
        l.activity != Activity::Inactive
            && (l.status == "Unavailable"
                || l.status == "Invalid TOML"
                || (l.scope == Scope::Profile && !l.present))
    });
    let layers: Vec<_> = raw.into_iter().map(|(l, _)| l).collect();
    let settings = resolve_settings(&layers);
    let mut findings = vec![LIMITATIONS.into(), "Trust modeled from exact canonical user project-root/selected-directory entries; contradictory, other ancestor or worktree mappings remain unknown.".into(), "Root discovery uses nearest configured marker within 64 ancestors; without a marker only the selected directory is inspected.".into()];
    findings.push(format!("Project trust: {trust:?}; source: user trust entries in {}. System/profile trust-table merging is not modeled.", privacy::path(&context.codex_home.join("config.toml"))));
    if other_trust {
        findings.push(
            "Warning: projects table outside user trust store; trust resolution is conditional"
                .into(),
        );
    }
    if incomplete {
        findings.push("Warning: incomplete applicable configuration; locally resolved values and eligibility may be incomplete".into());
    }
    if context.profile.is_some()
        && layers
            .iter()
            .any(|l| l.scope == Scope::Profile && !l.present)
    {
        findings.push("Error: selected profile file missing or inaccessible".into());
    }
    if rule_layers
        .iter()
        .filter(|&&i| layers[i].activity != Activity::Inactive)
        .count()
        > 1
    {
        findings.push("Warning: skills.config defined in multiple applicable layers; array merge semantics not established, eligibility remains undetermined".into());
    }
    Ok(Configuration {
        layers,
        settings,
        trust,
        directories,
        profile: context.profile.clone(),
        findings,
        rules,
        rule_layers,
        incomplete,
    })
}
impl Configuration {
    pub fn controls(&self, conditional: bool) -> Controls {
        let mut c = Controls {
            fallback: vec![],
            max_bytes: 32768,
            uncertain: self.incomplete,
        };
        for layer in &self.layers {
            if layer.activity == Activity::Inactive
                || (!conditional && layer.activity == Activity::Conditional)
            {
                continue;
            }
            for d in &layer.definitions {
                match (d.key.as_str(), &d.control) {
                    ("project_doc_fallback_filenames", Some(Control::Names(n))) => {
                        c.fallback = n.clone()
                    }
                    ("project_doc_max_bytes", Some(Control::Bytes(n))) => c.max_bytes = *n,
                    (
                        "project_doc_fallback_filenames"
                        | "project_doc_max_bytes"
                        | "project_root_markers",
                        None,
                    ) => c.uncertain = true,
                    _ => (),
                }
            }
        }
        c
    }
    pub fn correlate(&self, inventory: &skills::Inventory) -> Vec<Enablement> {
        inventory.skills.iter().map(|s| {
            let relevant: Vec<_> = self.rules.iter().filter(|r| self.layers[r.source].activity != Activity::Inactive && r.path.as_ref().is_some_and(|p| s.resolved.as_ref() == Some(p))).collect();
            let ambiguous = self.incomplete || self.rule_layers.iter().filter(|&&i| self.layers[i].activity != Activity::Inactive).count() > 1 || self.rules.iter().any(|r| self.layers[r.source].activity != Activity::Inactive && r.issue.is_some()) || relevant.iter().any(|r| self.layers[r.source].activity == Activity::Conditional) || relevant.iter().map(|r| r.enabled).collect::<BTreeSet<_>>().len() > 1;
            let source = relevant.last().map(|r| r.source);
            let (label, explanation) = if ambiguous { ("Undetermined", "Conditional trust, incomplete metadata, conflicting entries or unknown array merge semantics") }
            else if relevant.last().is_some_and(|r| r.enabled == Some(false)) { ("Explicitly disabled", "Applicable path-based configuration disables this discovered skill") }
            else if relevant.last().is_some_and(|r| r.enabled == Some(true)) { ("Explicitly enabled", "Applicable path-based configuration enables this skill; eligibility still requires valid discovery metadata") }
            else if s.metadata == skills::Validity::Valid && s.accessible && !s.incomplete && !s.duplicate && matches!(s.scope, skills::Scope::User | skills::Scope::Project | skills::Scope::Admin) { ("Eligible", "Valid discovered skill with no observed disabling rule under the modeled local configuration") }
            else { ("Undetermined", "Discovery validation or activation of legacy/system-cache sources is not established") };
            Enablement { label:label.into(), explanation:format!("{explanation}. Runtime overrides/policies and actual invocation are not observed."), source }
        }).collect()
    }
    pub fn unresolved_skills(&self, inventory: &skills::Inventory) -> usize {
        self.rules
            .iter()
            .filter(|r| {
                r.issue.is_some()
                    || !inventory
                        .skills
                        .iter()
                        .any(|s| r.path.is_some() && r.path == s.resolved)
            })
            .count()
    }
    pub fn source_text(&self, i: usize) -> String {
        let Some(l) = self.layers.get(i) else {
            return "Source unavailable".into();
        };
        let mut text = format!(
            "{}\n{:?} · {:?} · {}\n\n",
            privacy::path(&l.path),
            l.scope,
            l.activity,
            l.status
        );
        for d in &l.definitions {
            text.push_str(&format!("{}: {}\n", d.key, d.value));
        }
        for (n, rule) in self.rules.iter().filter(|r| r.source == i).enumerate() {
            text.push_str(&format!("\nSkill rule {}: enabled {}\nDirectory: {}\n{}\n", n + 1,
                rule.enabled.map_or("undetermined".into(), |b| b.to_string()),
                rule.path.as_ref().map_or("unresolved (original value withheld)".into(), |p| privacy::path(p)),
                rule.issue.as_deref().unwrap_or("Canonical reference; inspect Skills for discovery/eligibility. Array merging across layers is not inferred.")));
        }
        text.push_str("\nRaw TOML is never displayed. Unknown fields and values withheld.\n\n");
        text.push_str(&l.findings.join("\n\n"));
        text
    }
    pub fn setting_text(&self, i: usize) -> String {
        let Some(s) = self.settings.get(i) else {
            return LIMITATIONS.into();
        };
        let mut text = format!(
            "{}\n\nLocally resolved: {}\nSource: {}\n",
            s.key,
            s.value,
            s.source.map_or(
                if documented_default(&s.key).is_some() {
                    "Documented default (runtime/managed overrides unobserved)".into()
                } else {
                    "Not established".into()
                },
                |i| privacy::path(&self.layers[i].path)
            )
        );
        if let Some((value, i)) = &s.conditional {
            text.push_str(&format!(
                "\nIf project trusted: {value}\nConditional source: {}\n",
                privacy::path(&self.layers[*i].path)
            ));
        }
        for (i, value) in &s.definitions {
            text.push_str(&format!(
                "\n{}\n{value}\n{}\n",
                privacy::path(&self.layers[*i].path),
                if Some(*i) == s.source {
                    "Selected local definition"
                } else {
                    match self.layers[*i].activity {
                        Activity::Active => "Overridden",
                        Activity::Inactive => "Inactive",
                        Activity::Conditional => "Conditional",
                    }
                }
            ));
        }
        text.push_str(&format!("\n{LIMITATIONS}"));
        text
    }
}
