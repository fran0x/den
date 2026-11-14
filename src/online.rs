//! Online checks (`u`): the newest published version of the packages Den lists, from a
//! fixed set of public registries.
//!
//! Only after `u` and Enter, only HTTPS to the hosts in [`Registry::host`], with no
//! redirects, and only a package's name leaves the machine: the installed version is
//! compared here. Names are checked against each registry's rules before a URL is built.
//! Answers are kept in Den's own private file with the time of the check, so a later run
//! shows them without asking again.
use crate::inventory::safe_text;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime},
};

/// Where a package's newest version is looked up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Registry {
    CratesIo,
    Npm,
    PyPI,
}
impl Registry {
    pub const ALL: [Self; 3] = [Self::CratesIo, Self::Npm, Self::PyPI];
    /// The only host Den contacts for this registry.
    pub fn host(self) -> &'static str {
        match self {
            Self::CratesIo => "index.crates.io",
            Self::Npm => "registry.npmjs.org",
            Self::PyPI => "pypi.org",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::CratesIo => "crates.io",
            Self::Npm => "npm",
            Self::PyPI => "PyPI",
        }
    }
}

/// A package to look up: a registry and a name that follows that registry's rules.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Package {
    registry: Registry,
    name: String,
}
impl Package {
    /// `None` when the name breaks the registry's rules, so nothing odd reaches a URL.
    pub fn new(registry: Registry, name: &str) -> Option<Self> {
        let valid = match registry {
            // crates.io: ASCII letters, digits, `-` and `_`, starting with a letter.
            Registry::CratesIo => {
                (1..=64).contains(&name.len())
                    && name.starts_with(|c: char| c.is_ascii_alphabetic())
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            }
            // npm: lowercase `name` or `@scope/name`, URL-safe characters only.
            Registry::Npm => {
                let part = |p: &str| {
                    !p.is_empty()
                        && !p.starts_with(['.', '_'])
                        && p.chars().all(|c| {
                            c.is_ascii_lowercase()
                                || c.is_ascii_digit()
                                || matches!(c, '-' | '.' | '_' | '~')
                        })
                };
                name.len() <= 214
                    && match name.strip_prefix('@') {
                        Some(scoped) => scoped
                            .split_once('/')
                            .is_some_and(|(scope, rest)| part(scope) && part(rest)),
                        None => part(name),
                    }
            }
            // PyPI: letters, digits, `.`, `-` and `_`, starting and ending with a letter
            // or digit.
            Registry::PyPI => {
                (1..=100).contains(&name.len())
                    && name.starts_with(|c: char| c.is_ascii_alphanumeric())
                    && name.ends_with(|c: char| c.is_ascii_alphanumeric())
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
            }
        };
        valid.then(|| Self {
            registry,
            name: name.to_owned(),
        })
    }
    pub fn registry(&self) -> Registry {
        self.registry
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    /// How the results file names it: `crates.io:ripgrep`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.registry.label(), self.name)
    }
    /// The one URL asked for this package.
    pub fn url(&self) -> String {
        let host = self.registry.host();
        match self.registry {
            Registry::CratesIo => {
                // The sparse index's layout: 1/a, 2/ab, 3/a/abc, ab/cd/abcd….
                let n = self.name.to_ascii_lowercase();
                let path = match n.len() {
                    1 => format!("1/{n}"),
                    2 => format!("2/{n}"),
                    3 => format!("3/{}/{n}", &n[..1]),
                    _ => format!("{}/{}/{n}", &n[..2], &n[2..4]),
                };
                format!("https://{host}/{path}")
            }
            Registry::Npm => format!("https://{host}/{}/latest", self.name),
            Registry::PyPI => format!("https://{host}/pypi/{}/json", self.name),
        }
    }
}

/// A row's lookup: the package and the version installed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lookup {
    pub package: Package,
    pub installed: String,
}

/// What a registry said about a package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// The newest release (pre-releases and yanked versions left out).
    Latest(String),
    /// The registry has no package by that name, such as one installed from a folder.
    NotFound,
    /// The check did not get an answer: why, briefly.
    Failed(String),
}

