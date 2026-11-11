//! Kubernetes contexts from kubeconfig files (`KUBECONFIG`, or `~/.kube/config`), read as
//! local evidence. Nothing is called: no cluster, no `kubectl`, no exec plugin.
//!
//! Privacy: secrets are parsed as presence only (`IgnoredAny`), so tokens, passwords,
//! certificate and key data never leave the parser. Server URLs keep only scheme, host
//! class and port. Exec plugins keep the program name and env variable names; arguments
//! and values are dropped. File paths for certificates, keys and tokens are not shown.
pub mod state;

use crate::{
    aws::{Read, read_file, too_open_file},
    codex::mcp::{EndpointKind, endpoint},
    inventory::{count, safe_text},
    skills::filesystem,
};
use serde::{Deserialize, de::IgnoredAny};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Embedded certificates make kubeconfigs larger than other config files.
const FILE_LIMIT: usize = 4 * 1024 * 1024;
/// `KUBECONFIG` entries beyond this are ignored.
const MAX_FILES: usize = 16;

#[derive(Clone, Debug)]
pub struct Context {
    /// In merge order: for each name, the first file that defines it wins.
    pub files: Vec<PathBuf>,
    /// The list came from `KUBECONFIG`.
    pub from_env: bool,
    pub home: Option<PathBuf>,
    /// Fixture root: nothing outside it is read.
    pub boundary: Option<PathBuf>,
}
impl Context {
    pub fn local() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let listed: Vec<PathBuf> = std::env::var_os("KUBECONFIG")
            .map(|v| {
                std::env::split_paths(&v)
                    .filter(|p| !p.as_os_str().is_empty())
                    .take(MAX_FILES)
                    .collect()
            })
            .unwrap_or_default();
        let from_env = !listed.is_empty();
        let files = if from_env {
            listed
        } else {
            home.iter().map(|h| h.join(".kube/config")).collect()
        };
        Self {
            files,
            from_env,
            home,
            boundary: None,
        }
    }
    /// Isolated layout: `home/.kube/config`; no environment.
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let root = filesystem::resolve(root)?;
        Ok(Self {
            files: vec![root.join("home/.kube/config")],
            from_env: false,
            home: Some(root.join("home")),
            boundary: Some(root),
        })
    }
    pub fn short(&self, path: &Path) -> String {
        match self.home.as_ref().and_then(|h| path.strip_prefix(h).ok()) {
            Some(rest) => safe_text(&format!("~/{}", rest.display())),
            None => safe_text(&path.display().to_string()),
        }
    }
}

