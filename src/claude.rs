//! Claude Code skills and instruction files, read as local evidence only. Nothing is
//! executed, and nothing here claims what an active Claude Code session loaded.
pub mod config;
pub mod state;

use crate::{
    codex::privacy,
    skills::{self, filesystem},
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub const FILE_LIMIT: usize = 256 * 1024;
const PREVIEW_BUDGET: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Context {
    pub project: PathBuf,
    pub home: Option<PathBuf>,
    /// `CLAUDE_CONFIG_DIR`, or `~/.claude`.
    pub claude_home: Option<PathBuf>,
    /// The organisation-managed folder (CLAUDE.md, managed-settings.json, managed-mcp.json),
    /// if this platform has a known location.
    pub managed: Option<PathBuf>,
    /// `~/.claude.json`: only its MCP servers, MCP approvals and project trust are read.
    pub claude_json: Option<PathBuf>,
    /// `CLAUDE_CODE_PLUGIN_CACHE_DIR`, or the `plugins` folder in the Claude home.
    pub plugins: Option<PathBuf>,
    /// Fixture root: nothing outside it is read.
    pub boundary: Option<PathBuf>,
}
impl Context {
    pub fn local(project: PathBuf) -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let claude_home = std::env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".claude")));
        let managed = if cfg!(target_os = "macos") {
            Some("/Library/Application Support/ClaudeCode".into())
        } else if cfg!(target_os = "linux") {
            Some("/etc/claude-code".into())
        } else {
            None
        };
        let plugins = std::env::var_os("CLAUDE_CODE_PLUGIN_CACHE_DIR")
            .map(PathBuf::from)
            .or_else(|| claude_home.as_ref().map(|h| h.join("plugins")));
        Ok(Self {
            project: filesystem::resolve(&project)?,
            claude_json: home.as_ref().map(|h| h.join(".claude.json")),
            home,
            claude_home,
            managed,
            plugins,
            boundary: None,
        })
    }
    /// Isolated layout: `home/` (with `home/.claude` and `home/.claude.json`), `project/`
    /// and `managed/`.
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let root = filesystem::resolve(root)?;
        let project = root.join("project");
        if !project.is_dir() {
            return Err(io::Error::other("fixture must contain a project directory"));
        }
        Ok(Self {
            project,
            home: Some(root.join("home")),
            claude_home: Some(root.join("home/.claude")),
            managed: Some(root.join("managed")),
            claude_json: Some(root.join("home/.claude.json")),
            plugins: Some(root.join("home/.claude/plugins")),
            boundary: Some(root),
        })
    }
    pub(crate) fn permits(&self, path: &Path) -> bool {
        self.boundary
            .as_ref()
            .is_none_or(|root| path.starts_with(root))
    }
}

/// `./…` inside the project, `~/…` under home, otherwise the full path; terminal-safe.
/// Paths are shown as text, not redacted: Den already displays them in Codex evidence.
pub fn short(context: &Context, path: &Path) -> String {
    crate::inventory::short_path(path, &context.project, context.home.as_deref())
}

/// Where an instruction file sits in Claude Code's memory hierarchy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Managed,
    User,
    Project,
    Local,
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Managed => "Managed",
            Self::User => "User",
            Self::Project => "Project",
            Self::Local => "Local",
        }
    }
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Managed => "Set by your organisation for every user on this machine",
            Self::User => "Your personal instructions for every project",
            Self::Project => "Shared project instructions, usually committed",
            Self::Local => "Your private notes for this project (CLAUDE.local.md)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Read,
    Empty,
    /// A symlink: reported with its target, never followed.
    Link(Option<PathBuf>),
    Unreadable,
}

#[derive(Clone, Debug)]
pub struct InstructionFile {
    pub path: PathBuf,
    pub kind: Kind,
    pub status: Status,
    pub bytes: usize,
    /// `@path` references; Claude Code may import them, Den does not follow them.
    pub imports: usize,
    /// Redacted, best effort; never treated as instructions.
    pub preview: String,
}