/// The newest release in a registry's answer for one package.
pub fn latest(registry: Registry, body: &[u8]) -> Result<String, String> {
    let bad = || "unexpected answer".to_owned();
    let version = match registry {
        // One JSON object per published version, one per line.
        Registry::CratesIo => {
            #[derive(Deserialize)]
            struct Line {
                vers: String,
                #[serde(default)]
                yanked: bool,
            }
            let text = std::str::from_utf8(body).map_err(|_| bad())?;
            let mut best: Option<String> = None;
            for line in text.lines().filter(|l| !l.trim().is_empty()) {
                let line: Line = serde_json::from_str(line).map_err(|_| bad())?;
                if line.yanked || !crate::updates::is_release(&line.vers) {
                    continue;
                }
                if best
                    .as_deref()
                    .is_none_or(|b| crate::updates::is_newer(b, &line.vers))
                {
                    best = Some(line.vers);
                }
            }
            best.ok_or_else(|| "no released version".to_owned())?
        }
        // The `latest` tag's manifest.
        Registry::Npm => {
            #[derive(Deserialize)]
            struct Manifest {
                version: String,
            }
            serde_json::from_slice::<Manifest>(body)
                .map_err(|_| bad())?
                .version
        }
        Registry::PyPI => {
            #[derive(Deserialize)]
            struct Info {
                version: String,
            }
            #[derive(Deserialize)]
            struct Project {
                info: Info,
            }
            serde_json::from_slice::<Project>(body)
                .map_err(|_| bad())?
                .info
                .version
        }
    };
    if crate::updates::plausible(&version) {
        Ok(version)
    } else {
        Err(bad())
    }
}

/// Gets one URL: the body, `None` for "no such package" (404), or why it failed.
pub type Fetch<'a> = dyn Fn(&str) -> Result<Option<Vec<u8>>, String> + Sync + 'a;

/// Runs [`check`] in the background.
pub(crate) type Collector = crate::worker::Worker<Vec<(Package, Answer)>, io::Error>;

/// Requests at once: enough to be quick, few enough to be polite.
const PARALLEL: usize = 4;

/// Looks up each package, a few at a time. Stopping (`cancel`) waits for the requests
/// already sent and then returns an error, so nothing partial is kept.
pub fn check(
    packages: &[Package],
    fetch: &Fetch<'_>,
    cancel: &AtomicBool,
) -> io::Result<Vec<(Package, Answer)>> {
    let next = Mutex::new(packages.iter());
    let answers = Mutex::new(Vec::with_capacity(packages.len()));
    std::thread::scope(|scope| {
        for _ in 0..PARALLEL.min(packages.len()) {
            scope.spawn(|| {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    let Some(package) = next.lock().ok().and_then(|mut n| n.next()) else {
                        return;
                    };
                    let answer = match fetch(&package.url()) {
                        Ok(Some(body)) => match latest(package.registry, &body) {
                            Ok(version) => Answer::Latest(version),
                            Err(why) => Answer::Failed(why),
                        },
                        Ok(None) => Answer::NotFound,
                        Err(why) => Answer::Failed(why),
                    };
                    if let Ok(mut answers) = answers.lock() {
                        answers.push((package.clone(), answer));
                    }
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "stopped"));
    }
    let mut answers = answers
        .into_inner()
        .map_err(|_| io::Error::other("online check failed"))?;
    answers.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(answers)
}

/// The largest answer read for one package: PyPI's can reach a few MB.
const MAX_ANSWER_BYTES: u64 = 16 << 20;

/// The HTTP client: HTTPS only, the system's certificate store, no redirects (so only
/// the hosts above are reached), and 10 seconds per request. A proxy set in
/// `HTTPS_PROXY` is used.
pub fn agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(10)))
        .user_agent(concat!(
            "den/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/fran0x/den)"
        ))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

/// One GET with `agent`. Only the hosts in [`Registry::host`] are asked.
pub fn get(agent: &ureq::Agent, url: &str) -> Result<Option<Vec<u8>>, String> {
    let allowed = Registry::ALL
        .iter()
        .any(|r| url.starts_with(&format!("https://{}/", r.host())));
    if !allowed {
        return Err("not a registry Den checks".into());
    }
    match agent.get(url).call() {
        Ok(mut response) if response.status() == 200 => response
            .body_mut()
            .with_config()
            .limit(MAX_ANSWER_BYTES)
            .read_to_vec()
            .map(Some)
            .map_err(|e| failure(&e)),
        Ok(response) => Err(format!("HTTP {}", response.status().as_u16())),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(failure(&e)),
    }
}

