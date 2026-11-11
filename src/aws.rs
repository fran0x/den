//! AWS CLI/SDK profiles from `~/.aws/config` and `~/.aws/credentials`, read as local
//! evidence. Nothing is called: no AWS API, no SSO login, no credential process.
//!
//! Privacy: the credentials file is read line by line and only section names and key
//! names are kept; every value is dropped as soon as the line is split. In the config
//! file, values are kept only for an allowlist of non-secret keys; URLs, external IDs
//! and credential helper commands are withheld. SSO token caches are never read.
pub mod state;

use crate::{inventory::safe_text, skills::filesystem, status::Status};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const FILE_LIMIT: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Context {
    pub config: PathBuf,
    pub credentials: PathBuf,
    /// `AWS_PROFILE` (or `AWS_DEFAULT_PROFILE`): a profile name, not a secret.
    pub active: Option<String>,
    pub home: Option<PathBuf>,
    /// Fixture root: nothing outside it is read.
    pub boundary: Option<PathBuf>,
}
impl Context {
    pub fn local() -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let aws = home.as_ref().map(|h| h.join(".aws"));
        let file = |var: &str, name: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .or_else(|| aws.as_ref().map(|a| a.join(name)))
                .ok_or_else(|| io::Error::other("HOME unavailable"))
        };
        let active = ["AWS_PROFILE", "AWS_DEFAULT_PROFILE"]
            .iter()
            .find_map(|v| std::env::var(v).ok())
            .filter(|p| !p.is_empty() && p.len() <= 128);
        Ok(Self {
            config: file("AWS_CONFIG_FILE", "config")?,
            credentials: file("AWS_SHARED_CREDENTIALS_FILE", "credentials")?,
            active,
            home,
            boundary: None,
        })
    }
    /// Isolated layout: `home/.aws/config` and `home/.aws/credentials`; no environment.
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let root = filesystem::resolve(root)?;
        Ok(Self {
            config: root.join("home/.aws/config"),
            credentials: root.join("home/.aws/credentials"),
            active: None,
            home: Some(root.join("home")),
            boundary: Some(root),
        })
    }
    pub fn short(&self, path: &Path) -> String {
        match &self.home {
            Some(home) => match path.strip_prefix(home) {
                Ok(rest) => safe_text(&format!("~/{}", rest.display())),
                Err(_) => safe_text(&path.display().to_string()),
            },
            None => safe_text(&path.display().to_string()),
        }
    }
}

/// How a profile gets credentials, in the order the AWS SDKs check them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auth {
    /// `role_arn`: assumes a role using another profile, a credential source or a web
    /// identity token.
    Role,
    Sso,
    /// `credential_process`: runs a command.
    Process,
    StaticKeys,
    /// Nothing set: the SDK falls back to the environment, instance roles and so on.
    None,
}
impl Auth {
    pub fn label(self) -> &'static str {
        match self {
            Self::Role => "Role",
            Self::Sso => "SSO",
            Self::Process => "Process",
            Self::StaticKeys => "Static keys",
            Self::None => "None set",
        }
    }
}

/// Config values Den shows. Everything else is listed by key name only.
const SHOWN: &[&str] = &[
    "region",
    "output",
    "sso_session",
    "sso_account_id",
    "sso_role_name",
    "sso_region",
    "sso_registration_scopes",
    "role_arn",
    "source_profile",
    "credential_source",
    "mfa_serial",
    "role_session_name",
    "duration_seconds",
    "services",
    "cli_pager",
    "retry_mode",
    "max_attempts",
];
/// Keys whose presence matters but whose values are withheld.
const WITHHELD: &[(&str, &str)] = &[
    ("sso_start_url", "URL withheld"),
    ("endpoint_url", "URL withheld"),
    ("external_id", "withheld"),
    ("credential_process", "runs a command · withheld"),
    ("web_identity_token_file", "path withheld"),
    (
        "aws_access_key_id",
        "static key in the config file · withheld",
    ),
    (
        "aws_secret_access_key",
        "static key in the config file · withheld",
    ),
    (
        "aws_session_token",
        "session token in the config file · withheld",
    ),
];

/// One `[section]`: kept values (allowlist only) and every key name.
#[derive(Clone, Debug, Default)]
struct Section {
    values: BTreeMap<String, String>,
    keys: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    pub kind: Kind,
    pub auth: String,
    pub status: Status,
    pub detail: String,
    /// Stable identity for selection across refreshes.
    pub key: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Profile,
    SsoSession,
    File,
}

