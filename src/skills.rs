//! Codex skill inventory. Inspected instructions are data, never agent instructions.
mod collect;
pub(crate) mod filesystem;
mod metadata;
pub mod state;
pub use collect::{Config, Root, discover};
use std::{io, path::PathBuf};

pub const MAX_SKILLS: usize = 1024;
pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_FILES: usize = 512;
pub const MAX_DEPTH: usize = 8;
pub const MAX_INSTRUCTIONS: usize = 1024 * 1024;
pub const MAX_FILE: usize = 256 * 1024;
pub const MAX_TOTAL: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    User,
    Project,
    Admin,
    System,
    Legacy,
}
impl Scope {
    pub const ALL: [Self; 5] = [
        Self::User,
        Self::Project,
        Self::Admin,
        Self::System,
        Self::Legacy,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Project => "Project",
            Self::Admin => "Admin",
            Self::System => "Bundled",
            Self::Legacy => "Legacy",
        }
    }
    /// One plain sentence on where the skill lives and what that means.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::User => "Your skills folder (~/.agents/skills)",
            Self::Project => "This project's .agents/skills folder",
            Self::Admin => "The admin-managed skills folder",
            Self::System => {
                "Codex's cached copy of a skill it ships with (~/.codex/skills/.system); not proof Codex loads it"
            }
            Self::Legacy => "An older location (~/.codex/skills); current Codex may not read it",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    Valid,
    Invalid,
    NotEvaluated,
}
impl Validity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Valid => "Valid",
            Self::Invalid => "Invalid",
            Self::NotEvaluated => "Not evaluated",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}
#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}
#[derive(Debug, Clone)]
pub struct SupportingFile {
    pub path: PathBuf,
    pub kind: String,
    pub executable: bool,
    pub text: Option<String>,
    pub note: String,
}
#[derive(Debug, Clone)]
pub struct McpDependency {
    pub value: String,
    pub description: Option<String>,
    pub transport: Option<String>,
    pub url: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Skill {
    pub path: PathBuf,
    pub resolved: Option<PathBuf>,
    pub scope: Scope,
    pub name: String,
    pub description: String,
    pub declared_name: Option<String>,
    pub metadata: Validity,
    pub accessible: bool,
    pub incomplete: bool,
    pub duplicate: bool,
    pub instructions: String,
    pub files: Vec<SupportingFile>,
    pub findings: Vec<Finding>,
    pub mcp: Vec<McpDependency>,
    pub optional: Validity,
}
impl Skill {
    /// Why Den cannot call this skill usable from its own files alone, shared by the
    /// Codex and Claude areas: (status label, explanation).
    pub fn unknown_reason(&self) -> Option<(&'static str, &'static str)> {
        Some(if self.metadata == Validity::Invalid {
            (
                "Unknown · invalid SKILL.md",
                "SKILL.md frontmatter is invalid, so the agent may skip this skill",
            )
        } else if self.metadata == Validity::NotEvaluated {
            ("Unknown · not checked", "SKILL.md was not checked")
        } else if !self.accessible || self.incomplete {
            (
                "Unknown · unreadable",
                "The skill folder could not be fully read",
            )
        } else if self.duplicate {
            (
                "Unknown · duplicate name",
                "Another skill has the same name; which one the agent uses is not known",
            )
        } else {
            return None;
        })
    }
    pub fn finding(&mut self, severity: Severity, message: impl Into<String>) {
        self.findings.push(Finding {
            severity,
            message: message.into(),
        });
    }
}
#[derive(Debug, Clone)]
pub struct Location {
    pub path: PathBuf,
    pub scope: Scope,
    pub status: String,
}
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub skills: Vec<Skill>,
    pub locations: Vec<Location>,
    pub coverage: Vec<String>,
    pub partial: bool,
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

pub(crate) use collect::discover_bounded;
