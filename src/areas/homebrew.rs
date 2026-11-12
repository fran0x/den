//! Homebrew JSON normalization and the single read-only collection boundary.
pub mod diagnostics;
pub mod explorer;
pub mod graph;
mod process;
#[cfg(test)]
mod reconciliation;
pub use process::{Collector, CommandOutput, collect, diagnose};
#[cfg(unix)]
pub(crate) use process::{run_bounded, run_write};

use crate::inventory::{HomebrewMetadata, InstalledKeg, Inventory, Resource, ResourceKind};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    io::Read,
    path::{Path, PathBuf},
};

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PACKAGES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionError {
    Unavailable,
    Failed(String),
    Timeout,
    OutputLimit,
    Cancelled,
    Invalid(&'static str),
}

impl fmt::Display for CollectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => write!(f, "Homebrew unavailable: put brew on PATH, then press r"),
            Self::Failed(message) => write!(f, "Homebrew collection failed: {message}"),
            Self::Timeout => write!(f, "Homebrew collection exceeded 30 seconds"),
            Self::OutputLimit => write!(f, "Homebrew output exceeded its size limit"),
            Self::Cancelled => write!(f, "Homebrew collection cancelled"),
            Self::Invalid(message) => write!(f, "Invalid Homebrew inventory: {message}"),
        }
    }
}
impl std::error::Error for CollectionError {}

// Raw DTOs are distinct from Den's presentation model. Additive upstream fields
// are accepted, but essential inventory/installation records cannot be omitted.
#[derive(Deserialize)]
struct BrewInventory {
    formulae: Vec<Formula>,
    casks: Vec<Cask>,
}

#[derive(Deserialize)]
struct Formula {
    name: String,
    full_name: Option<String>,
    desc: Option<String>,
    tap: Option<String>,
    homepage: Option<String>,
    linked_keg: Option<String>,
    installed: Vec<InstalledKeg>,
    aliases: Option<Vec<String>>,
    oldnames: Option<Vec<String>>,
    versions: Option<FormulaVersions>,
    revision: Option<u32>,
    outdated: Option<bool>,
    pinned: Option<bool>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}

#[derive(Deserialize)]
struct FormulaVersions {
    stable: Option<String>,
}

#[derive(Deserialize)]
struct Cask {
    token: String,
    full_token: Option<String>,
    name: Option<Vec<String>>,
    desc: Option<String>,
    tap: Option<String>,
    homepage: Option<String>,
    installed: String,
    installed_on_request: Option<bool>,
    installed_as_dependency: Option<bool>,
    depends_on: Option<Value>,
    old_tokens: Option<Vec<String>>,
    version: Option<String>,
    outdated: Option<bool>,
}

pub fn parse(bytes: &[u8]) -> Result<Inventory, CollectionError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(CollectionError::OutputLimit);
    }
    let raw: BrewInventory = serde_json::from_slice(bytes).map_err(|_| {
        CollectionError::Invalid("malformed JSON or missing/incompatible required fields")
    })?;
    if raw.formulae.len().saturating_add(raw.casks.len()) > MAX_PACKAGES {
        return Err(CollectionError::Invalid(
            "too many packages (maximum 10000)",
        ));
    }
    let mut resources = Vec::new();
    for formula in raw.formulae {
        required(&formula.name)?;
        if formula.installed.is_empty() {
            return Err(CollectionError::Invalid("formula has no installed kegs"));
        }
        for keg in &formula.installed {
            required(&keg.version)?;
        }
        let versions = formula
            .installed
            .iter()
            .map(|k| k.version.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let id = identity(
            "formula",
            &formula.name,
            formula.full_name.as_deref(),
            formula.tap.as_deref(),
        );
        // Kegs carry the revision ("1.1.2_1"), so the latest version does too.
        let latest = formula
            .versions
            .and_then(|v| v.stable)
            .filter(|v| !v.trim().is_empty())
            .map(|v| match formula.revision {
                Some(r) if r > 0 => format!("{v}_{r}"),
                _ => v,
            });
        let metadata = HomebrewMetadata::Formula {
            latest,
            outdated: formula.outdated,
            pinned: formula.pinned,
            aliases: formula
                .aliases
                .into_iter()
                .flatten()
                .chain(formula.oldnames.into_iter().flatten())
                .collect(),
            full_name: formula.full_name,
            tap: formula.tap,
            homepage: formula.homepage,
            linked_keg: formula.linked_keg,
            installed: formula.installed,
            dependencies: formula
                .extra
                .into_iter()
                .filter(|(key, _)| {
                    matches!(
                        key.as_str(),
                        "dependencies"
                            | "build_dependencies"
                            | "test_dependencies"
                            | "optional_dependencies"
                            | "recommended_dependencies"
                            | "uses_from_macos"
                            | "uses_from_macos_bounds"
                            | "requirements"
                    )
                })
                .collect(),
        };
        resources.push(resource(
            id,
            formula.name,
            ResourceKind::Formula,
            versions,
            formula.desc,
            metadata,
        ));
    }
    for cask in raw.casks {
        required(&cask.token)?;
        required(&cask.installed)?;
        let id = identity(
            "cask",
            &cask.token,
            cask.full_token.as_deref(),
            cask.tap.as_deref(),
        );
        let metadata = HomebrewMetadata::Cask {
            old_tokens: cask.old_tokens.unwrap_or_default(),
            full_token: cask.full_token,
            display_names: cask.name.unwrap_or_default(),
            tap: cask.tap,
            homepage: cask.homepage,
            recorded_version: cask.installed.clone(),
            installed_on_request: cask.installed_on_request,
            installed_as_dependency: cask.installed_as_dependency,
            depends_on: cask.depends_on,
            latest: cask.version.filter(|v| !v.trim().is_empty()),
            outdated: cask.outdated,
        };
        resources.push(resource(
            id,
            cask.token,
            ResourceKind::Cask,
            cask.installed,
            cask.desc,
            metadata,
        ));
    }
    let mut identities = HashSet::new();
    for resource in &resources {
        if !identities.insert(&resource.id) {
            return Err(CollectionError::Invalid("duplicate package identity"));
        }
        // Validate retained metadata too, without echoing any of it in diagnostics.
        let value = serde_json::to_value(resource)
            .map_err(|_| CollectionError::Invalid("cannot normalize metadata"))?;
        validate_value(&value)?;
    }
    // Homebrew input order is retained, matching the snapshot/source-order convention.
    Ok(Inventory {
        schema_version: 1,
        resources,
    })
}

