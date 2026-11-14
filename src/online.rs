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
    /// Homebrew's formulas (homebrew/core).
    Homebrew,
    /// Homebrew's casks (homebrew/cask).
    HomebrewCask,
    CratesIo,
    Npm,
    PyPI,
    /// Docker Hub images, by `namespace/name:tag`.
    DockerHub,
    /// GitHub's container registry, `ghcr.io/owner/name:tag`.
    Ghcr,
    /// Release lines of languages and runtimes (`python`, `nodejs`, `rust`…): the newest
    /// patch of each and when its support ends.
    EndOfLife,
}
impl Registry {
    pub const ALL: [Self; 8] = [
        Self::Homebrew,
        Self::HomebrewCask,
        Self::CratesIo,
        Self::Npm,
        Self::PyPI,
        Self::DockerHub,
        Self::Ghcr,
        Self::EndOfLife,
    ];
    /// The only host Den contacts for this registry.
    pub fn host(self) -> &'static str {
        match self {
            Self::Homebrew | Self::HomebrewCask => "formulae.brew.sh",
            Self::CratesIo => "index.crates.io",
            Self::Npm => "registry.npmjs.org",
            Self::PyPI => "pypi.org",
            Self::DockerHub => "registry-1.docker.io",
            Self::Ghcr => "ghcr.io",
            Self::EndOfLife => "endoflife.date",
        }
    }
    /// Where an image registry hands out the anonymous read token it asks for.
    fn token_host(self) -> Option<&'static str> {
        match self {
            Self::DockerHub => Some("auth.docker.io"),
            Self::Ghcr => Some("ghcr.io"),
            _ => None,
        }
    }
    /// Images are compared by digest (the same tag, a newer build), not by version.
    pub fn is_images(self) -> bool {
        matches!(self, Self::DockerHub | Self::Ghcr)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Homebrew | Self::HomebrewCask => "Homebrew",
            Self::CratesIo => "crates.io",
            Self::Npm => "npm",
            Self::PyPI => "PyPI",
            Self::DockerHub => "Docker Hub",
            Self::Ghcr => "ghcr.io",
            Self::EndOfLife => "endoflife.date",
        }
    }
    /// How the results file names it; a formula and a cask may share a name.
    fn id(self) -> &'static str {
        match self {
            Self::HomebrewCask => "Homebrew cask",
            other => other.label(),
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
            // Homebrew: lowercase letters, digits and `-` `_` `.` `+` `@` (`python@3.13`,
            // `libsigc++`), starting with a letter or digit.
            Registry::Homebrew | Registry::HomebrewCask => {
                (1..=100).contains(&name.len())
                    && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && name.chars().all(|c| {
                        c.is_ascii_lowercase()
                            || c.is_ascii_digit()
                            || matches!(c, '-' | '_' | '.' | '+' | '@')
                    })
            }
            // Images: `repository:tag`, with lowercase path parts (`grafana/loki`) and
            // Docker's tag rules. Docker Hub names need a namespace (see `docker`).
            Registry::DockerHub | Registry::Ghcr => {
                name.len() <= 255
                    && name.rsplit_once(':').is_some_and(|(repo, tag)| {
                        let part = |p: &str| {
                            (1..=128).contains(&p.len())
                                && p.starts_with(|c: char| {
                                    c.is_ascii_lowercase() || c.is_ascii_digit()
                                })
                                && p.ends_with(|c: char| {
                                    c.is_ascii_lowercase() || c.is_ascii_digit()
                                })
                                && p.chars().all(|c| {
                                    c.is_ascii_lowercase()
                                        || c.is_ascii_digit()
                                        || matches!(c, '.' | '_' | '-')
                                })
                        };
                        let parts: Vec<_> = repo.split('/').collect();
                        (2..=8).contains(&parts.len())
                            && parts.iter().all(|p| part(p))
                            && (1..=128).contains(&tag.len())
                            && tag.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
                            && tag
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
                    })
            }
            // endoflife.date products: lowercase letters, digits and `-`.
            Registry::EndOfLife => {
                (1..=40).contains(&name.len())
                    && name.starts_with(|c: char| c.is_ascii_lowercase())
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            }
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
        format!("{}:{}", self.registry.id(), self.name)
    }
    /// The one URL asked for this package.
    pub fn url(&self) -> String {
        let host = self.registry.host();
        match self.registry {
            Registry::Homebrew => format!("https://{host}/api/formula/{}.json", self.name),
            Registry::HomebrewCask => format!("https://{host}/api/cask/{}.json", self.name),
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
            // The trailing `/` is the canonical address: without it, a redirect.
            Registry::EndOfLife => format!("https://{host}/api/v1/products/{}/", self.name),
            Registry::DockerHub | Registry::Ghcr => {
                let (repo, tag) = self.image();
                format!("https://{host}/v2/{repo}/manifests/{tag}")
            }
        }
    }
    /// An image's repository and tag.
    fn image(&self) -> (&str, &str) {
        self.name.rsplit_once(':').unwrap_or((&self.name, "latest"))
    }
    /// For an image: the URL of the anonymous, read-only token for its repository.
    fn token_url(&self) -> Option<String> {
        let host = self.registry.token_host()?;
        let (repo, _) = self.image();
        Some(match self.registry {
            Registry::DockerHub => format!(
                "https://{host}/token?service=registry.docker.io&scope=repository:{repo}:pull"
            ),
            _ => format!("https://{host}/token?scope=repository:{repo}:pull"),
        })
    }
}

