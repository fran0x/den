//! Where a skill came from, from records already on disk: the lock file `npx skills` keeps
//! (`~/.agents/.skill-lock.json`) and the git repository that holds the skill's folder.
//! Nothing is run. Only a source's owner/repo, its kind and install date are kept, and a
//! git remote is reduced to host/owner/repo, so tokens in a URL never reach the screen.
use super::*;
use std::{collections::BTreeMap, path::Path};

const LOCK: &str = ".agents/.skill-lock.json";
const LOCK_LIMIT: usize = 1024 * 1024;
const GIT_CONFIG_LIMIT: usize = 64 * 1024;
const MAX_ANCESTORS: usize = 32;

/// A skill's origin evidence, besides the author its SKILL.md names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// The lock file's record: "GitHub vercel-labs/skills, on 2026-08-16".
    pub installed: Option<String>,
    /// For a skill installed from GitHub: what `U` asks GitHub about it.
    pub online: Option<crate::online::Lookup>,
    /// The git repository holding the skill's folder.
    pub repo: Option<Repo>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub folder: PathBuf,
    /// Its `origin` remote as host/owner/repo, such as `github.com/fran0x/dotfiles`.
    pub remote: Option<String>,
}

/// Fills in each skill's origin. `home` locates the lock file; nothing outside `boundary`
/// is read when it is set.
pub fn annotate(inventory: &mut Inventory, home: Option<&Path>, boundary: Option<&Path>) {
    let boundary = boundary.map(|b| filesystem::resolve(b).unwrap_or_else(|_| b.to_owned()));
    let inside = |p: &Path| boundary.as_deref().is_none_or(|b| p.starts_with(b));
    // Skill targets are resolved paths, so the home folder is compared resolved too.
    let home = home.and_then(|h| filesystem::resolve(h).ok());
    let home = home.as_deref();
    let shared = home
        .and_then(|h| filesystem::resolve(&h.join(".agents/skills")).ok())
        .filter(|p| inside(p));
    let lock = home.map_or_else(BTreeMap::new, |h| lock_records(&h.join(LOCK), &inside));
    let mut repos: BTreeMap<PathBuf, Option<Repo>> = BTreeMap::new();
    for skill in &mut inventory.skills {
        let Some(target) = skill.resolved.clone() else {
            continue;
        };
        // A lock record names a folder in ~/.agents/skills; a same-named skill elsewhere
        // is not that install.
        if shared
            .as_deref()
            .is_some_and(|s| target.parent() == Some(s))
            && let Some(record) = target
                .file_name()
                .and_then(|n| lock.get(&*n.to_string_lossy()))
        {
            skill.origin.installed = Some(record.text.clone());
            skill.origin.online = record.online.clone();
        }
        let key = target.clone();
        skill.origin.repo = repos
            .entry(key)
            .or_insert_with(|| repository(&target, home, &inside))
            .clone();
    }
    inventory.coverage.push(format!(
        "Origin: the skills lock file (~/{LOCK}) for skills `npx skills` installed in ~/.agents/skills, the author a SKILL.md names, and the git repository holding the folder (its origin remote shown as host/owner/repo). A skill with none of these may still have been copied from elsewhere."
    ));
}

/// One lock file record: worded for the panel, and the online lookup when it came from
/// GitHub.
struct Record {
    text: String,
    online: Option<crate::online::Lookup>,
}

/// The lock file's records by skill name.
fn lock_records(path: &Path, inside: &impl Fn(&Path) -> bool) -> BTreeMap<String, Record> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Entry {
        source: Option<String>,
        source_type: Option<String>,
        installed_at: Option<String>,
        skill_path: Option<String>,
        skill_folder_hash: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Lock {
        skills: BTreeMap<String, Entry>,
    }
    let Some(text) = filesystem::resolve(path)
        .ok()
        .filter(|p| inside(p))
        .and_then(|p| read(&p, LOCK_LIMIT))
    else {
        return BTreeMap::new();
    };
    let Ok(lock) = serde_json::from_str::<Lock>(&text) else {
        return BTreeMap::new();
    };
    lock.skills
        .into_iter()
        .take(MAX_SKILLS)
        .map(|(name, e)| {
            let kind = match e.source_type.as_deref() {
                Some("github") => "GitHub",
                Some("gitlab") => "GitLab",
                Some("git") => "a git repository",
                Some("local") => "a local folder",
                _ => "a source",
            };
            // A local folder's path, or anything odd, is not shown.
            let source = e.source.filter(|_| kind != "a local folder").and_then(|s| {
                if s.contains("://") || s.contains('@') {
                    remote(&s)
                } else {
                    Some(s).filter(|s| slug(s))
                }
            });
            let date = e
                .installed_at
                .and_then(|d| d.get(..10).map(str::to_owned))
                .filter(|d| d.bytes().all(|b| b.is_ascii_digit() || b == b'-'));
            let online = match (kind, &source, &e.skill_path, &e.skill_folder_hash) {
                ("GitHub", Some(source), Some(path), Some(hash)) => {
                    crate::online::skill(source, path, hash)
                }
                _ => None,
            };
            let mut text = kind.to_owned();
            if let Some(source) = source {
                text.push_str(&format!(" {source}"));
            }
            if let Some(date) = date {
                text.push_str(&format!(", on {date}"));
            }
            (name, Record { text, online })
        })
        .collect()
}

