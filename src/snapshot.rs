//! What changed: each area's rows reduced to entries that can be compared later.
//!
//! An entry holds only what the list shows for a row (its section, key, name and one
//! short value), never the details panel, so a snapshot cannot hold anything Den
//! withholds. Values leave out what changes on its own: whether a container is running,
//! and newer versions Homebrew knows of.
use crate::{
    areas::{
        agents::{self, Sources},
        cloud::{aws, kube},
        docker, toolchains,
    },
    inventory::{self, ResourceKind},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// One row as a snapshot keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub section: String,
    pub key: String,
    pub name: String,
    /// What is compared: a version, a status, an image.
    pub value: String,
}
impl Entry {
    fn new(section: &str, key: impl Into<String>, name: &str, value: impl Into<String>) -> Self {
        Self {
            section: section.into(),
            key: key.into(),
            name: name.into(),
            value: value.into(),
        }
    }
}

/// How a row differs from the snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    Added,
    /// The value it had in the snapshot.
    Changed {
        before: String,
    },
}

/// The difference between a snapshot and now, for one area.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Rows present now, by section and key.
    current: BTreeMap<(String, String), Change>,
    /// Rows in the snapshot that are gone, in snapshot order.
    removed: Vec<Entry>,
}
impl Changes {
    pub fn of(&self, section: &str, key: &str) -> Option<&Change> {
        self.current.get(&(section.to_owned(), key.to_owned()))
    }
    pub fn removed<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.removed.iter().filter(move |e| e.section == section)
    }
    /// Added, changed and removed rows together.
    pub fn len(&self) -> usize {
        self.current.len() + self.removed.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Added, changed and removed rows in one section.
    pub fn in_section(&self, section: &str) -> usize {
        self.current.keys().filter(|(s, _)| s == section).count() + self.removed(section).count()
    }
}

/// The saved snapshot's format; a file from a newer Den is not read.
pub const FILE_VERSION: u32 = 1;
/// A snapshot larger than this is not read.
const MAX_BYTES: u64 = 4 << 20;

/// The snapshot file: each area's entries when `S` last saved it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub version: u32,
    /// By area: Homebrew, Agents, AWS, Kubernetes, Docker, Toolchains.
    pub areas: BTreeMap<String, Saved>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    /// Seconds since 1970, UTC.
    pub taken: u64,
    pub entries: Vec<Entry>,
}
impl Saved {
    pub fn new(taken: SystemTime, entries: Vec<Entry>) -> Self {
        let taken = taken
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        Self { taken, entries }
    }
    /// When it was taken, and what it held.
    pub fn point(&self) -> (SystemTime, Vec<Entry>) {
        (
            SystemTime::UNIX_EPOCH + Duration::from_secs(self.taken),
            self.entries.clone(),
        )
    }
}

/// `$XDG_STATE_HOME/den/snapshot.json`, or `~/.local/state/den/snapshot.json`.
pub fn path() -> Option<PathBuf> {
    crate::actions::state_dir().map(|dir| dir.join("snapshot.json"))
}

/// Reads a snapshot file. A file from a newer Den, or one that is not a Den snapshot, is
/// an error that says so.
pub fn parse(bytes: &[u8]) -> Result<File, String> {
    #[derive(Deserialize)]
    struct Head {
        version: u32,
    }
    let head: Head =
        serde_json::from_slice(bytes).map_err(|_| "it is not a Den snapshot".to_owned())?;
    if head.version > FILE_VERSION {
        return Err("it was saved by a newer Den".into());
    }
    if head.version == 0 {
        return Err("it is not a Den snapshot".into());
    }
    serde_json::from_slice(bytes).map_err(|_| "it is not a Den snapshot".to_owned())
}