/// The lookup for a tagged image pulled from Docker Hub or ghcr.io, given its local
/// `repo:tag` and digest. Only names with a namespace (`grafana/loki`) or a registry
/// (`ghcr.io/…`, `docker.io/…`) are looked up: a single name such as `postgres` could just
/// as well be an image built here, and its name stays on the machine. Other registries
/// are not checked.
pub fn docker(reference: &str, digest: &str) -> Option<Lookup> {
    if !is_digest(digest) {
        return None;
    }
    let (registry, name) = if let Some(rest) = reference.strip_prefix("ghcr.io/") {
        (Registry::Ghcr, rest.to_owned())
    } else if let Some(rest) = reference.strip_prefix("docker.io/") {
        let rest = if rest.split(':').next()?.contains('/') {
            rest.to_owned()
        } else {
            format!("library/{rest}")
        };
        (Registry::DockerHub, rest)
    } else {
        let first = reference.split('/').next()?;
        // `quay.io/…`, `localhost:5000/…`: another registry. No `/`: a single name.
        if !reference.contains('/') || first.contains(['.', ':']) || first == "localhost" {
            return None;
        }
        (Registry::DockerHub, reference.to_owned())
    };
    Some(Lookup {
        package: Package::new(registry, &name)?,
        installed: digest.to_owned(),
        newest: false,
    })
}

/// `sha256:` and 64 lowercase hex digits.
fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

/// A row's lookup: the package and the version installed here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lookup {
    pub package: Package,
    pub installed: String,
    /// For a release-line registry: compare with the newest release of any line (a
    /// channel such as Rust's `stable`), not the newest of the installed version's line.
    pub newest: bool,
}

/// A release line from endoflife.date, such as Python `3.12`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub line: String,
    /// Its newest patch release, such as `3.12.13`.
    pub latest: Option<String>,
    /// Support has ended.
    pub ended: bool,
    /// When support ends or ended (`YYYY-MM-DD`), when known.
    pub eol: Option<String>,
}

/// The release lines in an endoflife.date answer (`result.releases`), newest first.
pub fn releases(body: &[u8]) -> Result<Vec<Release>, String> {
    #[derive(Deserialize)]
    struct Latest {
        name: String,
    }
    #[derive(Deserialize)]
    struct Raw {
        name: String,
        #[serde(rename = "isEol", default)]
        is_eol: bool,
        #[serde(rename = "eolFrom")]
        eol_from: Option<String>,
        latest: Option<Latest>,
    }
    #[derive(Deserialize)]
    struct Product {
        releases: Vec<Raw>,
    }
    #[derive(Deserialize)]
    struct Body {
        result: Product,
    }
    let date = |d: &str| {
        d.len() == 10
            && d.chars().enumerate().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }
            })
    };
    let body: Body = serde_json::from_slice(body).map_err(|_| "unexpected answer")?;
    let found: Vec<_> = body
        .result
        .releases
        .into_iter()
        .filter(|r| crate::updates::plausible(&r.name))
        .take(200)
        .map(|r| Release {
            line: r.name,
            latest: r
                .latest
                .map(|l| l.name)
                .filter(|v| crate::updates::plausible(v)),
            ended: r.is_eol,
            eol: r.eol_from.filter(|d| date(d)),
        })
        .collect();
    if found.is_empty() {
        return Err("no release lines".into());
    }
    Ok(found)
}