// The kubeconfig subset Den reads. Unknown fields are ignored; secrets are presence only.
#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
struct File {
    current_context: Option<String>,
    #[serde(default)]
    clusters: Vec<Named<Cluster>>,
    #[serde(default)]
    users: Vec<Named<User>>,
    #[serde(default)]
    contexts: Vec<Named<ContextEntry>>,
}
#[derive(Deserialize)]
struct Named<T> {
    name: String,
    #[serde(alias = "cluster", alias = "user", alias = "context")]
    item: Option<T>,
}
#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "kebab-case")]
struct Cluster {
    /// Classified at once; the raw URL is not kept.
    #[serde(default, deserialize_with = "server")]
    server: Option<Server>,
    certificate_authority: Option<IgnoredAny>,
    certificate_authority_data: Option<IgnoredAny>,
    #[serde(default)]
    insecure_skip_tls_verify: bool,
    proxy_url: Option<IgnoredAny>,
}
#[derive(Clone)]
struct Server {
    scheme: &'static str,
    kind: Option<EndpointKind>,
    port: Option<u16>,
}
fn server<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Server>, D::Error> {
    let raw = Option::<String>::deserialize(d)?;
    Ok(raw.map(|raw| {
        let scheme = if raw
            .get(..8)
            .is_some_and(|s| s.eq_ignore_ascii_case("https://"))
        {
            "https"
        } else if raw
            .get(..7)
            .is_some_and(|s| s.eq_ignore_ascii_case("http://"))
        {
            "http"
        } else {
            "unknown scheme"
        };
        let parsed = endpoint(&raw);
        Server {
            scheme,
            port: url::Url::parse(&raw)
                .ok()
                .and_then(|u| u.port_or_known_default()),
            kind: parsed.map(|e| e.kind),
        }
    }))
}
#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "kebab-case")]
struct User {
    token: Option<IgnoredAny>,
    #[serde(rename = "tokenFile")]
    token_file: Option<IgnoredAny>,
    client_certificate: Option<IgnoredAny>,
    client_certificate_data: Option<IgnoredAny>,
    client_key: Option<IgnoredAny>,
    client_key_data: Option<IgnoredAny>,
    username: Option<IgnoredAny>,
    password: Option<IgnoredAny>,
    #[serde(rename = "as")]
    impersonate: Option<IgnoredAny>,
    exec: Option<Exec>,
    auth_provider: Option<AuthProvider>,
}
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Exec {
    /// Kept as the program's file name only.
    #[serde(default, deserialize_with = "program")]
    command: Option<String>,
    api_version: Option<String>,
    #[serde(default)]
    env: Vec<EnvName>,
    interactive_mode: Option<String>,
    #[serde(default)]
    provide_cluster_info: bool,
}
fn program<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let raw = Option::<String>::deserialize(d)?;
    Ok(raw.and_then(|c| {
        Path::new(c.split_whitespace().next()?)
            .file_name()
            .map(|n| safe_text(&n.to_string_lossy()))
    }))
}
#[derive(Deserialize, Clone)]
struct EnvName {
    name: String,
}
#[derive(Deserialize, Clone)]
struct AuthProvider {
    name: String,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub auth: String,
    pub status: String,
    pub detail: String,
    pub file: bool,
    /// Stable identity for selection across refreshes.
    pub key: String,
}