/// The nearest folder at or above `target` with a `.git` entry, no higher than the home
/// folder when `target` is inside it.
fn repository(target: &Path, home: Option<&Path>, inside: &impl Fn(&Path) -> bool) -> Option<Repo> {
    let ceiling = home.filter(|h| target.starts_with(h));
    for dir in target.ancestors().take(MAX_ANCESTORS) {
        if !inside(dir) || ceiling.is_some_and(|h| !dir.starts_with(h)) {
            return None;
        }
        let git = dir.join(".git");
        let Ok(meta) = std::fs::symlink_metadata(&git) else {
            continue;
        };
        // A `.git` file (a worktree or submodule) points elsewhere; its remote is not read.
        let remote = meta
            .is_dir()
            .then(|| read(&git.join("config"), GIT_CONFIG_LIMIT))
            .flatten()
            .and_then(|config| origin_url(&config))
            .and_then(|url| remote(&url));
        return Some(Repo {
            folder: dir.to_owned(),
            remote,
        });
    }
    None
}

/// The `url` of `[remote "origin"]` in a git config.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_origin = line.replace(' ', "") == "[remote\"origin\"]";
        } else if in_origin
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "url"
        {
            return Some(value.trim().to_owned());
        }
    }
    None
}

/// A remote URL as host/owner/repo, without user, password, token or port:
/// `https://x:token@github.com/fran0x/dotfiles.git` and `git@github.com:fran0x/dotfiles`
/// both give `github.com/fran0x/dotfiles`. Local paths and `file://` give `None`.
pub fn remote(url: &str) -> Option<String> {
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !matches!(scheme, "https" | "http" | "ssh" | "git") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        (host.split(':').next()?, path)
    } else {
        // scp-like: [user@]host:path
        let (authority, path) = url.split_once(':')?;
        (authority.rsplit('@').next()?, path)
    };
    let host = host.to_ascii_lowercase();
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let host_ok = !host.is_empty()
        && host.contains('.')
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    (host_ok && path.contains('/') && slug(path)).then(|| format!("{host}/{path}"))
}

/// An owner/repo-like name: short, and only letters, digits and `.` `_` `-` `/`.
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && !s.starts_with(['/', '.'])
        && !s.contains("..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}

#[cfg(unix)]
fn read(path: &Path, limit: usize) -> Option<String> {
    let dir = filesystem::Directory::open(path.parent()?).ok()?;
    let file = dir.child(path.file_name()?).ok()?;
    filesystem::text(file, limit).ok()
}
#[cfg(not(unix))]
fn read(_: &Path, _: usize) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remotes_lose_credentials_and_ports() {
        let ok = |u: &str| remote(u).unwrap_or_default();
        assert_eq!(
            ok("https://github.com/fran0x/dotfiles.git"),
            "github.com/fran0x/dotfiles"
        );
        assert_eq!(
            ok("git@github.com:fran0x/dotfiles.git"),
            "github.com/fran0x/dotfiles"
        );
        assert_eq!(
            ok("https://fran0x:ghp_secret@github.com/fran0x/dotfiles"),
            "github.com/fran0x/dotfiles"
        );
        assert_eq!(ok("ssh://git@gitlab.com:2222/a/b.git/"), "gitlab.com/a/b");
        for bad in [
            "/Users/x/repo",
            "file:///tmp/x",
            "../x",
            "https://github.com/",
            "c:/x",
        ] {
            assert_eq!(remote(bad), None, "{bad}");
        }
    }

    #[test]
    fn only_the_origin_remote_url_is_taken() {
        let config = "[core]\n\turl = nope\n[remote \"upstream\"]\n\turl = https://github.com/a/b\n[remote \"origin\"]\n\tfetch = +refs/heads/*\n\turl = git@github.com:fran0x/dotfiles.git\n";
        assert_eq!(
            origin_url(config).as_deref(),
            Some("git@github.com:fran0x/dotfiles.git")
        );
        assert_eq!(origin_url("[remote \"upstream\"]\nurl = x\n"), None);
    }
}