/// The release line a version belongs to: the longest one it starts with (`3.12.7` →
/// `3.12`, `24.1.0` → `24`).
fn line_of<'a>(releases: &'a [Release], version: &str) -> Option<&'a Release> {
    releases
        .iter()
        .filter(|r| version == r.line || version.starts_with(&format!("{}.", r.line)))
        .max_by_key(|r| r.line.len())
}

/// What a registry said about a package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Answer {
    /// The newest release (pre-releases and yanked versions left out), or for an image the
    /// digest its tag points to now.
    Latest(String),
    /// A release-line registry's lines (endoflife.date).
    Releases(Vec<Release>),
    /// The registry has no package by that name, such as one installed from a folder.
    NotFound,
    /// The check did not get an answer: why, briefly.
    Failed(String),
}

/// The newest release in a registry's answer for one package.
pub fn latest(registry: Registry, body: &[u8]) -> Result<String, String> {
    let bad = || "unexpected answer".to_owned();
    let version = match registry {
        // Installed kegs carry the revision (`1.25.0_2`), so the newest version does too.
        Registry::Homebrew => {
            #[derive(Deserialize)]
            struct Versions {
                stable: String,
            }
            #[derive(Deserialize)]
            struct Formula {
                versions: Versions,
                #[serde(default)]
                revision: u32,
            }
            let f: Formula = serde_json::from_slice(body).map_err(|_| bad())?;
            match f.revision {
                0 => f.versions.stable,
                r => format!("{}_{r}", f.versions.stable),
            }
        }
        Registry::HomebrewCask => {
            #[derive(Deserialize)]
            struct Cask {
                version: String,
            }
            let c: Cask = serde_json::from_slice(body).map_err(|_| bad())?;
            cask_version(&c.version).to_owned()
        }
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
        // Answered by a header, in `image_digest`, or by release lines, in `releases`.
        Registry::DockerHub | Registry::Ghcr | Registry::EndOfLife => return Err(bad()),
    };
    if crate::updates::plausible(&version) {
        Ok(version)
    } else {
        Err(bad())
    }
}

/// A cask's version without the build Homebrew may add after a comma
/// (`5.11492.5,1403e2…` → `5.11492.5`), so it can be compared.
pub fn cask_version(version: &str) -> &str {
    version.split(',').next().unwrap_or(version)
}

/// The lookup for a Homebrew package from the main formula or cask tap; other taps are
/// not on formulae.brew.sh. A formula's installed version is its linked keg, or the
/// newest keg when none is linked.
pub fn homebrew(resource: &crate::inventory::Resource) -> Option<Lookup> {
    use crate::inventory::HomebrewMetadata;
    let (registry, installed) = match resource.homebrew.as_ref()? {
        HomebrewMetadata::Formula {
            tap,
            linked_keg,
            installed,
            ..
        } if tap.as_deref() == Some("homebrew/core") => (
            Registry::Homebrew,
            linked_keg.clone().or_else(|| {
                installed.iter().map(|k| k.version.clone()).reduce(|a, b| {
                    if crate::updates::is_newer(&a, &b) {
                        b
                    } else {
                        a
                    }
                })
            })?,
        ),
        HomebrewMetadata::Cask {
            tap,
            recorded_version,
            ..
        } if tap.as_deref() == Some("homebrew/cask") => (
            Registry::HomebrewCask,
            cask_version(recorded_version).to_owned(),
        ),
        _ => return None,
    };
    if !crate::updates::plausible(&installed) {
        return None;
    }
    Some(Lookup {
        package: Package::new(registry, &resource.name)?,
        installed,
        newest: false,
    })
}

/// Looks up one package.
pub type Ask<'a> = dyn Fn(&Package) -> Answer + Sync + 'a;

/// Runs [`check`] in the background.
pub(crate) type Collector = crate::worker::Worker<Vec<(Package, Answer)>, io::Error>;

/// Requests at once: enough to be quick, few enough to be polite.
const PARALLEL: usize = 4;