fn failure(error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("HTTP {code}"),
        ureq::Error::Timeout(_) => "timed out".into(),
        ureq::Error::HostNotFound => "host not found".into(),
        ureq::Error::BodyExceedsLimit(_) => "answer too large".into(),
        other => safe_text(&other.to_string()).chars().take(80).collect(),
    }
}

// ---------------------------------------------------------------------------------------
// The results file

pub const FILE_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 4 << 20;

/// `$XDG_STATE_HOME/den/online.json`, or `~/.local/state/den/online.json`.
pub fn path() -> Option<PathBuf> {
    crate::actions::state_dir().map(|dir| dir.join("online.json"))
}

/// What the last checks found, by package.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub version: u32,
    pub packages: BTreeMap<String, Checked>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checked {
    /// Seconds since 1970, UTC.
    pub at: u64,
    pub answer: Answer,
}
impl Checked {
    pub fn when(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(self.at)
    }
}

/// Reads the results file: none when there is none. Anything else Den cannot use is an
/// error that says why, and the file is left as it is.
pub fn load(path: &Path) -> Result<Option<File>, String> {
    let Some(bytes) = crate::actions::read_private(path, MAX_FILE_BYTES)? else {
        return Ok(None);
    };
    #[derive(Deserialize)]
    struct Head {
        version: u32,
    }
    let not_den = || "it is not Den's online results".to_owned();
    let head: Head = serde_json::from_slice(&bytes).map_err(|_| not_den())?;
    if head.version > FILE_VERSION {
        return Err("it was saved by a newer Den".into());
    }
    if head.version == 0 {
        return Err(not_den());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| not_den())
}

// ---------------------------------------------------------------------------------------
// In the app

