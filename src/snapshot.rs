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
use std::collections::BTreeMap;

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
}

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

/// Homebrew: each installed formula and cask, with its installed version.
pub fn homebrew(inventory: &inventory::Inventory) -> Vec<Entry> {
    inventory
        .resources
        .iter()
        .filter(|r| matches!(r.kind, ResourceKind::Formula | ResourceKind::Cask))
        .map(|r| {
            let kind = if r.kind == ResourceKind::Cask {
                "cask"
            } else {
                "formula"
            };
            Entry::new(
                "Packages",
                format!("{kind}:{}", r.name),
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
    let containers = inventory.containers.iter().map(|r| {
        Entry::new(
            "Containers",
            format!("container:{}", r.name),
            &r.name,
            &r.status,
        )
    });
    let images = inventory
        .images
        .iter()
        .map(|r| Entry::new("Images", format!("image:{}", r.target), &r.name, &r.state));
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
            key: format!("container:{name}-id"),
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