/// Looks up each package, a few at a time. Stopping (`cancel`) waits for the requests
/// already sent and then returns an error, so nothing partial is kept.
pub fn check(
    packages: &[Package],
    ask: &Ask<'_>,
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
                    let answer = ask(package);
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

/// Looks up one package with `agent`: a GET of its URL, or for an image an anonymous
/// token and then the digest its tag points to.
pub fn ask(agent: &ureq::Agent, package: &Package) -> Answer {
    if package.registry == Registry::EndOfLife {
        return match get(agent, &package.url()) {
            Ok(Some(body)) => match releases(&body) {
                Ok(found) => Answer::Releases(found),
                Err(why) => Answer::Failed(why),
            },
            Ok(None) => Answer::NotFound,
            Err(why) => Answer::Failed(why),
        };
    }
    let fetched = if package.registry.is_images() {
        image_digest(agent, package)
    } else {
        get(agent, &package.url())
            .and_then(|body| body.map(|b| latest(package.registry, &b)).transpose())
    };
    match fetched {
        Ok(Some(version)) => Answer::Latest(version),
        Ok(None) => Answer::NotFound,
        Err(why) => Answer::Failed(why),
    }
}

/// Only HTTPS to the registries' hosts and their token hosts.
fn allowed(url: &str) -> bool {
    Registry::ALL
        .iter()
        .flat_map(|r| [Some(r.host()), r.token_host()])
        .flatten()
        .any(|host| url.starts_with(&format!("https://{host}/")))
}

/// The manifest types a tag may point to: multi-platform indexes first.
const MANIFESTS: &str = "application/vnd.oci.image.index.v1+json, \
    application/vnd.docker.distribution.manifest.list.v2+json, \
    application/vnd.docker.distribution.manifest.v2+json, \
    application/vnd.oci.image.manifest.v1+json";

/// The digest an image's tag points to now: an anonymous read token (no saved login is
/// used), then a HEAD of the manifest, which Docker Hub does not count as a pull. `None`
/// when the registry has no such public image.
fn image_digest(agent: &ureq::Agent, package: &Package) -> Result<Option<String>, String> {
    #[derive(Deserialize)]
    struct Token {
        token: String,
    }
    let token_url = package.token_url().ok_or("not an image registry")?;
    let Some(body) = get(agent, &token_url)? else {
        return Ok(None);
    };
    let token: Token = serde_json::from_slice(&body).map_err(|_| "unexpected answer")?;
    let url = package.url();
    if !allowed(&url) {
        return Err("not a registry Den checks".into());
    }
    let response = agent
        .head(&url)
        .header("Authorization", format!("Bearer {}", token.token))
        .header("Accept", MANIFESTS)
        .call();
    match response {
        Ok(response) if response.status() == 200 => {
            let digest = response
                .headers()
                .get("docker-content-digest")
                .and_then(|v| v.to_str().ok())
                .filter(|d| is_digest(d))
                .ok_or("unexpected answer")?;
            Ok(Some(digest.to_owned()))
        }
        Ok(response) => Err(format!("HTTP {}", response.status().as_u16())),
        // A private or missing repository looks the same to an anonymous reader.
        Err(ureq::Error::StatusCode(401 | 403 | 404)) => Ok(None),
        Err(e) => Err(failure(&e)),
    }
}

/// One GET with `agent`. Only the hosts in [`Registry::host`] are asked.
pub fn get(agent: &ureq::Agent, url: &str) -> Result<Option<Vec<u8>>, String> {
    if !allowed(url) {
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
        let mut hosts: Vec<&str> = Registry::ALL
            .iter()
            .filter(|r| self.asking.iter().any(|p| p.registry == **r))
            .map(|r| r.host())
            .collect();
        hosts.dedup();
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
            Answer::Latest(d) if lookup.package.registry.is_images() => {
                (*d != lookup.installed).then_some("newer build")
            }
            Answer::Latest(v) if crate::updates::is_newer(&lookup.installed, v) => Some(v),
            Answer::Releases(found) => {
                let candidate = if lookup.newest {
                    found
                        .iter()
                        .filter_map(|r| r.latest.as_deref())
                        .reduce(|a, b| if crate::updates::is_newer(a, b) { b } else { a })
                } else {
                    line_of(found, &lookup.installed)?.latest.as_deref()
                };
                candidate.filter(|v| crate::updates::is_newer(&lookup.installed, v))
            }
            _ => None,
        }
    }
    /// The installed version's release line, when its support has ended. Not for a
    /// channel (`newest`), which moves to the newest line itself.
    pub fn ended(&self, lookup: &Lookup) -> Option<&Release> {
        match &self.answer(&lookup.package)?.answer {
            Answer::Releases(found) if !lookup.newest => {
                line_of(found, &lookup.installed).filter(|r| r.ended)
            }
            _ => None,
        }
    }
    /// True when the last check found the newest version: it is then newer information
    /// than Homebrew's own data.
    pub fn knows(&self, lookup: &Lookup) -> bool {
        self.answer(&lookup.package)
            .is_some_and(|c| matches!(c.answer, Answer::Latest(_)))
    }
    /// The panel's line about the last check, starting with a blank line; empty when
    /// the package was never checked.
    pub fn panel(&self, lookup: &Lookup, now: SystemTime) -> String {
        let Some(checked) = self.answer(&lookup.package) else {
            return String::new();
        };
        let registry = lookup.package.registry.label();
        let when = crate::updates::age(checked.when(), now);
        let short = |d: &str| {
            d.trim_start_matches("sha256:")
                .chars()
                .take(12)
                .collect::<String>()
        };
        let what = match &checked.answer {
            Answer::Latest(d) if lookup.package.registry.is_images() && *d != lookup.installed => {
                format!(
                    "↑ {} now points to a newer build on {registry} ({})",
                    lookup.package.image().1,
                    short(d)
                )
            }
            Answer::Latest(_) if lookup.package.registry.is_images() => {
                format!("Same build as on {registry}")
            }
            Answer::Latest(v) if crate::updates::is_newer(&lookup.installed, v) => {
                format!("↑ {v} available on {registry}")
            }
            Answer::Latest(v) => format!("Up to date on {registry} (newest {v})"),
            Answer::Releases(found) => releases_panel(self.newer(lookup), found, lookup),
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

/// What the panel says about a version's release line.
fn releases_panel(newer: Option<&str>, found: &[Release], lookup: &Lookup) -> String {
    let Some(line) = line_of(found, &lookup.installed) else {
        return if let Some(v) = newer {
            format!("↑ {v} is the newest release on endoflife.date")
        } else {
            format!(
                "endoflife.date lists no release line for {}",
                lookup.installed
            )
        };
    };
    let mut text = match (newer, lookup.newest) {
        (Some(v), true) => format!("↑ {v} is the newest release"),
        (Some(v), false) => format!("↑ {v} is the newest {} release", line.line),
        (None, true) => "Up to date: the newest release".to_owned(),
        (None, false) => format!("Up to date: the newest {} release", line.line),
    };
    if !lookup.newest {
        match (line.ended, &line.eol) {
            (true, Some(d)) => {
                text.push_str(&format!(" · {} reached end of life on {d}", line.line))
            }
            (true, None) => text.push_str(&format!(" · {} has reached end of life", line.line)),
            (false, Some(d)) => text.push_str(&format!(" · {} is supported until {d}", line.line)),
            (false, None) => {}
        }
    }
    text.push_str(" · from endoflife.date");
    text
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
        for ok in ["wget", "python@3.13", "libsigc++", "font-jetbrains-mono"] {
            assert!(Package::new(Registry::Homebrew, ok).is_some(), "{ok}");
        }
        for bad in ["", "Wget", "-x", "a/b", "../x", "a b", "a?b", "a%2f"] {
            assert!(Package::new(Registry::HomebrewCask, bad).is_none(), "{bad}");
        }
        // A formula and a cask with one name are kept apart.
        assert_ne!(
            package(Registry::Homebrew, "docker").key(),
            package(Registry::HomebrewCask, "docker").key()
        );
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
        assert_eq!(
            url(Registry::Homebrew, "python@3.13"),
            "https://formulae.brew.sh/api/formula/python@3.13.json"
        );
        assert_eq!(
            url(Registry::HomebrewCask, "firefox"),
            "https://formulae.brew.sh/api/cask/firefox.json"
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
        assert_eq!(
            latest(
                Registry::Homebrew,
                br#"{"name":"wget","versions":{"stable":"1.25.0","head":"HEAD"},"revision":2}"#
            )
            .unwrap(),
            "1.25.0_2"
        );
        assert_eq!(
            latest(
                Registry::Homebrew,
                br#"{"versions":{"stable":"3.5"},"revision":0}"#
            )
            .unwrap(),
            "3.5"
        );
        assert_eq!(
            latest(
                Registry::HomebrewCask,
                br#"{"token":"zoom","version":"6.4.0.12345,abc"}"#
            )
            .unwrap(),
            "6.4.0.12345"
        );
        assert!(latest(Registry::HomebrewCask, br#"{"version":"latest"}"#).is_err());
        // Anything that is not a plain version is not believed.
        assert!(latest(Registry::Npm, br#"{"version":"\u001b[31m1.0"}"#).is_err());
        assert!(latest(Registry::PyPI, b"<html>").is_err());
    }

    #[test]
    fn a_check_asks_each_package_once_and_keeps_every_answer() {
        let asked = Mutex::new(vec![]);
        let fetch = |p: &Package| {
            asked.lock().unwrap().push(p.key());
            match p.name() {
                "npm" => Answer::Latest("12.2.2".into()),
                "gone" => Answer::NotFound,
                _ => Answer::Failed("timed out".into()),
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
        let fetch = |_: &Package| Answer::NotFound;
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
        assert!(allowed("https://auth.docker.io/token?x"));
        assert!(!allowed("https://auth.docker.io.example.com/token"));
    }

    const D1: &str = "sha256:839fbb5a4942aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const D2: &str = "sha256:40865bc877a2bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn only_images_from_docker_hub_and_ghcr_with_a_namespace_are_looked_up() {
        let key = |r: &str| docker(r, D1).map(|l| l.package.key());
        assert_eq!(
            key("grafana/loki:3.5").as_deref(),
            Some("Docker Hub:grafana/loki:3.5")
        );
        assert_eq!(
            key("ghcr.io/gnzsnz/ib-gateway:stable").as_deref(),
            Some("ghcr.io:gnzsnz/ib-gateway:stable")
        );
        assert_eq!(
            key("docker.io/postgres:17").as_deref(),
            Some("Docker Hub:library/postgres:17")
        );
        // A single name may be an image built here: its name stays on the machine.
        assert_eq!(key("postgres:17-bookworm"), None);
        assert_eq!(key("freya-demo:local"), None);
        // Other registries are not checked.
        assert_eq!(key("quay.io/prometheus/node-exporter:v1"), None);
        assert_eq!(key("localhost:5000/team/app:1"), None);
        assert_eq!(key("localhost/team/app:1"), None);
        // Names Docker would not accept never reach a URL.
        assert_eq!(key("Grafana/loki:3"), None);
        assert_eq!(key("grafana/loki:-x"), None);
        assert_eq!(key("grafana/../loki:3"), None);
        assert_eq!(key("grafana/loki:3?x"), None);
        assert!(docker("grafana/loki:3", "sha256:short").is_none());
        assert!(docker("grafana/loki:3", "<none>").is_none());
    }

    #[test]
    fn image_urls_are_fixed() {
        let hub = docker("grafana/loki:3.5", D1).unwrap().package;
        assert_eq!(
            hub.url(),
            "https://registry-1.docker.io/v2/grafana/loki/manifests/3.5"
        );
        assert_eq!(
            hub.token_url().unwrap(),
            "https://auth.docker.io/token?service=registry.docker.io&scope=repository:grafana/loki:pull"
        );
        let ghcr = docker("ghcr.io/astral-sh/uv:latest", D1).unwrap().package;
        assert_eq!(
            ghcr.url(),
            "https://ghcr.io/v2/astral-sh/uv/manifests/latest"
        );
        assert_eq!(
            ghcr.token_url().unwrap(),
            "https://ghcr.io/token?scope=repository:astral-sh/uv:pull"
        );
        assert!(package(Registry::Npm, "npm").token_url().is_none());
    }

    #[test]
    fn an_image_is_newer_when_its_tag_points_elsewhere() {
        let lookup = docker("grafana/loki:3.5", D1).unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let mut state = State::default();
        state.finish(
            Ok(vec![(lookup.package.clone(), Answer::Latest(D2.into()))]),
            &[],
            now,
        );
        assert_eq!(state.newer(&lookup), Some("newer build"));
        assert_eq!(
            state.panel(&lookup, now),
            "\n\n↑ 3.5 now points to a newer build on Docker Hub (40865bc877a2) · checked online just now"
        );
        state.finish(
            Ok(vec![(lookup.package.clone(), Answer::Latest(D1.into()))]),
            &[],
            now,
        );
        assert_eq!(state.newer(&lookup), None);
        assert_eq!(
            state.panel(&lookup, now),
            "\n\nSame build as on Docker Hub · checked online just now"
        );
    }

    #[test]
    fn newer_compares_with_the_installed_version() {
        let mut state = State::default();
        let lookup = |installed: &str| Lookup {
            package: package(Registry::CratesIo, "ripgrep"),
            installed: installed.into(),
            newest: false,
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

    fn eol_answer() -> Vec<Release> {
        releases(
            br#"{"schema_version":"1.2.1","result":{"name":"python","releases":[
              {"name":"3.15","isEol":false,"eolFrom":"2031-10-31","latest":{"name":"3.15.2"}},
              {"name":"3.12","isEol":false,"eolFrom":"2028-10-31","latest":{"name":"3.12.13"}},
              {"name":"3.9","isEol":true,"eolFrom":"2025-10-31","latest":{"name":"3.9.25"}},
              {"name":"<b>","isEol":true,"eolFrom":null,"latest":null},
              {"name":"2.7","isEol":true,"eolFrom":"not a date","latest":{"name":"\u001b[31m"}}
            ]}}"#,
        )
        .unwrap()
    }

    #[test]
    fn release_lines_are_read_and_checked() {
        let found = eol_answer();
        // A line that is not a version is dropped, and so are odd versions and dates.
        assert_eq!(found.len(), 4);
        assert_eq!(found[3].line, "2.7");
        assert_eq!(found[3].latest, None);
        assert_eq!(found[3].eol, None);
        assert_eq!(found[2].eol.as_deref(), Some("2025-10-31"));
        assert!(releases(br#"{"result":{"releases":[]}}"#).is_err());
        assert!(releases(b"<html>").is_err());
        assert_eq!(
            package(Registry::EndOfLife, "nodejs").url(),
            "https://endoflife.date/api/v1/products/nodejs/"
        );
        assert!(Package::new(Registry::EndOfLife, "../x").is_none());
        assert!(Package::new(Registry::EndOfLife, "Python").is_none());
    }

    #[test]
    fn a_version_is_compared_with_its_release_line() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let mut state = State::default();
        let python = package(Registry::EndOfLife, "python");
        state.finish(
            Ok(vec![(python.clone(), Answer::Releases(eol_answer()))]),
            &[],
            now,
        );
        let lookup = |installed: &str, newest| Lookup {
            package: python.clone(),
            installed: installed.into(),
            newest,
        };
        // The newest patch of its own line, not the newest line.
        assert_eq!(state.newer(&lookup("3.12.7", false)), Some("3.12.13"));
        assert!(state.ended(&lookup("3.12.7", false)).is_none());
        assert_eq!(
            state.panel(&lookup("3.12.7", false), now),
            "\n\n↑ 3.12.13 is the newest 3.12 release · 3.12 is supported until 2028-10-31 \
             · from endoflife.date · checked online just now"
        );
        assert_eq!(state.newer(&lookup("3.12.13", false)), None);
        // A line whose support has ended.
        assert_eq!(state.ended(&lookup("3.9.25", false)).unwrap().line, "3.9");
        assert_eq!(
            state.panel(&lookup("3.9.25", false), now),
            "\n\nUp to date: the newest 3.9 release · 3.9 reached end of life on 2025-10-31 \
             · from endoflife.date · checked online just now"
        );
        // Lines match whole parts only: 3.1.4 is not in the 3.15 line.
        assert_eq!(state.newer(&lookup("3.1.4", false)), None);
        assert_eq!(
            state.panel(&lookup("3.1.4", false), now),
            "\n\nendoflife.date lists no release line for 3.1.4 · checked online just now"
        );
        // A channel follows the newest line, and its line's end of life is not news.
        assert_eq!(state.newer(&lookup("3.9.25", true)), Some("3.15.2"));
        assert!(state.ended(&lookup("3.9.25", true)).is_none());
        assert_eq!(
            state.panel(&lookup("3.9.25", true), now),
            "\n\n↑ 3.15.2 is the newest release · from endoflife.date · checked online just now"
        );
    }
}