/// The saved snapshot: none when there is no file. A link, something other than a file,
/// a file over 4 MiB or an unreadable one is an error that says why; Den never changes
/// or removes it on its own.
pub fn load(path: &Path) -> Result<Option<File>, String> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
    };
    #[cfg(not(unix))]
    let file = std::fs::File::open(path);
    let file = match file {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) => {
            return Err("it is a link; Den does not follow it".into());
        }
        Err(e) => return Err(crate::inventory::safe_text(&e.to_string())),
    };
    let meta = file
        .metadata()
        .map_err(|e| crate::inventory::safe_text(&e.to_string()))?;
    if !meta.is_file() {
        return Err("it is not a file".into());
    }
    if meta.len() > MAX_BYTES {
        return Err("it is larger than 4 MiB".into());
    }
    let mut bytes = vec![];
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| crate::inventory::safe_text(&e.to_string()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("it is larger than 4 MiB".into());
    }
    parse(&bytes).map(Some)
}

/// The file's text: indented JSON, so it reads and diffs well.
pub fn encode(file: &File) -> std::io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(file)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Which rows a list shows: all of them, or only what changed since the saved snapshot
/// or since Den started. `d` cycles through them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    All,
    SinceSnapshot,
    SinceStart,
}

/// What changed in one list: since the snapshot saved with `S`, and since Den first read
/// it this session.
#[derive(Clone, Debug, Default)]
pub struct Tracking {
    start: Option<(SystemTime, Vec<Entry>)>,
    saved: Option<(SystemTime, Vec<Entry>)>,
    /// The latest reading, which `S` saves.
    last: Option<Vec<Entry>>,
    since_start: Changes,
    since_saved: Changes,
    pub view: View,
}
impl Tracking {
    /// A fresh reading: the first one is the starting point, later ones are compared to it.
    pub fn observe(&mut self, entries: Vec<Entry>, now: SystemTime) {
        if self.start.is_none() {
            self.start = Some((now, entries.clone()));
        }
        self.last = Some(entries);
        self.compare();
    }
    /// The snapshot this list is compared with, from the saved file.
    pub fn use_saved(&mut self, saved: Option<(SystemTime, Vec<Entry>)>) {
        self.saved = saved;
        if self.saved.is_none() && self.view == View::SinceSnapshot {
            self.view = View::All;
        }
        self.compare();
    }
    /// The latest reading, once there is one.
    pub fn last(&self) -> Option<&[Entry]> {
        self.last.as_deref()
    }
    /// The latest reading was just saved: it is now the snapshot.
    pub fn saved_now(&mut self, now: SystemTime) {
        if let Some(last) = &self.last {
            self.saved = Some((now, last.clone()));
            self.compare();
        }
    }
    pub fn has_snapshot(&self) -> bool {
        self.saved.is_some()
    }
    fn compare(&mut self) {
        let Some(last) = &self.last else {
            return;
        };
        let against = |point: &Option<(SystemTime, Vec<Entry>)>| {
            point
                .as_ref()
                .map(|(_, before)| diff(before, last))
                .unwrap_or_default()
        };
        self.since_start = against(&self.start);
        self.since_saved = against(&self.saved);
    }
    /// `d`: all rows → since the snapshot (when there is one) → since Den started → all.
    pub fn next_view(&mut self) {
        self.view = match self.view {
            View::All if self.saved.is_some() => View::SinceSnapshot,
            View::All | View::SinceSnapshot => View::SinceStart,
            View::SinceStart => View::All,
        };
    }
    /// Only changed rows are shown.
    pub fn only(&self) -> bool {
        self.view != View::All
    }
    /// The changes marks and the panel follow: the view's, or with every row shown, the
    /// session's.
    pub fn changes(&self) -> &Changes {
        match self.view {
            View::SinceSnapshot => &self.since_saved,
            View::All | View::SinceStart => &self.since_start,
        }
    }
    /// Anything to show with `d`: changes either way, or a snapshot to compare with.
    pub fn any(&self) -> bool {
        self.only()
            || self.saved.is_some()
            || !self.since_start.is_empty()
            || !self.since_saved.is_empty()
    }
    /// Whether the list shows this row: every row, or in a changes view only changed ones.
    pub fn keeps(&self, section: &str, key: &str) -> bool {
        !self.only() || self.changes().of(section, key).is_some()
    }
    /// "+ " before a new row's name, "~ " before a changed one.
    pub fn mark(&self, section: &str, key: &str) -> &'static str {
        match self.changes().of(section, key) {
            Some(Change::Added) => "+ ",
            Some(Change::Changed { .. }) => "~ ",
            None => "",
        }
    }
    /// Removed rows, listed after the others in a changes view.
    pub fn removed<'a>(&'a self, section: &'a str) -> Vec<&'a Entry> {
        if self.only() {
            self.changes().removed(section).collect()
        } else {
            vec![]
        }
    }
    /// "since Den started 5 min ago", or "since the snapshot 2 days ago".
    pub fn since(&self, now: SystemTime) -> String {
        let (what, point) = match self.view {
            View::SinceSnapshot => ("since the snapshot", &self.saved),
            View::All | View::SinceStart => ("since Den started", &self.start),
        };
        point.as_ref().map_or_else(
            || what.into(),
            |(when, _)| format!("{what} {}", crate::updates::age(*when, now)),
        )
    }
    /// The filter line in a changes view: "Changes 3 · since Den started 5 min ago".
    pub fn filter(&self, section: &str, now: SystemTime) -> Option<String> {
        self.only().then(|| {
            format!(
                "Changes {} · {}",
                self.changes().in_section(section),
                self.since(now)
            )
        })
    }
    /// The status line's counts: " · 3 changes since Den started · 12 since the
    /// snapshot", or nothing.
    pub fn status(&self) -> String {
        let mut text = String::new();
        if !self.since_start.is_empty() {
            text.push_str(&format!(
                " · {} since Den started",
                crate::inventory::count(self.since_start.len(), "change")
            ));
        }
        if self.saved.is_some() && !self.since_saved.is_empty() {
            text.push_str(&format!(
                " · {} since the snapshot",
                crate::inventory::count(self.since_saved.len(), "change")
            ));
        }
        text
    }
    /// The panel lines for a row that changed, or nothing.
    pub fn panel(&self, section: &str, key: &str, now: SystemTime) -> String {
        let since = self.since(now);
        match self.changes().of(section, key) {
            Some(Change::Added) => format!("\n\n{HEADING}What changed\nNew {since}."),
            Some(Change::Changed { before }) => format!(
                "\n\n{HEADING}What changed\nChanged {since}. Before: {}",
                crate::inventory::safe_text(before)
            ),
            None => String::new(),
        }
    }
}
/// Panel section headings, as the list areas mark them.
const HEADING: &str = "## ";