/// The online check's state: the answers so far, the question `u` asks, and the check
/// running.
#[derive(Debug, Default)]
pub struct State {
    /// Where answers are kept: set only for live data, never for demo or fixtures.
    pub path: Option<PathBuf>,
    pub checked: BTreeMap<String, Checked>,
    /// `u` asked about these; Enter checks them, anything else cancels.
    pub asking: Vec<Package>,
    /// Confirmed, for the event loop to start.
    pub request: Option<Vec<Package>>,
    /// How many packages the running check looks up.
    pub running: Option<usize>,
    /// Esc during a check: the loop stops it.
    pub stop: bool,
}
impl State {
    /// Live data only: reads earlier answers. A file Den cannot use is reported (the
    /// returned notice) and left alone.
    pub fn use_file(&mut self, path: PathBuf) -> Option<String> {
        let notice = match load(&path) {
            Ok(Some(file)) => {
                self.checked = file.packages;
                None
            }
            Ok(None) => None,
            Err(why) => Some(format!("Online results not used: {why}")),
        };
        self.path = Some(path);
        notice
    }
    /// The question on the status line while `u` waits for Enter.
    pub fn question(&self) -> String {
        let hosts: Vec<&str> = Registry::ALL
            .iter()
            .filter(|r| self.asking.iter().any(|p| p.registry == **r))
            .map(|r| r.host())
            .collect();
        format!(
            "Check online? Enter:check Esc:cancel · sends {} to {}; versions stay here",
            crate::inventory::count(self.asking.len(), "package name"),
            hosts.join(", ")
        )
    }
    pub fn answer(&self, package: &Package) -> Option<&Checked> {
        self.checked.get(&package.key())
    }
    /// The newer version a registry has, when it has one.
    pub fn newer(&self, lookup: &Lookup) -> Option<&str> {
        match &self.answer(&lookup.package)?.answer {
            Answer::Latest(v) if crate::updates::is_newer(&lookup.installed, v) => Some(v),
            _ => None,
        }
    }
    /// The panel's line about the last check, starting with a blank line; empty when
    /// the package was never checked.
    pub fn panel(&self, lookup: &Lookup, now: SystemTime) -> String {
        let Some(checked) = self.answer(&lookup.package) else {
            return String::new();
        };
        let registry = lookup.package.registry.label();
        let when = crate::updates::age(checked.when(), now);
        let what = match &checked.answer {
            Answer::Latest(v) if crate::updates::is_newer(&lookup.installed, v) => {
                format!("↑ {v} available on {registry}")
            }
            Answer::Latest(v) => format!("Up to date on {registry} (newest {v})"),
            Answer::NotFound => format!("Not found on {registry}"),
            Answer::Failed(why) => format!("Online check failed: {why}"),
        };
        format!("\n\n{what} · checked online {when}")
    }
    /// Keeps a finished check's answers, saves them, and says what it found.
    pub fn finish(
        &mut self,
        result: io::Result<Vec<(Package, Answer)>>,
        lookups: &[Lookup],
        now: SystemTime,
    ) -> String {
        self.running = None;
        self.stop = false;
        let answers = match result {
            Ok(answers) => answers,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                return "Online check stopped; nothing was kept".into();
            }
            Err(e) => return format!("Online check failed: {}", safe_text(&e.to_string())),
        };
        let at = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let failed = answers
            .iter()
            .filter(|(_, a)| matches!(a, Answer::Failed(_)))
            .count();
        let checked = answers.len();
        for (package, answer) in answers {
            self.checked.insert(package.key(), Checked { at, answer });
        }
        let newer = lookups.iter().filter(|l| self.newer(l).is_some()).count();
        let mut notice = format!(
            "Checked {} online: {} newer",
            crate::inventory::count(checked, "package"),
            newer
        );
        if failed > 0 {
            notice.push_str(&format!(" · {failed} failed"));
        }
        if let Some(path) = &self.path {
            let file = File {
                version: FILE_VERSION,
                packages: self.checked.clone(),
            };
            let saved = serde_json::to_vec_pretty(&file)
                .map_err(io::Error::from)
                .and_then(|mut bytes| {
                    bytes.push(b'\n');
                    crate::actions::write_private(path, &bytes)
                });
            if let Err(e) = saved {
                notice.push_str(&format!(" · not saved: {}", safe_text(&e.to_string())));
            }
        }
        notice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(registry: Registry, name: &str) -> Package {
        Package::new(registry, name).unwrap()
    }