pub struct Inventory {
    pub rows: Vec<Row>,
    pub active: Option<String>,
    pub sso_sessions: usize,
    pub coverage: Vec<String>,
    /// False when neither the config nor the credentials file exists.
    pub found: bool,
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

/// One file read: absent, its text, or why it could not be used.
pub(crate) enum Read {
    Absent,
    Text(String),
    Problem(&'static str),
}

fn read(context: &Context, path: &Path) -> Read {
    read_file(context.boundary.as_deref(), path, FILE_LIMIT)
}

/// Descriptor-relative read of a regular file inside `boundary` (when set). Linked files
/// are reported, not read.
pub(crate) fn read_file(boundary: Option<&Path>, path: &Path, limit: usize) -> Read {
    if boundary.is_some_and(|root| !path.starts_with(root)) {
        return Read::Absent;
    }
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Read::Absent,
        Ok(m) if m.file_type().is_symlink() => return Read::Problem("a link; not read"),
        Err(_) => return Read::Problem("unreadable"),
        _ => {}
    }
    #[cfg(unix)]
    {
        let Some(name) = path.file_name() else {
            return Read::Problem("unreadable");
        };
        let text = path
            .parent()
            .ok_or_else(|| io::Error::other("missing parent"))
            .and_then(filesystem::resolve)
            .and_then(|parent| filesystem::Directory::open(&parent))
            .and_then(|dir| dir.child(name))
            .and_then(|file| filesystem::text(file, limit));
        match text {
            Ok(text) => Read::Text(text),
            Err(_) => Read::Problem("unreadable, too large or not UTF-8"),
        }
    }
    #[cfg(not(unix))]
    Read::Problem("secure reads require macOS/Linux")
}

/// INI sections. With `keep_values` false (the credentials file), every value is
/// dropped as the line is split, so only section and key names leave this function.
fn sections(text: &str, keep_values: bool) -> BTreeMap<String, Section> {
    let mut found: BTreeMap<String, Section> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        // Indented lines continue a nested setting (for example s3 = ...); skip them.
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim().to_owned();
            found.entry(name.clone()).or_default();
            current = Some(name);
            continue;
        }
        let (Some(section), Some((key, value))) = (&current, line.split_once('=')) else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let entry = found.entry(section.clone()).or_default();
        if keep_values && SHOWN.contains(&key.as_str()) {
            entry.values.insert(key.clone(), safe_text(value.trim()));
        }
        entry.keys.insert(key);
    }
    found
}

pub fn discover(context: &Context, cancel: &AtomicBool) -> io::Result<Inventory> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("collection cancelled"));
    }
    let mut rows = vec![];
    let file_row = |path: &Path, why: &str| Row {
        name: context.short(path),
        kind: Kind::File,
        auth: "File".into(),
        status: Status::problem(why),
        detail: format!(
            "{}\n\nThis file could not be used: {why}. Profiles in it are not shown.",
            context.short(path)
        ),
        key: format!("file:{}", path.display()),
    };
    let config = read(context, &context.config);
    let credentials = read(context, &context.credentials);
    let found = !matches!((&config, &credentials), (Read::Absent, Read::Absent));
    let config = match config {
        Read::Text(text) => sections(&text, true),
        Read::Absent => BTreeMap::new(),
        Read::Problem(why) => {
            rows.push(file_row(&context.config, why));
            BTreeMap::new()
        }
    };
    let credentials = match credentials {
        Read::Text(text) => sections(&text, false),
        Read::Absent => BTreeMap::new(),
        Read::Problem(why) => {
            rows.push(file_row(&context.credentials, why));
            BTreeMap::new()
        }
    };
    if let Some(why) = too_open(context) {
        rows.push(file_row(&context.credentials, why));
    }

    // Profiles: [default] and [profile x] in config; [x] in credentials.
    let mut profiles: BTreeMap<String, (Option<&Section>, Option<&Section>)> = BTreeMap::new();
    let mut sso: BTreeMap<String, &Section> = BTreeMap::new();
    let mut services = 0;
    for (name, section) in &config {
        if name == "default" {
            profiles.entry(name.clone()).or_default().0 = Some(section);
        } else if let Some(p) = name.strip_prefix("profile ") {
            profiles.entry(p.trim().to_owned()).or_default().0 = Some(section);
        } else if let Some(s) = name.strip_prefix("sso-session ") {
            sso.insert(s.trim().to_owned(), section);
        } else if name.starts_with("services ") {
            services += 1;
        }
    }
    for (name, section) in &credentials {
        profiles.entry(name.clone()).or_default().1 = Some(section);
    }
    let mut ordered: Vec<_> = profiles.keys().cloned().collect();
    ordered.sort_by_key(|n| (n != "default", n.clone()));
    for name in &ordered {
        rows.push(profile_row(context, name, &profiles, &sso));
    }
    for (name, section) in &sso {
        let users: Vec<_> = profiles
            .iter()
            .filter(|(_, (c, _))| c.and_then(|c| c.values.get("sso_session")) == Some(name))
            .map(|(p, _)| safe_text(p))
            .collect();
        let mut lines = vec![
            format!("SSO session {}", safe_text(name)),
            String::new(),
            format!(
                "Region: {}",
                section
                    .values
                    .get("sso_region")
                    .map_or("not set", String::as_str)
            ),
            format!(
                "Start URL: {}",
                if section.keys.contains("sso_start_url") {
                    "set · URL withheld"
                } else {
                    "not set"
                }
            ),
        ];
        if let Some(scopes) = section.values.get("sso_registration_scopes") {
            lines.push(format!("Scopes: {scopes}"));
        }
        lines.push(format!(
            "Used by: {}",
            if users.is_empty() {
                "no profile".into()
            } else {
                users.join(", ")
            }
        ));
        lines.push("Not shown: SSO tokens (never read) and whether a login is current.".into());
        rows.push(Row {
            name: safe_text(name),
            kind: Kind::SsoSession,
            auth: "SSO session".into(),
            status: if users.is_empty() {
                Status::off("used by no profile")
            } else {
                Status::ready(format!(
                    "used by {}",
                    crate::inventory::count(users.len(), "profile")
                ))
            },
            detail: lines.join("\n"),
            key: format!("sso:{name}"),
        });
    }
    Ok(Inventory {
        found,
        rows,
        active: context.active.clone(),
        sso_sessions: sso.len(),
        coverage: vec![
            format!(
                "Read: {} and {} (names only; credential values are discarded as they are read).",
                context.short(&context.config),
                context.short(&context.credentials)
            ),
            format!(
                "Not read: SSO and CLI caches. {} counted, not shown.",
                crate::inventory::count(services, "[services] section")
            ),
            "Nothing is called: no AWS API, SSO login or credential process. Whether credentials work is not checked.".into(),
        ],
    })
}