/// Compares two lists of entries by section and key. A key repeated within a section
/// counts once, by its last entry.
pub fn diff(before: &[Entry], now: &[Entry]) -> Changes {
    let index = |entries: &[Entry]| -> BTreeMap<(String, String), usize> {
        entries
            .iter()
            .enumerate()
            .map(|(n, e)| ((e.section.clone(), e.key.clone()), n))
            .collect()
    };
    let (old, new) = (index(before), index(now));
    let mut changes = Changes::default();
    for (id, &n) in &new {
        match old.get(id) {
            None => {
                changes.current.insert(id.clone(), Change::Added);
            }
            Some(&o) if before[o].value != now[n].value => {
                changes.current.insert(
                    id.clone(),
                    Change::Changed {
                        before: before[o].value.clone(),
                    },
                );
            }
            Some(_) => {}
        }
    }
    let mut gone: Vec<usize> = old
        .iter()
        .filter(|(id, _)| !new.contains_key(*id))
        .map(|(_, &n)| n)
        .collect();
    gone.sort_unstable();
    changes.removed = gone.into_iter().map(|n| before[n].clone()).collect();
    changes
}

/// A Homebrew package's key: `formula:git`, `cask:firefox`.
pub fn homebrew_key(r: &inventory::Resource) -> String {
    let kind = if r.kind == ResourceKind::Cask {
        "cask"
    } else {
        "formula"
    };
    format!("{kind}:{}", r.name)
}

/// Homebrew: each installed formula and cask, with its installed version.
pub fn homebrew(inventory: &inventory::Inventory) -> Vec<Entry> {
    inventory
        .resources
        .iter()
        .filter(|r| matches!(r.kind, ResourceKind::Formula | ResourceKind::Cask))
        .map(|r| {
            Entry::new(
                "Packages",
                homebrew_key(r),
                &r.display_name(),
                r.version.clone().unwrap_or_default(),
            )
        })
        .collect()
}