pub struct Inventory {
    pub rows: Vec<Row>,
    pub current: Option<String>,
    pub contexts: usize,
    pub coverage: Vec<String>,
    /// False when no kubeconfig file exists.
    pub found: bool,
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

fn parse(text: &str) -> Result<File, ()> {
    let options = serde_saphyr::options! {
        reject_unsupported_tags: true,
        budget: serde_saphyr::budget! {max_depth: 32, max_events: 400_000, max_total_scalar_bytes: FILE_LIMIT, max_aliases: 128, max_nodes: 200_000},
    };
    // An empty file is a valid, empty kubeconfig.
    if text.trim().is_empty() {
        return Ok(File::default());
    }
    serde_saphyr::from_str_with_options(text, options).map_err(|_| ())
}

/// Each name with the file that defines it; later files only add names (kubectl's rule).
struct Merged<T> {
    items: BTreeMap<String, (T, usize)>,
    shadowed: usize,
}
impl<T> Default for Merged<T> {
    fn default() -> Self {
        Self {
            items: BTreeMap::new(),
            shadowed: 0,
        }
    }
}
impl<T: Default> Merged<T> {
    fn add(&mut self, list: Vec<Named<T>>, file: usize) {
        for n in list {
            match self.items.entry(n.name) {
                std::collections::btree_map::Entry::Occupied(_) => self.shadowed += 1,
                std::collections::btree_map::Entry::Vacant(slot) => {
                    slot.insert((n.item.unwrap_or_default(), file));
                }
            }
        }
    }
}

pub fn discover(context: &Context, cancel: &AtomicBool) -> io::Result<Inventory> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("collection cancelled"));
    }
    let boundary = context.boundary.as_deref();
    let mut rows = vec![];
    let file_row = |path: &Path, why: &str| Row {
        name: context.short(path),
        auth: "File".into(),
        status: format!("Problem · {why}"),
        detail: format!(
            "{}\n\nThis file could not be used: {why}. Contexts in it are not shown.",
            context.short(path)
        ),
        file: true,
        key: format!("file:{}", path.display()),
    };
    let mut found = false;
    let mut current = None;
    let mut clusters = Merged::<Cluster>::default();
    let mut users = Merged::<User>::default();
    let mut contexts = Merged::<ContextEntry>::default();
    for (n, path) in context.files.iter().enumerate() {
        let text = match read_file(boundary, path, FILE_LIMIT) {
            Read::Absent => continue,
            Read::Problem(why) => {
                found = true;
                rows.push(file_row(path, why));
                continue;
            }
            Read::Text(text) => text,
        };
        found = true;
        let Ok(file) = parse(&text) else {
            rows.push(file_row(path, "not a valid kubeconfig"));
            continue;
        };
        drop(text);
        if let Some(why) = too_open_file(boundary, path) {
            let mut row = file_row(path, why);
            row.detail = format!(
                "{}\n\nOther users can read or change this file, and kubeconfigs usually hold credentials. kubectl warns about this too.",
                context.short(path)
            );
            rows.push(row);
        }
        if current.is_none() {
            current = file.current_context.filter(|c| !c.is_empty());
        }
        clusters.add(file.clusters, n);
        users.add(file.users, n);
        contexts.add(file.contexts, n);
    }
    if let Some(c) = &current
        && !contexts.items.contains_key(c)
    {
        rows.push(Row {
            name: "current-context".into(),
            auth: "File".into(),
            status: format!("Problem · '{}' is not defined", safe_text(c)),
            detail: format!(
                "current-context\n\ncurrent-context names '{}', but no file defines a context with that name. kubectl commands without --context fail.",
                safe_text(c)
            ),
            file: true,
            key: "current-context".into(),
        });
    }
    let files: Vec<String> = context.files.iter().map(|p| context.short(p)).collect();
    // The current context first, then by name.
    let mut ordered: Vec<_> = contexts.items.iter().collect();
    ordered.sort_by_key(|(name, _)| current.as_deref() != Some(name.as_str()));
    for (name, (entry, file)) in ordered {
        rows.push(context_row(
            name,
            entry,
            &files[*file],
            current.as_deref() == Some(name),
            &clusters,
            &users,
        ));
    }
    let used = |pick: fn(&ContextEntry) -> Option<&String>, name: &String| {
        contexts.items.values().any(|(c, _)| pick(c) == Some(name))
    };
    let unused_clusters = clusters
        .items
        .keys()
        .filter(|n| !used(|c| c.cluster.as_ref(), n))
        .count();
    let unused_users = users
        .items
        .keys()
        .filter(|n| !used(|c| c.user.as_ref(), n))
        .count();
    let shadowed = clusters.shadowed + users.shadowed + contexts.shadowed;
    let mut coverage = vec![format!(
        "Read: {}{} (names and structure only).",
        files.join(", "),
        if context.from_env {
            " from KUBECONFIG; for each name the first file wins"
        } else {
            ""
        }
    )];
    if unused_clusters + unused_users > 0 {
        coverage.push(format!(
            "Not used by any context: {} and {}.",
            count(unused_clusters, "cluster"),
            count(unused_users, "user")
        ));
    }
    if shadowed > 0 {
        coverage.push(format!(
            "{} ignored: an earlier file defines the same name.",
            count(shadowed, "entry")
        ));
    }
    coverage.push("Not shown: server hosts, tokens, passwords, certificate and key data, file paths for them, exec arguments and env values.".into());
    coverage.push(
        "Nothing is called: no cluster, kubectl or exec plugin, so whether access works is not checked."
            .into(),
    );
    Ok(Inventory {
        contexts: contexts.items.len(),
        rows,
        current,
        coverage,
        found,
    })
}

