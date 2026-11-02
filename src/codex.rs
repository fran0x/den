//! Local evidence only: never a claim about an active Codex session.
pub mod config;
pub mod instructions;
pub mod privacy;
pub mod state;

use crate::skills::{self, filesystem};
pub use config::{Configuration, Enablement, Trust};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const LIMITATIONS: &str = "Locally resolved configuration for the selected context. CLI overrides, cloud defaults, enforced requirements, managed policy and host/session state are not observed. Eligible never means loaded or invoked. Unknown values are redacted; unknown table/array merge semantics are not inferred.";
pub const FILE_LIMIT: usize = 256 * 1024;

#[derive(Clone, Debug)]
pub struct Context {
    pub project: PathBuf,
    pub home: Option<PathBuf>,
    pub codex_home: PathBuf,
    pub system: PathBuf,
    pub profile: Option<String>,
    pub boundary: Option<PathBuf>,
}
impl Context {
    pub fn local(project: PathBuf, profile: Option<String>) -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let codex_home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|p| p.join(".codex")))
            .ok_or_else(|| io::Error::other("HOME and CODEX_HOME unavailable"))?;
        let project = filesystem::resolve(&project)?;
        let context = Self {
            project,
            home,
            codex_home,
            system: "/etc/codex".into(),
            profile,
            boundary: None,
        };
        context.validate()?;
        Ok(context)
    }
    pub fn fixture(root: &Path, profile: Option<String>) -> io::Result<Self> {
        let root = filesystem::resolve(root)?;
        let context = Self {
            project: root.join("project"),
            home: Some(root.join("home")),
            codex_home: root.join("home/.codex"),
            system: root.join("system"),
            profile,
            boundary: Some(root),
        };
        context.validate()?;
        Ok(context)
    }
    pub(crate) fn validate(&self) -> io::Result<()> {
        if !self.project.is_dir() || self.profile.as_ref().is_some_and(|s| !identifier(s)) {
            return Err(io::Error::other(
                "project must be a directory; profile must be a short alphanumeric, dash or underscore name",
            ));
        }
        Ok(())
    }
    pub(crate) fn permits(&self, path: &Path) -> bool {
        self.boundary
            .as_ref()
            .is_none_or(|root| path.starts_with(root))
    }
}
pub fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
/// Only descriptor-relative regular files. Config/instruction symlinks are deliberately
/// not followed: a config.toml link must never become an auth.json read.
#[cfg(unix)]
pub(crate) fn read(context: &Context, path: &Path) -> io::Result<String> {
    if !context.permits(path)
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(io::Error::other("outside fixture boundary"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing parent"))?;
    let resolved_parent;
    let parent = if parent == context.system || parent == context.codex_home {
        resolved_parent = filesystem::resolve(parent)?;
        if !context.permits(&resolved_parent) {
            return Err(io::Error::other("outside fixture boundary"));
        }
        &resolved_parent
    } else {
        parent
    };
    let file = filesystem::Directory::open(parent)?.child(
        path.file_name()
            .ok_or_else(|| io::Error::other("missing filename"))?,
    )?;
    filesystem::text(file, FILE_LIMIT)
}
#[cfg(not(unix))]
pub(crate) fn read(_: &Context, _: &Path) -> io::Result<String> {
    Err(io::Error::other(
        "secure configuration reads require macOS/Linux",
    ))
}

pub struct Inventory {
    pub configuration: Configuration,
    pub instructions: instructions::Chain,
    pub conditional_instructions: Option<instructions::Chain>,
    pub skills: skills::Inventory,
    pub skills_config: skills::Config,
    pub enablement: Vec<Enablement>,
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

pub fn discover(context: &Context, cancel: &AtomicBool) -> io::Result<Inventory> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("collection cancelled"));
    }
    let mut configuration = config::collect(context, cancel)?;
    let instructions = instructions::collect(
        context,
        &configuration.directories,
        &configuration.controls(false),
        cancel,
    )?;
    let conditional_instructions = (configuration.trust == Trust::Unknown)
        .then(|| {
            instructions::collect(
                context,
                &configuration.directories,
                &configuration.controls(true),
                cancel,
            )
        })
        .transpose()?;
    // Keep existing collection, changing only project roots to the modeled root markers.
    let mut skills_config = if context.boundary.is_some() {
        let mut roots = vec![
            skills::Root {
                path: context.system.join("skills"),
                scope: skills::Scope::Admin,
            },
            skills::Root {
                path: context.codex_home.join("skills/.system"),
                scope: skills::Scope::System,
            },
            skills::Root {
                path: context.codex_home.join("skills"),
                scope: skills::Scope::Legacy,
            },
        ];
        if let Some(home) = &context.home {
            roots.push(skills::Root {
                path: home.join(".agents/skills"),
                scope: skills::Scope::User,
            });
        }
        skills::Config {
            roots,
            project: context.project.clone(),
            coverage: vec![
                "Explicit Codex fixture boundary; no real HOME, admin or ancestor discovery".into(),
            ],
        }
    } else {
        skills::Config::for_context(
            context.project.clone(),
            context.home.clone(),
            Some(context.codex_home.clone()),
            Some(context.system.join("skills")),
        )?
    };
    skills_config
        .roots
        .retain(|r| r.scope != skills::Scope::Project);
    for directory in &configuration.directories {
        skills_config.roots.push(skills::Root {
            path: directory.join(".agents/skills"),
            scope: skills::Scope::Project,
        });
    }
    let mut skills = skills::discover_bounded(&skills_config, cancel, context.boundary.as_deref())?;
    let unresolved = configuration.unresolved_skills(&skills);
    configuration.findings.push(format!(
        "Skill references: {unresolved} unresolved; no undiscovered skills are invented"
    ));
    skills_config
        .coverage
        .retain(|s| !s.contains("Configuration status"));
    skills_config.coverage.push(LIMITATIONS.into());
    skills.coverage = skills_config.coverage.clone();
    let enablement = configuration.correlate(&skills);
    Ok(Inventory {
        configuration,
        instructions,
        conditional_instructions,
        skills,
        skills_config,
        enablement,
    })
}