fn resource(
    id: String,
    name: String,
    kind: ResourceKind,
    version: String,
    description: Option<String>,
    metadata: HomebrewMetadata,
) -> Resource {
    let reason = if metadata.request_recorded() {
        format!(
            "{} according to Homebrew installation records; missing or conflicting evidence is Unknown.",
            metadata.intent().label()
        )
    } else {
        "Homebrew keeps no installed-on-request record for casks; a cask nothing installed needs is listed under Tools.".into()
    };
    Resource {
        id,
        name,
        kind,
        version: Some(version),
        path: None,
        description: description.unwrap_or_default(),
        origin: "Homebrew installation records".into(),
        reason,
        homebrew: Some(metadata),
    }
}

fn required(value: &str) -> Result<(), CollectionError> {
    if value.trim().is_empty() {
        Err(CollectionError::Invalid(
            "empty package name or installed version",
        ))
    } else {
        Ok(())
    }
}

fn identity(kind: &str, name: &str, full: Option<&str>, tap: Option<&str>) -> String {
    let full = full.filter(|s| !s.trim().is_empty()).unwrap_or(name);
    let qualified = match tap.filter(|s| !s.trim().is_empty()) {
        Some(tap) if !full.starts_with(&format!("{tap}/")) => format!("{tap}/{full}"),
        _ => full.to_owned(),
    };
    format!("brew:{kind}:{qualified}")
}

fn validate_value(value: &Value) -> Result<(), CollectionError> {
    match value {
        Value::String(s) if s.len() > 8192 => {
            return Err(CollectionError::Invalid(
                "metadata field exceeds 8192 bytes",
            ));
        }
        Value::Array(items) => {
            if items.len() > 2048 {
                return Err(CollectionError::Invalid(
                    "metadata array exceeds 2048 entries",
                ));
            }
            for item in items {
                validate_value(item)?;
            }
        }
        Value::Object(items) => {
            for (key, value) in items {
                if key.len() > 8192 {
                    return Err(CollectionError::Invalid("metadata key exceeds 8192 bytes"));
                }
                validate_value(value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn load_snapshot(path: &Path) -> Result<Inventory, CollectionError> {
    let file = crate::source::open_snapshot(path)
        .map_err(|_| CollectionError::Invalid("snapshot must be a readable regular file"))?;
    read_snapshot(file)
}

pub fn read_snapshot(reader: impl Read) -> Result<Inventory, CollectionError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| CollectionError::Invalid("cannot read snapshot"))?;
    parse(&bytes)
}

impl From<std::io::Error> for CollectionError {
    fn from(_: std::io::Error) -> Self {
        Self::Failed("collection worker unavailable".into())
    }
}

/// Homebrew's download cache: `HOMEBREW_CACHE`, or its usual place.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(cache) = std::env::var_os("HOMEBREW_CACHE") {
        return Some(PathBuf::from(cache));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(if cfg!(target_os = "macos") {
        home.join("Library/Caches/Homebrew")
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"))
            .join("Homebrew")
    })
}

/// When Homebrew last downloaded its package data (`*.jws.json` under `api/`, or
/// `api/internal/` in newer versions): "outdated" is only as current as this. Only file
/// times are read, never the files; anything unreadable is simply skipped.
pub fn data_refreshed(cache: &Path) -> Option<std::time::SystemTime> {
    let api = cache.join("api");
    [api.join("internal"), api]
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.take(256).flatten())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".jws.json"))
        // Not followed: a link's own metadata is not a regular file.
        .filter_map(|e| e.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .filter_map(|m| m.modified().ok())
        .max()
}
