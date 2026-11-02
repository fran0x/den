use super::*;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct Root {
    pub path: PathBuf,
    pub scope: Scope,
}
#[derive(Debug, Clone)]
pub struct Config {
    pub roots: Vec<Root>,
    pub project: PathBuf,
    pub coverage: Vec<String>,
}
impl Config {
    pub fn local(project: PathBuf) -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let codex = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".codex")));
        Self::for_context(
            project,
            home,
            codex,
            Some(PathBuf::from("/etc/codex/skills")),
        )
    }
    pub fn for_context(
        project: PathBuf,
        home: Option<PathBuf>,
        codex: Option<PathBuf>,
        admin: Option<PathBuf>,
    ) -> io::Result<Self> {
        let project = filesystem::resolve(&project)?;
        if !project.is_dir() {
            return Err(io::Error::other("project must be a directory"));
        }
        let mut roots = vec![];
        let mut coverage=vec![
            "Discovered filesystem skills, not enabled/loaded/invoked skills. Configuration status not evaluated by standalone discovery.".into(),
            "Plugin and session-provided skills are not enumerated. System cache is only partial evidence of bundled skills.".into(),
            "Immediate non-hidden skill folders only (plus explicit .system root). Supporting traversal: depth 8, 512 entries/skill, 20000 total entries, 1024 skills, 32 MiB text, 30s cooperative deadline. Blocking filesystem calls cannot be forcibly cancelled.".into(),
            "SKILL.md limit 1 MiB; supporting previews 256 KiB; YAML 64 KiB with parser budgets; text previews at most 20000 lines. Directory links inside a skill are not traversed.".into(),
        ];
        let ancestors: Vec<_> = project.ancestors().take(64).collect();
        let boundary = ancestors
            .iter()
            .position(|p| fs::symlink_metadata(p.join(".git")).is_ok());
        let count = boundary.map_or(1, |n| n + 1);
        if boundary.is_none() {
            coverage.push("No Git root identified within 64 ancestors; only selected directory project skills inspected.".into());
        }
        for path in ancestors.into_iter().take(count) {
            roots.push(Root {
                path: path.join(".agents/skills"),
                scope: Scope::Project,
            });
        }
        if let Some(home) = home {
            roots.push(Root {
                path: home.join(".agents/skills"),
                scope: Scope::User,
            });
        } else {
            coverage.push("HOME unavailable: user skills location unresolved.".into());
        }
        if let Some(path) = admin {
            roots.push(Root {
                path,
                scope: Scope::Admin,
            });
        }
        if let Some(codex) = codex {
            roots.push(Root {
                path: codex.join("skills/.system"),
                scope: Scope::System,
            });
            roots.push(Root {
                path: codex.join("skills"),
                scope: Scope::Legacy,
            });
            coverage.push("CODEX_HOME/skills and .system are compatibility/cache candidates; current documentation does not establish their activation. Other .codex paths and plugin caches are not scanned.".into());
        }
        Ok(Self {
            roots,
            project,
            coverage,
        })
    }
    /// Explicit, isolated fixture layout. No HOME, CODEX_HOME, admin or ancestor discovery.
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let root = filesystem::resolve(root)?;
        if !root.is_dir() {
            return Err(io::Error::other("fixture must be a directory"));
        }
        Ok(Self{project:root.clone(),roots:vec![Root{path:root.join("user"),scope:Scope::User},Root{path:root.join("project"),scope:Scope::Project},Root{path:root.join("admin"),scope:Scope::Admin},Root{path:root.join("system"),scope:Scope::System}],coverage:vec!["Explicit fixture discovery only; no real HOME, project ancestors, CODEX_HOME, admin or Homebrew access.".into(),"Configuration status not evaluated; discovered does not mean enabled.".into()]})
    }
}
#[cfg(not(unix))]
pub fn discover(_: &Config, _: &AtomicBool) -> io::Result<Inventory> {
    Err(io::Error::other(
        "secure skills collection currently requires macOS/Linux",
    ))
}
#[cfg(unix)]
pub fn discover(config: &Config, cancel: &AtomicBool) -> io::Result<Inventory> {
    discover_bounded(config, cancel, None)
}
#[cfg(not(unix))]
pub(crate) fn discover_bounded(
    config: &Config,
    cancel: &AtomicBool,
    _: Option<&Path>,
) -> io::Result<Inventory> {
    discover(config, cancel)
}
#[cfg(unix)]
pub(crate) fn discover_bounded(
    config: &Config,
    cancel: &AtomicBool,
    boundary: Option<&Path>,
) -> io::Result<Inventory> {
    use filesystem::Directory;
    let mut inventory = Inventory {
        coverage: config.coverage.clone(),
        ..Inventory::default()
    };
    let mut budget = Budget {
        cancel,
        start: Instant::now(),
        entries: 0,
        bytes: 0,
    };
    let mut seen = HashSet::new();
    let mut location_errors = 0;
    let mut accessible_roots = 0;
    for root in config.roots.iter().take(68) {
        budget.check()?;
        let resolved = match filesystem::resolve(&root.path) {
            Ok(path) if boundary.is_some_and(|b| !path.starts_with(b)) => {
                inventory.partial = true;
                inventory.locations.push(Location {
                    path: root.path.clone(),
                    scope: root.scope,
                    status: "Fixture boundary: external root not read".into(),
                });
                continue;
            }
            Ok(path) => path,
            Err(error) => {
                let broken = fs::symlink_metadata(&root.path).is_ok();
                if error.kind() != io::ErrorKind::NotFound || broken {
                    location_errors += 1;
                    inventory.partial = true;
                }
                inventory.locations.push(Location {
                    path: root.path.clone(),
                    scope: root.scope,
                    status: format!(
                        "{} ({:?})",
                        if error.kind() == io::ErrorKind::NotFound && !broken {
                            "Absent"
                        } else if broken {
                            "Broken/unresolved location link"
                        } else {
                            "Inaccessible/unresolved location"
                        },
                        error.kind()
                    ),
                });
                continue;
            }
        };
        if !seen.insert((root.scope, resolved.clone())) {
            continue;
        }
        let directory = match Directory::open(&resolved) {
            Ok(d) => d,
            Err(e) => {
                location_errors += 1;
                inventory.partial = true;
                inventory.locations.push(Location {
                    path: root.path.clone(),
                    scope: root.scope,
                    status: format!("Inaccessible/unexpected directory ({:?})", e.kind()),
                });
                continue;
            }
        };
        let (names, limited) = match directory.names(MAX_SKILLS + 1) {
            Ok(v) => v,
            Err(e) => {
                location_errors += 1;
                inventory.partial = true;
                inventory.locations.push(Location {
                    path: root.path.clone(),
                    scope: root.scope,
                    status: format!("Unreadable location ({:?})", e.kind()),
                });
                continue;
            }
        };
        accessible_roots += 1;
        inventory.locations.push(Location {
            path: root.path.clone(),
            scope: root.scope,
            status: if resolved != root.path {
                format!("Accessible; resolved root: {}", resolved.display())
            } else {
                "Accessible".into()
            },
        });
        inventory.partial |= limited;
        for name in names {
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            budget.check()?;
            if inventory.skills.len() >= MAX_SKILLS || budget.entries >= MAX_ENTRIES {
                inventory.partial = true;
                break;
            }
            let path = root.path.join(&name);
            let mut skill = Skill {
                path: path.clone(),
                resolved: None,
                scope: root.scope,
                name: name.to_string_lossy().into_owned(),
                description: String::new(),
                declared_name: None,
                metadata: Validity::NotEvaluated,
                accessible: false,
                incomplete: false,
                duplicate: false,
                instructions: String::new(),
                files: vec![],
                findings: vec![],
                mcp: vec![],
                optional: Validity::NotEvaluated,
            };
            match filesystem::resolve(&path) {
                Err(e) => skill.finding(
                    Severity::Error,
                    format!(
                        "Broken symlink, inaccessible path, or resolution limit ({:?})",
                        e.kind()
                    ),
                ),
                Ok(target) if boundary.is_some_and(|b| !target.starts_with(b)) => {
                    skill.finding(
                        Severity::Warning,
                        "Fixture boundary: external skill directory not read",
                    );
                }
                Ok(target) => {
                    skill.resolved = Some(target.clone());
                    if !target.starts_with(&resolved) {
                        skill.finding(Severity::Info,"External skill-folder symlink: scanning only its resolved target, with the same limits");
                    }
                    match Directory::open(&target) {
                        Err(e) => skill.finding(
                            Severity::Error,
                            format!(
                                "Unexpected structure or inaccessible skill directory ({:?})",
                                e.kind()
                            ),
                        ),
                        Ok(dir) => {
                            skill.accessible = true;
                            // Establish a skill boundary before traversing supporting files.
                            // A folder link to an unrelated directory without SKILL.md is never crawled.
                            let file = dir.child(std::ffi::OsStr::new("SKILL.md")).or_else(|_| {
                                let actual = filesystem::resolve(&target.join("SKILL.md"))?;
                                dir.relative(actual.strip_prefix(&target).map_err(|_| {
                                    io::Error::other("external SKILL.md link blocked")
                                })?)
                            });
                            match file {
                                Ok(file) => match filesystem::text(
                                    file,
                                    MAX_INSTRUCTIONS.min(MAX_TOTAL.saturating_sub(budget.bytes)),
                                ) {
                                    Ok(text) => {
                                        budget.bytes += text.len();
                                        skill.files.push(SupportingFile {
                                            path: PathBuf::from("SKILL.md"),
                                            kind: "Instructions".into(),
                                            executable: false,
                                            text: Some(text.clone()),
                                            note: String::new(),
                                        });
                                        metadata::instructions(&mut skill, text);
                                        walk(
                                            &dir,
                                            &dir,
                                            &target,
                                            Path::new(""),
                                            0,
                                            &mut budget,
                                            &mut skill,
                                        )?;
                                    }
                                    Err(error) => {
                                        skill.incomplete = true;
                                        skill.finding(
                                            Severity::Error,
                                            format!("SKILL.md unreadable: {error}"),
                                        );
                                        skill.files.push(SupportingFile {
                                            path: PathBuf::from("SKILL.md"),
                                            kind: "Instructions".into(),
                                            executable: false,
                                            text: None,
                                            note: error.to_string(),
                                        });
                                    }
                                },
                                Err(error) => {
                                    if error.kind() == io::ErrorKind::NotFound
                                        && fs::symlink_metadata(target.join("SKILL.md")).is_err()
                                    {
                                        skill.metadata = Validity::Invalid;
                                        skill.finding(
                                            Severity::Error,
                                            "Missing SKILL.md; supporting directory not traversed",
                                        );
                                    } else {
                                        skill.finding(Severity::Error,format!("SKILL.md inaccessible, broken symlink or outside skill boundary ({:?})",error.kind()));
                                    }
                                }
                            }
                            if skill.files.len() >= MAX_FILES {
                                inventory.partial = true;
                                skill.finding(
                                    Severity::Warning,
                                    "Supporting-file traversal limit reached; coverage incomplete",
                                );
                            }
                            if let Some(file) = skill
                                .files
                                .iter()
                                .find(|f| f.path == Path::new("agents/openai.yaml"))
                            {
                                if let Some(text) = file.text.clone() {
                                    metadata::optional(&mut skill, &text);
                                } else {
                                    skill.optional = Validity::Invalid;
                                    skill.finding(
                                        Severity::Warning,
                                        "agents/openai.yaml could not be read",
                                    );
                                }
                            }
                            metadata::references(&mut skill);
                        }
                    }
                }
            }
            inventory.partial |= skill.incomplete;
            inventory.skills.push(skill);
        }
    }
    inventory.partial |=
        budget.entries >= MAX_ENTRIES || budget.bytes >= MAX_TOTAL || config.roots.len() > 68;
    if accessible_roots == 0 && location_errors > 0 {
        return Err(io::Error::other(
            "all available skill roots failed; prior inventory retained",
        ));
    }
    let mut duplicates = BTreeMap::<String, Vec<usize>>::new();
    for (i, skill) in inventory.skills.iter().enumerate() {
        if let Some(name) = &skill.declared_name {
            duplicates.entry(name.clone()).or_default().push(i);
        }
    }
    for indexes in duplicates.values().filter(|v| v.len() > 1) {
        for &i in indexes {
            inventory.skills[i].duplicate = true;
            inventory.skills[i].finding(
                Severity::Info,
                "Duplicate declared name; Codex may show both. No conflict or precedence inferred.",
            );
        }
    }
    inventory
        .skills
        .sort_by(|a, b| (&a.name, &a.scope, &a.path).cmp(&(&b.name, &b.scope, &b.path)));
    Ok(inventory)
}
struct Budget<'a> {
    cancel: &'a AtomicBool,
    start: Instant,
    entries: usize,
    bytes: usize,
}
impl Budget<'_> {
    fn check(&self) -> io::Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(io::Error::other("skills collection cancelled"))
        } else if self.start.elapsed() > Duration::from_secs(30) {
            Err(io::Error::other("skills collection deadline exceeded"))
        } else {
            Ok(())
        }
    }
}
#[cfg(unix)]
fn walk(
    root: &filesystem::Directory,
    dir: &filesystem::Directory,
    base: &Path,
    relative: &Path,
    depth: usize,
    budget: &mut Budget<'_>,
    skill: &mut Skill,
) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let (names, limited) = match dir.names(MAX_FILES.saturating_sub(skill.files.len())) {
        Ok(v) => v,
        Err(e) => {
            skill.incomplete = true;
            skill.finding(
                Severity::Warning,
                format!(
                    "Unreadable directory {} ({:?})",
                    relative.display(),
                    e.kind()
                ),
            );
            return Ok(());
        }
    };
    if limited {
        skill.incomplete = true;
        skill.finding(
            Severity::Warning,
            "Directory entry limit reached; some files were not inspected",
        );
    }
    for name in names {
        budget.check()?;
        if skill.files.len() >= MAX_FILES || budget.entries >= MAX_ENTRIES {
            skill.incomplete = true;
            return Ok(());
        }
        budget.entries += 1;
        let path = relative.join(&name);
        if path == Path::new("SKILL.md") {
            continue;
        }
        let mut entry = SupportingFile {
            path: path.clone(),
            kind: "File".into(),
            executable: false,
            text: None,
            note: String::new(),
        };
        // Descriptor-relative opens reject symlinks. An internal file link can be
        // resolved once and re-opened under the anchored skill root; external links stay opaque.
        let opened = dir.child(&name).or_else(|original| {
            if !fs::symlink_metadata(base.join(&path)).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(original);
            }
            entry.kind = "Symlink".into();
            let target = filesystem::resolve(&base.join(&path))?;
            let relative = target
                .strip_prefix(base)
                .map_err(|_| io::Error::other("external supporting symlink not followed"))?;
            let file = root.relative(relative)?;
            if file.metadata()?.is_dir() {
                return Err(io::Error::other(
                    "supporting directory symlink not followed",
                ));
            }
            Ok(file)
        });
        match opened {
            Err(e) => {
                entry.note = format!("Not read: {e}");
                skill.finding(
                    Severity::Warning,
                    format!(
                        "{}: inaccessible, broken/external link, or unsupported structure ({:?})",
                        path.display(),
                        e.kind()
                    ),
                );
            }
            Ok(file) => {
                let meta = file.metadata()?;
                if meta.is_dir() {
                    entry.kind = "Directory".into();
                    skill.files.push(entry);
                    if depth < MAX_DEPTH {
                        walk(
                            root,
                            &filesystem::Directory(file),
                            base,
                            &path,
                            depth + 1,
                            budget,
                            skill,
                        )?;
                    } else {
                        skill.incomplete = true;
                        skill.finding(
                            Severity::Warning,
                            "Supporting directory depth limit reached",
                        );
                    }
                    continue;
                }
                entry.executable = meta.permissions().mode() & 0o111 != 0;
                if path.starts_with("scripts") {
                    entry.kind = "Script/supporting file".into();
                }
                let limit = if path == Path::new("SKILL.md") {
                    MAX_INSTRUCTIONS
                } else {
                    MAX_FILE
                };
                let remaining = MAX_TOTAL.saturating_sub(budget.bytes);
                match filesystem::text(file, limit.min(remaining)) {
                    Ok(text) => {
                        budget.bytes += text.len();
                        entry.text = Some(text);
                    }
                    Err(e) => {
                        entry.note = e.to_string();
                        skill.finding(
                            if path == Path::new("SKILL.md") {
                                Severity::Error
                            } else {
                                Severity::Info
                            },
                            format!("{}: {}", path.display(), entry.note),
                        );
                    }
                }
            }
        }
        skill.files.push(entry);
    }
    Ok(())
}