/// Agents: every section, with the Tools and Status cells the list shows.
pub fn agents(sources: &Sources<'_>) -> Vec<Entry> {
    // Settings as settings, whichever view is on screen.
    let sources = Sources {
        codex_sources: false,
        ..*sources
    };
    agents::Section::ALL
        .into_iter()
        .flat_map(|section| {
            let sources = &sources;
            sources.rows(section).into_iter().map(move |row| {
                let (tools, status) = sources.cells(section, &row);
                Entry::new(
                    section.label(),
                    row.key.clone(),
                    &row.name,
                    format!("{tools} · {status}"),
                )
            })
        })
        .collect()
}

/// AWS: profiles and SSO sessions, with how they sign in and their status.
pub fn aws(inventory: &aws::Inventory) -> Vec<Entry> {
    inventory
        .rows
        .iter()
        .map(|r| Entry::new("AWS", &r.key, &r.name, format!("{} · {}", r.auth, r.status)))
        .collect()
}

/// Kubernetes: contexts, with how they sign in and their status.
pub fn kube(inventory: &kube::Inventory) -> Vec<Entry> {
    inventory
        .rows
        .iter()
        .map(|r| {
            Entry::new(
                "Kubernetes",
                &r.key,
                &r.name,
                format!("{} · {}", r.auth, r.status),
            )
        })
        .collect()
}

/// Docker: containers by name with their image (running or not is not a change),
/// images by name with their size, volumes with their driver.
pub fn docker(inventory: &docker::Inventory) -> Vec<Entry> {
    let containers = inventory
        .containers
        .iter()
        .map(|r| Entry::new("Containers", &r.key, &r.name, &r.status));
    let images = inventory
        .images
        .iter()
        .map(|r| Entry::new("Images", &r.key, &r.name, &r.state));
    let volumes = inventory
        .volumes
        .iter()
        .map(|r| Entry::new("Volumes", &r.key, &r.name, &r.state));
    containers.chain(images).chain(volumes).collect()
}