#[derive(Deserialize, Default, Clone)]
struct ContextEntry {
    cluster: Option<String>,
    user: Option<String>,
    namespace: Option<String>,
}

/// In-tree auth providers removed from kubectl 1.26; they need an exec plugin now.
const REMOVED_PROVIDERS: &[&str] = &["gcp", "azure"];

fn context_row(
    name: &str,
    entry: &ContextEntry,
    file: &str,
    current: bool,
    clusters: &Merged<Cluster>,
    users: &Merged<User>,
) -> Row {
    let mut problems = vec![];
    let mut lines = vec![safe_text(name), String::new()];
    if current {
        lines.push("Current: current-context selects this context".into());
    }
    let namespace = entry.namespace.as_deref().filter(|n| !n.is_empty());
    lines.push(format!(
        "Namespace: {}",
        namespace.map_or_else(|| "default (not set)".into(), safe_text)
    ));

    lines.push(String::new());
    match entry.cluster.as_deref() {
        None => problems.push("no cluster set".to_owned()),
        Some(c) => {
            lines.push(format!("Cluster: {}", safe_text(c)));
            match clusters.items.get(c) {
                None => problems.push(format!("cluster '{}' is not defined", safe_text(c))),
                Some((cluster, _)) => cluster_lines(cluster, &mut lines, &mut problems),
            }
        }
    }

    lines.push(String::new());
    let auth = match entry.user.as_deref() {
        None => {
            lines.push("User: not set".into());
            "None set".to_owned()
        }
        Some(u) => {
            lines.push(format!("User: {}", safe_text(u)));
            match users.items.get(u) {
                None => {
                    problems.push(format!("user '{}' is not defined", safe_text(u)));
                    "Unknown".to_owned()
                }
                Some((user, _)) => user_lines(user, &mut lines, &mut problems),
            }
        }
    };

    lines.push(String::new());
    lines.push(format!("Defined in: {file}"));
    if !problems.is_empty() {
        lines.push(String::new());
        lines.push(format!("Problems: {}", problems.join("; ")));
    }
    let namespace = namespace.map_or_else(|| "default".into(), safe_text);
    let status = match problems.first() {
        Some(p) => format!("Problem · {p}"),
        None if current => format!("Ready · current · {namespace}"),
        None => format!("Ready · {namespace}"),
    };
    Row {
        name: safe_text(name),
        auth,
        status,
        detail: lines.join("\n"),
        file: false,
        key: format!("context:{name}"),
    }
}

fn cluster_lines(cluster: &Cluster, lines: &mut Vec<String>, problems: &mut Vec<String>) {
    match &cluster.server {
        None => problems.push("the cluster has no server".into()),
        Some(s) => {
            let place = match s.kind {
                Some(EndpointKind::Loopback) => "this machine",
                Some(EndpointKind::PrivateIp) => "private network address",
                Some(EndpointKind::OtherIp) => "public IP address",
                Some(EndpointKind::RemoteHostname) => "host name",
                None => "unrecognised address",
            };
            let port = s.port.map_or_else(String::new, |p| format!(" · port {p}"));
            lines.push(format!(
                "Server: {} · {place}{port} · host withheld",
                s.scheme
            ));
            if s.scheme == "http" {
                problems.push("the server uses plain http".into());
            }
        }
    }
    lines.push(format!(
        "Certificate authority: {}",
        if cluster.certificate_authority_data.is_some() {
            "embedded · withheld"
        } else if cluster.certificate_authority.is_some() {
            "from a file"
        } else {
            "system trust store"
        }
    ));
    if cluster.insecure_skip_tls_verify {
        lines.push("TLS verification: off (insecure-skip-tls-verify)".into());
        problems.push("TLS verification is off".into());
    }
    if cluster.proxy_url.is_some() {
        lines.push("Proxy: set · URL withheld".into());
    }
}

