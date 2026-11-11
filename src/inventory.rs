use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Package,
    Executable,
    Runtime,
    Library,
    Skill,
    McpServer,
    Cache,
    Formula,
    Cask,
}

impl ResourceKind {
    pub const ALL: [Self; 7] = [
        Self::Package,
        Self::Executable,
        Self::Runtime,
        Self::Library,
        Self::Skill,
        Self::McpServer,
        Self::Cache,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Package => "Package",
            Self::Executable => "Executable",
            Self::Runtime => "Runtime",
            Self::Library => "Library",
            Self::Skill => "Skill",
            Self::McpServer => "MCP server",
            Self::Cache => "Cache",
            Self::Formula => "Formula",
            Self::Cask => "Cask",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub id: String,
    pub name: String,
    pub kind: ResourceKind,
    pub version: Option<String>,
    pub path: Option<String>,
    pub description: String,
    pub origin: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homebrew: Option<HomebrewMetadata>,
}
impl Resource {
    /// Casks are bracketed so they stand apart from formulae without a type column.
    pub fn display_name(&self) -> String {
        if self.kind == ResourceKind::Cask {
            format!("[{}]", self.name)
        } else {
            self.name.clone()
        }
    }
    pub fn intent(&self) -> InstallationIntent {
        self.homebrew
            .as_ref()
            .map_or(InstallationIntent::Unknown, HomebrewMetadata::intent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationIntent {
    Requested,
    Dependency,
    Unknown,
}

impl InstallationIntent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Requested => "Requested",
            Self::Dependency => "Dependency",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledKeg {
    pub version: String,
    pub installed_on_request: Option<bool>,
    pub installed_as_dependency: Option<bool>,
    // Keep the recorded dependency objects (including version/revision/directness)
    // intact for Stage 2. They are never interpreted as commands or rendered raw.
    pub runtime_dependencies: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HomebrewMetadata {
    Formula {
        full_name: Option<String>,
        tap: Option<String>,
        homepage: Option<String>,
        linked_keg: Option<String>,
        installed: Vec<InstalledKeg>,
        dependencies: std::collections::BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        aliases: Vec<String>,
        /// Newest stable version Homebrew knows of (as of the last `brew update`).
        #[serde(default)]
        latest: Option<String>,
        #[serde(default)]
        outdated: Option<bool>,
        /// Pinned formulae are not upgraded by `brew upgrade`.
        #[serde(default)]
        pinned: Option<bool>,
    },
    Cask {
        full_token: Option<String>,
        display_names: Vec<String>,
        tap: Option<String>,
        homepage: Option<String>,
        recorded_version: String,
        installed_on_request: Option<bool>,
        installed_as_dependency: Option<bool>,
        depends_on: Option<serde_json::Value>,
        #[serde(default)]
        old_tokens: Vec<String>,
        /// Newest version Homebrew knows of (as of the last `brew update`).
        #[serde(default)]
        latest: Option<String>,
        #[serde(default)]
        outdated: Option<bool>,
    },
}

/// A false request flag alone does not prove a dependency installation.
pub fn installation_intent(
    requested: Option<bool>,
    dependency: Option<bool>,
) -> InstallationIntent {
    match (requested, dependency) {
        (Some(true), Some(true)) => InstallationIntent::Unknown,
        (Some(true), _) => InstallationIntent::Requested,
        (_, Some(true)) => InstallationIntent::Dependency,
        _ => InstallationIntent::Unknown,
    }
}

impl HomebrewMetadata {
    /// The newer version Homebrew reports for an outdated package. Homebrew compares
    /// against its local metadata, so this is as current as the last `brew update`.
    pub fn update(&self) -> Option<&str> {
        let (latest, outdated) = match self {
            Self::Formula {
                latest, outdated, ..
            }
            | Self::Cask {
                latest, outdated, ..
            } => (latest, outdated),
        };
        (*outdated == Some(true)).then(|| latest.as_deref().unwrap_or("newer version"))
    }
    pub fn pinned(&self) -> bool {
        matches!(
            self,
            Self::Formula {
                pinned: Some(true),
                ..
            }
        )
    }
    pub fn intent(&self) -> InstallationIntent {
        match self {
            Self::Formula { installed, .. } => {
                let first = installed.first().map(|k| {
                    installation_intent(k.installed_on_request, k.installed_as_dependency)
                });
                first
                    .filter(|intent| {
                        installed.iter().all(|k| {
                            installation_intent(k.installed_on_request, k.installed_as_dependency)
                                == *intent
                        })
                    })
                    .unwrap_or(InstallationIntent::Unknown)
            }
            Self::Cask {
                installed_on_request,
                installed_as_dependency,
                ..
            } => installation_intent(*installed_on_request, *installed_as_dependency),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub schema_version: u32,
    pub resources: Vec<Resource>,
}

/// Replace terminal controls and Unicode direction/format controls with visible markers.
/// Never pass raw snapshot strings to terminal widgets or diagnostics.
pub fn safe_text(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_control()
                || matches!(ch,
            '\u{00ad}' | '\u{061c}' | '\u{200b}'..='\u{200f}' |
            '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
            {
                '\u{fffd}'
            } else {
                ch
            }
        })
        .collect()
}

/// Shared literal, case-insensitive, all-words search for package and skill inventories.
pub fn matches_query(query: &str, text: &str) -> bool {
    let text = safe_text(text).to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|term| text.contains(term))
}

/// `./…` inside the project, `~/…` under home, otherwise the full path; terminal-safe.
/// Paths are shown as text, not redacted: they are local locations, not secrets.
pub fn short_path(
    path: &std::path::Path,
    project: &std::path::Path,
    home: Option<&std::path::Path>,
) -> String {
    let text = if let Ok(rest) = path.strip_prefix(project) {
        format!("./{}", rest.display())
    } else if let Some(rest) = home.and_then(|home| path.strip_prefix(home).ok()) {
        format!("~/{}", rest.display())
    } else {
        path.display().to_string()
    };
    safe_text(&text)
}

/// "1 skill", "2 skills".
pub fn count(n: usize, noun: &str) -> String {
    format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
}