/// Toolchains: PATH, Rust and Packages, with the manager and status each row shows.
pub fn toolchains(inventory: &toolchains::Inventory) -> Vec<Entry> {
    use toolchains::Section;
    [Section::Path, Section::Rust, Section::Packages]
        .into_iter()
        .flat_map(|section| {
            inventory.rows(section).iter().map(move |r| {
                Entry::new(
                    section.label(),
                    &r.key,
                    &r.name,
                    format!("{} · {}", r.kind, r.status),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(section: &str, key: &str, value: &str) -> Entry {
        Entry::new(section, key, key, value)
    }

    #[test]
    fn diff_finds_added_changed_and_removed_rows() {
        let before = [
            entry("Packages", "formula:git", "2.47.0"),
            entry("Packages", "formula:wget", "1.25.0"),
            entry("Packages", "formula:jq", "1.7.1"),
        ];
        let now = [
            entry("Packages", "formula:git", "2.48.1"),
            entry("Packages", "formula:jq", "1.7.1"),
            entry("Packages", "formula:ripgrep", "14.1.1"),
        ];
        let changes = diff(&before, &now);
        assert_eq!(
            changes.of("Packages", "formula:git"),
            Some(&Change::Changed {
                before: "2.47.0".into()
            })
        );
        assert_eq!(
            changes.of("Packages", "formula:ripgrep"),
            Some(&Change::Added)
        );
        assert_eq!(changes.of("Packages", "formula:jq"), None);
        let removed: Vec<_> = changes.removed("Packages").map(|e| &e.key).collect();
        assert_eq!(removed, ["formula:wget"]);
        assert_eq!(changes.len(), 3);
    }

    #[test]
    fn the_same_rows_are_no_change() {
        let rows = [entry("AWS", "profile:dev", "SSO · Ready")];
        assert!(diff(&rows, &rows).is_empty());
        assert!(diff(&[], &[]).is_empty());
    }

    #[test]
    fn sections_are_compared_separately() {
        // The same key in another section is another row.
        let before = [entry("Images", "x", "1 GB")];
        let now = [entry("Volumes", "x", "1 GB")];
        let changes = diff(&before, &now);
        assert_eq!(changes.of("Volumes", "x"), Some(&Change::Added));
        assert_eq!(changes.removed("Images").count(), 1);
        assert_eq!(changes.removed("Volumes").count(), 0);
    }

    #[test]
    fn removed_rows_keep_snapshot_order() {
        let before = [
            entry("Rust", "crate:zoxide", "a"),
            entry("Rust", "crate:bacon", "a"),
            entry("Rust", "crate:just", "a"),
        ];
        let changes = diff(&before, &[]);
        let removed: Vec<_> = changes.removed("Rust").map(|e| e.key.as_str()).collect();
        assert_eq!(removed, ["crate:zoxide", "crate:bacon", "crate:just"]);
    }

    fn container(name: &str, state: &str, image: &str) -> docker::Row {
        docker::Row {
            name: name.into(),
            state: state.into(),
            status: image.into(),
            detail: "SECRET_ENV=hunter2".into(),
            key: format!("container:{name}"),
            target: format!("{name}-id"),
        }
    }

    #[test]
    fn a_container_stopping_is_no_change_but_a_new_image_is() {
        let before = docker::Inventory {
            containers: vec![
                container("web", "Running", "nginx:1.27"),
                container("db", "Running", "postgres:16"),
            ],
            ..Default::default()
        };
        let now = docker::Inventory {
            containers: vec![
                container("web", "Exited", "nginx:1.27"),
                container("db", "Running", "postgres:17"),
            ],
            ..Default::default()
        };
        let changes = diff(&docker(&before), &docker(&now));
        assert_eq!(changes.of("Containers", "container:web"), None);
        assert_eq!(
            changes.of("Containers", "container:db"),
            Some(&Change::Changed {
                before: "postgres:16".into()
            })
        );
    }

    #[test]
    fn entries_never_hold_the_details_panel() {
        // The panel is where Den puts what it may show but never keeps; an entry is
        // built from the list columns only.
        let docker = docker(&docker::Inventory {
            containers: vec![container("web", "Running", "nginx")],
            ..Default::default()
        });
        let toolchains = toolchains(&toolchains::Inventory {
            packages: vec![toolchains::Row {
                homebrew: None,
                manage: None,
                class: toolchains::Class::Package,
                name: "tool".into(),
                kind: "npm".into(),
                status: crate::status::Status::ready("1.0.0"),
                detail: "registry https://token@private.example/".into(),
                key: "npm:tool".into(),
            }],
            ..Default::default()
        });
        for e in docker.iter().chain(&toolchains) {
            let text = serde_json::to_string(e).expect("an entry serializes");
            assert!(!text.contains("hunter2") && !text.contains("private.example"));
        }
        assert_eq!(toolchains[0].value, "npm · Ready · 1.0.0");
    }

    #[test]
    fn homebrew_keeps_installed_version_not_the_newer_one() {
        let json = r#"{"schema_version":1,"resources":[
          {"id":"formula:git","name":"git","kind":"formula","version":"2.47.0","path":null,
           "description":"","origin":"Homebrew","reason":"",
           "homebrew":{"type":"formula","full_name":"git","tap":null,"homepage":null,
             "linked_keg":null,"installed":[],"dependencies":{},"latest":"2.48.1","outdated":true}},
          {"id":"cask:firefox","name":"firefox","kind":"cask","version":"133.0","path":null,
           "description":"","origin":"Homebrew","reason":""}]}"#;
        let inventory: inventory::Inventory = serde_json::from_str(json).expect("fixture");
        let entries = homebrew(&inventory);
        assert_eq!(
            entries[0],
            Entry::new("Packages", "formula:git", "git", "2.47.0")
        );
        assert_eq!(
            entries[1],
            Entry::new("Packages", "cask:firefox", "[firefox]", "133.0")
        );
    }
}