pub struct Inventory {
    pub skills: skills::Inventory,
    pub instructions: Vec<InstructionFile>,
    /// Candidate locations checked and absent.
    pub absent: usize,
    /// Settings, MCP, hooks and plugins.
    pub config: config::Config,
    pub coverage: Vec<String>,
    /// The Claude home ("~/.claude") when neither it nor ~/.claude.json exists.
    pub missing_home: Option<String>,
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

pub fn discover(context: &Context, cancel: &AtomicBool) -> io::Result<Inventory> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("collection cancelled"));
    }
    let mut roots = vec![skills::Root {
        path: context.project.join(".claude/skills"),
        scope: skills::Scope::Project,
    }];
    if let Some(home) = &context.claude_home {
        roots.push(skills::Root {
            path: home.join("skills"),
            scope: skills::Scope::User,
        });
    }
    let config = skills::Config {
        roots,
        project: context.project.clone(),
        coverage: vec![],
    };
    let skills = skills::discover_bounded(&config, cancel, context.boundary.as_deref())?;
    let (instructions, absent) = instructions(context, cancel)?;
    let config = config::collect(context, cancel)?;
    let exists = |p: &Option<PathBuf>| p.as_ref().is_some_and(|p| p.exists());
    let missing_home =
        (!exists(&context.claude_home) && !exists(&context.claude_json)).then(|| {
            context
                .claude_home
                .as_ref()
                .map_or("~/.claude".into(), |p| short(context, p))
        });
    Ok(Inventory {
        missing_home,
        skills,
        instructions,
        absent,
        config,
        coverage: vec![
            "Skills: ~/.claude/skills and this project's .claude/skills. Plugin skills and nested .claude/skills folders are not listed.".into(),
            "Instructions: the managed file, ~/.claude/CLAUDE.md, and CLAUDE.md, .claude/CLAUDE.md and CLAUDE.local.md in this project and its parent folders. CLAUDE.md files in subfolders, which Claude reads on demand, are not listed.".into(),
            "@imports are counted, not followed. Linked files are reported, not read.".into(),
            "Local files only: what a running Claude Code session loaded is not observed.".into(),
        ],
    })
}

/// Candidate files in the order Claude Code reads them: managed, user, then project
/// folders from the outermost parent down to the project.
fn candidates(context: &Context) -> Vec<(PathBuf, Kind)> {
    let mut files = vec![];
    if let Some(dir) = &context.managed {
        files.push((dir.join("CLAUDE.md"), Kind::Managed));
    }
    if let Some(home) = &context.claude_home {
        files.push((home.join("CLAUDE.md"), Kind::User));
    }
    let mut folders: Vec<_> = context
        .project
        .ancestors()
        .take(64)
        .filter(|dir| dir.parent().is_some() && context.permits(dir))
        .collect();
    folders.reverse();
    for dir in folders {
        files.push((dir.join("CLAUDE.md"), Kind::Project));
        files.push((dir.join(".claude/CLAUDE.md"), Kind::Project));
        files.push((dir.join("CLAUDE.local.md"), Kind::Local));
    }
    files
}

fn instructions(
    context: &Context,
    cancel: &AtomicBool,
) -> io::Result<(Vec<InstructionFile>, usize)> {
    let start = Instant::now();
    let mut found = vec![];
    let mut absent = 0;
    let mut retained = 0;
    for (path, kind) in candidates(context) {
        if cancel.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(30) {
            return Err(io::Error::other(
                "instruction discovery cancelled or timed out",
            ));
        }
        if !context.permits(&path) {
            continue;
        }
        let mut file = InstructionFile {
            path,
            kind,
            status: Status::Read,
            bytes: 0,
            imports: 0,
            preview: String::new(),
        };
        match std::fs::symlink_metadata(&file.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                absent += 1;
                continue;
            }
            Ok(m) if m.file_type().is_symlink() => {
                file.status = Status::Link(std::fs::read_link(&file.path).ok());
                found.push(file);
                continue;
            }
            _ => {}
        }
        match read(context, &file.path) {
            Ok(text) => {
                file.bytes = text.len();
                file.imports = imports(&text);
                if text.trim().is_empty() {
                    file.status = Status::Empty;
                }
                let mut end = text.len().min(PREVIEW_BUDGET.saturating_sub(retained));
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                file.preview = privacy::preview(&text[..end]);
                retained += end;
            }
            Err(_) => file.status = Status::Unreadable,
        }
        found.push(file);
    }
    Ok((found, absent))
}

/// Descriptor-relative read of a regular file; the file itself is never followed.
fn read(context: &Context, path: &Path) -> io::Result<String> {
    read_limited(context, path, FILE_LIMIT)
}
#[cfg(unix)]
pub(crate) fn read_limited(context: &Context, path: &Path, limit: usize) -> io::Result<String> {
    let parent = filesystem::resolve(
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    )?;
    if !context.permits(&parent) {
        return Err(io::Error::other("outside fixture boundary"));
    }
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("missing filename"))?;
    filesystem::text(filesystem::Directory::open(&parent)?.child(name)?, limit)
}
#[cfg(not(unix))]
pub(crate) fn read_limited(_: &Context, _: &Path, _: usize) -> io::Result<String> {
    Err(io::Error::other("secure reads require macOS/Linux"))
}

/// Lines' `@path` tokens, e.g. `@docs/notes.md` or `@~/.claude/extra.md`. Code spans
/// and e-mail-like tokens are not distinguished; the count is a hint, not a parse.
fn imports(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| {
            word.strip_prefix('@').is_some_and(|rest| {
                rest.starts_with(['/', '~', '.'])
                    || rest.contains('/')
                    || Path::new(rest)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
            })
        })
        .count()
}

#[cfg(test)]
mod tests {
    #[test]
    fn imports_count_path_references_only() {
        assert_eq!(
            super::imports("See @docs/a.md and @~/x.md, not @someone or a@b.c\n@README.md"),
            3
        );
    }
}