/// A credentials file other users can read or write.
fn too_open(context: &Context) -> Option<&'static str> {
    too_open_file(context.boundary.as_deref(), &context.credentials)
}

/// A regular file other users can read or write.
pub(crate) fn too_open_file(boundary: Option<&Path>, path: &Path) -> Option<&'static str> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if boundary.is_some_and(|root| !path.starts_with(root)) {
            return None;
        }
        let meta = std::fs::symlink_metadata(path).ok()?;
        (meta.is_file() && meta.permissions().mode() & 0o077 != 0)
            .then_some("other users can read or change it (expected 600)")
    }
    #[cfg(not(unix))]
    None
}

fn profile_row(
    context: &Context,
    name: &str,
    profiles: &BTreeMap<String, (Option<&Section>, Option<&Section>)>,
    sso: &BTreeMap<String, &Section>,
) -> Row {
    let (config, credentials) = profiles[name];
    let empty = Section::default();
    let c = config.unwrap_or(&empty);
    let value = |k: &str| c.values.get(k).map(String::as_str);
    let has = |k: &str| c.keys.contains(k);
    let static_keys = credentials.is_some_and(|s| s.keys.contains("aws_access_key_id"))
        || has("aws_access_key_id");
    let auth = if has("role_arn") {
        Auth::Role
    } else if has("sso_session") || has("sso_start_url") || has("sso_account_id") {
        Auth::Sso
    } else if has("credential_process") {
        Auth::Process
    } else if static_keys {
        Auth::StaticKeys
    } else {
        Auth::None
    };
    let mut problems = vec![];
    let mut lines = vec![safe_text(name), String::new()];
    if context.active.as_deref() == Some(name) {
        lines.push("Active: AWS_PROFILE selects this profile".into());
    } else if name == "default" && context.active.is_none() {
        lines.push("Active: the default profile (AWS_PROFILE is not set)".into());
    }
    lines.push(format!("Auth: {}", auth.label()));
    match auth {
        Auth::Role => {
            lines.push(format!("Role: {}", value("role_arn").unwrap_or("withheld")));
            match (value("source_profile"), value("credential_source")) {
                (Some(source), _) => {
                    lines.push(format!("Role chain: {}", chain(name, profiles, &mut problems)));
                    if !profiles.contains_key(source) {
                        problems.push(format!("source_profile '{source}' is not defined"));
                    }
                }
                (None, Some(source)) => lines.push(format!("Credential source: {source}")),
                (None, None) if has("web_identity_token_file") => {
                    lines.push("Web identity token file: path withheld".into())
                }
                (None, None) => problems.push(
                    "role_arn needs source_profile, credential_source or web_identity_token_file"
                        .into(),
                ),
            }
            if let Some(mfa) = value("mfa_serial") {
                lines.push(format!("MFA device: {mfa}"));
            }
            for k in ["role_session_name", "duration_seconds"] {
                if let Some(v) = value(k) {
                    lines.push(format!("{k}: {v}"));
                }
            }
        }
        Auth::Sso => {
            match value("sso_session") {
                Some(session) if sso.contains_key(session) => {
                    lines.push(format!("SSO session: {session}"))
                }
                Some(session) => {
                    lines.push(format!("SSO session: {session}"));
                    problems.push(format!("sso-session '{session}' is not defined"));
                }
                None => lines.push("SSO: legacy settings in the profile (no sso-session)".into()),
            }
            for (k, label) in [("sso_account_id", "Account"), ("sso_role_name", "Role")] {
                match value(k) {
                    Some(v) => lines.push(format!("{label}: {v}")),
                    None => problems.push(format!("{k} is not set")),
                }
            }
        }
        Auth::Process => {
            lines.push("credential_process: runs a command · withheld".into())
        }
        Auth::StaticKeys => lines.push(format!(
            "Static keys in {}{}",
            if credentials.is_some_and(|s| s.keys.contains("aws_access_key_id")) {
                context.short(&context.credentials)
            } else {
                context.short(&context.config)
            },
            if credentials.is_some_and(|s| s.keys.contains("aws_session_token")) {
                " · with a session token"
            } else {
                ""
            }
        )),
        Auth::None => lines.push(
            "No credentials set here: the SDK falls back to the environment, instance or container roles"
                .into(),
        ),
    }
    let region = value("region");
    lines.push(format!("Region: {}", region.unwrap_or("not set")));
    if let Some(output) = value("output") {
        lines.push(format!("Output: {output}"));
    }
    for (k, why) in WITHHELD {
        if has(k) && !matches!(*k, "credential_process") {
            lines.push(format!("{k}: {why}"));
        }
    }
    let other: Vec<_> = c
        .keys
        .iter()
        .filter(|k| !SHOWN.contains(&k.as_str()) && !WITHHELD.iter().any(|(w, _)| w == *k))
        .map(|k| safe_text(k))
        .collect();
    if !other.is_empty() {
        lines.push(format!(
            "Other settings (values not shown): {}",
            other.join(", ")
        ));
    }
    lines.push(String::new());
    lines.push(format!(
        "Defined in: {}",
        [
            config.map(|_| context.short(&context.config)),
            credentials.map(|_| format!("{} (names only)", context.short(&context.credentials))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    ));
    if !problems.is_empty() {
        lines.push(String::new());
        lines.push(format!("Problems: {}", problems.join("; ")));
    }
    lines.push(
        "Not shown: credential values, SSO tokens, and whether these credentials currently work."
            .into(),
    );
    let status = match problems.first() {
        Some(p) => Status::problem(p.as_str()),
        None => Status::ready(region.unwrap_or("no region")),
    };
    Row {
        name: safe_text(name),
        kind: Kind::Profile,
        auth: auth.label().into(),
        status,
        detail: lines.join("\n"),
        key: format!("profile:{name}"),
    }
}

/// `dev → base (static keys)`; loops and missing profiles are problems.
fn chain(
    start: &str,
    profiles: &BTreeMap<String, (Option<&Section>, Option<&Section>)>,
    problems: &mut Vec<String>,
) -> String {
    let mut seen = vec![start.to_owned()];
    let mut current = start.to_owned();
    while let Some(next) = profiles
        .get(&current)
        .and_then(|(c, _)| c.as_ref())
        .and_then(|c| c.values.get("source_profile"))
    {
        if seen.contains(next) {
            // A profile may use its own static keys as the source: that is not a loop.
            if *next != current {
                problems.push(format!("role chain loops back to '{next}'"));
            }
            break;
        }
        seen.push(next.clone());
        current = next.clone();
        if seen.len() > 16 {
            problems.push("role chain is longer than 16 profiles".into());
            break;
        }
    }
    seen.iter()
        .map(|p| safe_text(p))
        .collect::<Vec<_>>()
        .join(" → ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn credentials_values_never_leave_the_parser() {
        let parsed = super::sections(
            "[dev]\naws_access_key_id = AKIA-SECRET\naws_secret_access_key=SECRET\n",
            false,
        );
        let text = format!("{parsed:?}");
        assert!(!text.contains("SECRET"));
        assert!(parsed["dev"].keys.contains("aws_secret_access_key"));
    }
    #[test]
    fn config_keeps_only_allowlisted_values() {
        let parsed = super::sections(
            "[profile a]\nregion = eu-west-1\nsso_start_url = https://PRIVATE\ncredential_process = PRIVATE cmd\ns3 =\n  max_concurrent_requests = 10\n",
            true,
        );
        let text = format!("{parsed:?}");
        assert!(!text.contains("PRIVATE"));
        assert_eq!(parsed["profile a"].values["region"], "eu-west-1");
        assert!(parsed["profile a"].keys.contains("sso_start_url"));
        assert!(!parsed["profile a"].keys.contains("max_concurrent_requests"));
    }
}