    #[test]
    fn names_follow_each_registry() {
        for ok in ["ripgrep", "cargo-edit", "a", "serde_json"] {
            assert!(Package::new(Registry::CratesIo, ok).is_some(), "{ok}");
        }
        for bad in ["", "1abc", "a/b", "a b", "../x", "é", &"a".repeat(65)] {
            assert!(Package::new(Registry::CratesIo, bad).is_none(), "{bad}");
        }
        for ok in ["npm", "@openai/codex", "typescript", "lodash.merge"] {
            assert!(Package::new(Registry::Npm, ok).is_some(), "{ok}");
        }
        for bad in [
            "", "Upper", "@scope", "@/x", "a/b", ".hidden", "a?b", "@a/b/c",
        ] {
            assert!(Package::new(Registry::Npm, bad).is_none(), "{bad}");
        }
        for ok in ["ruff", "Django", "zope.interface", "pre-commit"] {
            assert!(Package::new(Registry::PyPI, ok).is_some(), "{ok}");
        }
        for bad in ["", "-x", "x-", "a/b", "a b", "a?b"] {
            assert!(Package::new(Registry::PyPI, bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn urls_are_fixed_per_registry() {
        let url = |r, n| package(r, n).url();
        assert_eq!(url(Registry::CratesIo, "a"), "https://index.crates.io/1/a");
        assert_eq!(
            url(Registry::CratesIo, "ab"),
            "https://index.crates.io/2/ab"
        );
        assert_eq!(
            url(Registry::CratesIo, "syn"),
            "https://index.crates.io/3/s/syn"
        );
        assert_eq!(
            url(Registry::CratesIo, "RipGrep"),
            "https://index.crates.io/ri/pg/ripgrep"
        );
        assert_eq!(
            url(Registry::Npm, "@openai/codex"),
            "https://registry.npmjs.org/@openai/codex/latest"
        );
        assert_eq!(
            url(Registry::PyPI, "ruff"),
            "https://pypi.org/pypi/ruff/json"
        );
    }

    #[test]
    fn the_newest_release_is_found() {
        let index = br#"{"name":"x","vers":"1.2.0","yanked":false}
{"name":"x","vers":"1.10.0","yanked":false}
{"name":"x","vers":"2.0.0","yanked":true}
{"name":"x","vers":"3.0.0-beta.1","yanked":false}
{"name":"x","vers":"1.9.9","yanked":false}
"#;
        assert_eq!(latest(Registry::CratesIo, index).unwrap(), "1.10.0");
        assert!(latest(Registry::CratesIo, br#"{"vers":"1.0.0-rc.1"}"#).is_err());
        assert_eq!(
            latest(Registry::Npm, br#"{"name":"npm","version":"12.2.2"}"#).unwrap(),
            "12.2.2"
        );
        assert_eq!(
            latest(
                Registry::PyPI,
                br#"{"info":{"version":"0.17.5"},"releases":{}}"#
            )
            .unwrap(),
            "0.17.5"
        );
        // Anything that is not a plain version is not believed.
        assert!(latest(Registry::Npm, br#"{"version":"\u001b[31m1.0"}"#).is_err());
        assert!(latest(Registry::PyPI, b"<html>").is_err());
    }

    #[test]
    fn a_check_asks_each_package_once_and_keeps_every_answer() {
        let asked = Mutex::new(vec![]);
        let fetch = |url: &str| {
            asked.lock().unwrap().push(url.to_owned());
            match url {
                "https://registry.npmjs.org/npm/latest" => {
                    Ok(Some(br#"{"version":"12.2.2"}"#.to_vec()))
                }
                "https://pypi.org/pypi/gone/json" => Ok(None),
                _ => Err("timed out".into()),
            }
        };
        let packages = [
            package(Registry::Npm, "npm"),
            package(Registry::PyPI, "gone"),
            package(Registry::CratesIo, "slow"),
        ];
        let answers = check(&packages, &fetch, &AtomicBool::new(false)).unwrap();
        assert_eq!(asked.lock().unwrap().len(), 3);
        assert_eq!(
            answers,
            vec![
                (packages[2].clone(), Answer::Failed("timed out".into())),
                (packages[0].clone(), Answer::Latest("12.2.2".into())),
                (packages[1].clone(), Answer::NotFound),
            ]
        );
    }

    #[test]
    fn a_stopped_check_keeps_nothing() {
        let fetch = |_: &str| Ok(None);
        let packages = [package(Registry::Npm, "npm")];
        let err = check(&packages, &fetch, &AtomicBool::new(true)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        let mut state = State::default();
        let notice = state.finish(Err(err), &[], SystemTime::now());
        assert_eq!(notice, "Online check stopped; nothing was kept");
        assert!(state.checked.is_empty());
    }

    #[test]
    fn only_registry_hosts_are_requested() {
        let agent = agent();
        assert_eq!(
            get(&agent, "https://example.com/x").unwrap_err(),
            "not a registry Den checks"
        );
        // A look-alike host is not a registry either.
        assert!(get(&agent, "https://pypi.org.example.com/x").is_err());
        assert!(get(&agent, "http://pypi.org/pypi/ruff/json").is_err());
    }

    #[test]
    fn newer_compares_with_the_installed_version() {
        let mut state = State::default();
        let lookup = |installed: &str| Lookup {
            package: package(Registry::CratesIo, "ripgrep"),
            installed: installed.into(),
        };
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        state.finish(
            Ok(vec![(
                package(Registry::CratesIo, "ripgrep"),
                Answer::Latest("15.2.0".into()),
            )]),
            &[],
            now,
        );
        assert_eq!(state.newer(&lookup("14.1.1")), Some("15.2.0"));
        assert_eq!(state.newer(&lookup("15.2.0")), None);
        assert_eq!(
            state.panel(&lookup("14.1.1"), now + Duration::from_secs(180)),
            "\n\n↑ 15.2.0 available on crates.io · checked online 3 min ago"
        );
        assert_eq!(
            state.panel(&lookup("15.2.0"), now),
            "\n\nUp to date on crates.io (newest 15.2.0) · checked online just now"
        );
    }
}