/// The user's lines; returns the Auth column. kubectl prefers an exec plugin or auth
/// provider, then a client certificate, then a token, then basic auth.
fn user_lines(user: &User, lines: &mut Vec<String>, problems: &mut Vec<String>) -> String {
    let cert = user.client_certificate.is_some() || user.client_certificate_data.is_some();
    let key = user.client_key.is_some() || user.client_key_data.is_some();
    let auth = if let Some(exec) = &user.exec {
        let program = exec.command.clone().unwrap_or_else(|| "no command".into());
        lines.push(format!(
            "Auth: exec plugin · {program} (runs when kubectl connects; not run)"
        ));
        lines.push("Arguments: withheld".into());
        if !exec.env.is_empty() {
            let names: Vec<_> = exec.env.iter().map(|e| safe_text(&e.name)).collect();
            lines.push(format!("Env (names only): {}", names.join(", ")));
        }
        if let Some(v) = &exec.api_version {
            lines.push(format!("API version: {}", safe_text(v)));
        }
        if let Some(m) = &exec.interactive_mode {
            lines.push(format!("Interactive mode: {}", safe_text(m)));
        }
        if exec.provide_cluster_info {
            lines.push("Receives cluster info: yes".into());
        }
        if exec.command.is_none() {
            problems.push("the exec plugin has no command".into());
        }
        format!("Exec · {program}")
    } else if let Some(p) = &user.auth_provider {
        let name = safe_text(&p.name);
        lines.push(format!(
            "Auth: auth provider · {name} (configuration withheld)"
        ));
        if REMOVED_PROVIDERS.contains(&p.name.as_str()) {
            problems.push(format!(
                "the {name} auth provider was removed in kubectl 1.26; use an exec plugin"
            ));
        }
        format!("{name} provider")
    } else if cert {
        lines.push(format!(
            "Auth: client certificate · {}",
            if user.client_certificate_data.is_some() {
                "embedded · withheld"
            } else {
                "from a file"
            }
        ));
        if !key {
            problems.push("client certificate without a key".into());
        }
        "Client cert".into()
    } else if user.token.is_some() || user.token_file.is_some() {
        lines.push(format!(
            "Auth: bearer token · {}",
            if user.token.is_some() {
                "in the file · withheld"
            } else {
                "from a file"
            }
        ));
        "Token".into()
    } else if user.username.is_some() || user.password.is_some() {
        lines.push("Auth: basic auth · withheld".into());
        if user.password.is_some() {
            problems.push("a password is stored in the kubeconfig".into());
        }
        "Basic auth".into()
    } else {
        lines.push("Auth: none set".into());
        "None set".into()
    };
    if user.impersonate.is_some() {
        lines.push("Impersonation: set · withheld".into());
    }
    auth
}

#[cfg(test)]
mod tests {
    #[test]
    fn secrets_are_presence_only() {
        let file = super::parse(
            "users:\n- name: u\n  user:\n    token: PRIVATE-TOKEN\n    client-key-data: PRIVATE-KEY\n    exec:\n      command: /usr/local/bin/aws\n      args: [PRIVATE-ARG]\n      env: [{name: AWS_PROFILE, value: PRIVATE-ENV}]\nclusters:\n- name: c\n  cluster: {server: 'https://PRIVATE-HOST.example:6443'}\n",
        )
        .unwrap_or_else(|_| panic!("valid kubeconfig"));
        let user = file.users[0].item.clone().unwrap();
        assert!(user.token.is_some() && user.client_key_data.is_some());
        let exec = user.exec.unwrap();
        assert_eq!(exec.command.as_deref(), Some("aws"));
        assert_eq!(exec.env[0].name, "AWS_PROFILE");
        let server = file.clusters[0].item.clone().unwrap().server.unwrap();
        assert_eq!(server.port, Some(6443));
        assert!(matches!(
            server.kind,
            Some(crate::codex::mcp::EndpointKind::RemoteHostname)
        ));
    }
}
