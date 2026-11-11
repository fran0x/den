//! Docker containers, images and volumes, from four fixed read-only `docker` commands.
//!
//! Nothing is started, stopped, pulled, inspected or removed. Container commands, labels
//! (except the Compose project name) and environment are withheld. From
//! `~/.docker/config.json` only the current context, registry names and credential helper
//! names are kept; saved logins are dropped while parsing. `DOCKER_HOST` is noted, not shown.
pub mod state;

use crate::inventory::safe_text;
use serde_json::{Map, Value};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

/// The only commands Den runs: each lists one kind of object as JSON lines.
pub const COMMANDS: [(&str, &[&str]); 4] = [
    (
        "containers",
        &[
            "container",
            "ls",
            "--all",
            "--no-trunc",
            "--format",
            "{{json .}}",
        ],
    ),
    (
        "images",
        &["image", "ls", "--no-trunc", "--format", "{{json .}}"],
    ),
    ("volumes", &["volume", "ls", "--format", "{{json .}}"]),
    ("disk", &["system", "df", "--format", "{{json .}}"]),
];
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const TIMEOUT_SECONDS: u64 = 10;

#[derive(Clone, Debug)]
pub struct Context {
    /// `DOCKER_CONFIG`, or `~/.docker`.
    pub config_dir: Option<PathBuf>,
    pub home: Option<PathBuf>,
    /// `DOCKER_HOST` is set: Docker may talk to another machine. The value is withheld.
    pub remote_host: bool,
    /// `DOCKER_CONTEXT`: a context name, not a secret.
    pub context: Option<String>,
    /// Fixture: JSON-lines files named after `COMMANDS` instead of running `docker`.
    pub fixture: Option<PathBuf>,
}
impl Context {
    pub fn local() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        Self {
            config_dir: std::env::var_os("DOCKER_CONFIG")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".docker"))),
            home,
            remote_host: std::env::var_os("DOCKER_HOST").is_some_and(|v| !v.is_empty()),
            context: std::env::var("DOCKER_CONTEXT")
                .ok()
                .filter(|c| !c.is_empty() && c.len() <= 128),
            fixture: None,
        }
    }
    /// Isolated layout: `containers.jsonl`, `images.jsonl`, `volumes.jsonl`, `disk.jsonl`
    /// and `home/.docker/config.json`. Nothing is run.
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let root = crate::skills::filesystem::resolve(root)?;
        Ok(Self {
            config_dir: Some(root.join("home/.docker")),
            home: Some(root.join("home")),
            remote_host: false,
            context: None,
            fixture: Some(root),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Containers,
    Images,
    Volumes,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub name: String,
    /// State (containers), size (images) or driver (volumes).
    pub state: String,
    pub status: String,
    pub detail: String,
    pub key: String,
}

#[derive(Default)]
pub struct Inventory {
    pub containers: Vec<Row>,
    pub images: Vec<Row>,
    pub volumes: Vec<Row>,
    /// `docker system df` lines: (type, size, reclaimable).
    pub disk: Vec<(String, String, String)>,
    pub running: usize,
    /// Output lines that were not JSON objects; their rows are missing, so counts may be low.
    pub unreadable: usize,
    /// Current context, registries with saved logins and credential helpers.
    pub setup: Vec<String>,
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

/// Why Docker could not be listed, in words for the status line.
fn failure(error: crate::homebrew::CollectionError) -> io::Error {
    use crate::homebrew::CollectionError as E;
    io::Error::other(match error {
        E::Unavailable => "Docker CLI not found: put docker on PATH, then press r".into(),
        E::Timeout => format!("docker did not answer within {TIMEOUT_SECONDS} seconds"),
        E::OutputLimit => "docker output exceeded its size limit".into(),
        E::Cancelled => "Docker collection cancelled".into(),
        _ => "docker failed".into(),
    })
}

/// One command's JSON lines, from `docker` or the fixture, and how many lines were not
/// JSON objects (counted, not silently dropped).
fn lines(
    context: &Context,
    name: &str,
    args: &[&str],
    cancel: &AtomicBool,
) -> io::Result<(Vec<Map<String, Value>>, usize)> {
    let bytes = match &context.fixture {
        Some(root) => fixture(&root.join(format!("{name}.jsonl")))?,
        None => run(args, cancel)?,
    };
    let (mut rows, mut unreadable) = (vec![], 0);
    for line in String::from_utf8_lossy(&bytes).lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Object(map)) => rows.push(map),
            _ => unreadable += 1,
        }
    }
    Ok((rows, unreadable))
}

/// A saved listing, with the same size limit as live output; a missing file is empty.
fn fixture(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let mut bytes = vec![];
    file.take(OUTPUT_LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_LIMIT {
        return Err(io::Error::other("docker output exceeded its size limit"));
    }
    Ok(bytes)
}

#[cfg(unix)]
fn run(args: &[&str], cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    let mut command = std::process::Command::new("docker");
    command.args(args).env("DOCKER_CLI_HINTS", "false");
    let output = crate::homebrew::run_bounded(
        &mut command,
        cancel,
        std::time::Duration::from_secs(TIMEOUT_SECONDS),
        OUTPUT_LIMIT,
    )
    .map_err(failure)?;
    if output.code == Some(0) {
        Ok(output.stdout)
    } else {
        // Usually the daemon is not running; Docker's own message is not shown.
        Err(io::Error::other(
            "Docker daemon not reachable: start Docker, then press r",
        ))
    }
}
#[cfg(not(unix))]
fn run(_: &[&str], _: &AtomicBool) -> io::Result<Vec<u8>> {
    Err(io::Error::other("Docker collection requires macOS/Linux"))
}

fn text(map: &Map<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => safe_text(s),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

pub fn discover(context: &Context, cancel: &AtomicBool) -> io::Result<Inventory> {
    // The four listings run side by side: the wait is the slowest one (usually
    // `system df`), not their sum. Each keeps its own time and output limits.
    let found = std::thread::scope(|scope| {
        let workers: Vec<_> = COMMANDS
            .iter()
            .map(|(name, args)| scope.spawn(move || lines(context, name, args, cancel)))
            .collect();
        workers
            .into_iter()
            .map(|w| {
                w.join()
                    .unwrap_or_else(|_| Err(io::Error::other("docker listing failed")))
            })
            .collect::<io::Result<Vec<_>>>()
    })?;
    let unreadable = found.iter().map(|(_, n)| n).sum();
    let found: Vec<_> = found.into_iter().map(|(rows, _)| rows).collect();
    let [containers, images, volumes, disk] =
        <[_; 4]>::try_from(found).map_err(|_| io::Error::other("unexpected command count"))?;

    let container_rows: Vec<_> = containers.iter().map(container_row).collect();
    let mut inventory = Inventory {
        running: containers
            .iter()
            .filter(|c| text(c, "State") == "running")
            .count(),
        disk: disk
            .iter()
            .map(|d| (text(d, "Type"), text(d, "Size"), text(d, "Reclaimable")))
            .collect(),
        setup: setup(context),
        unreadable,
        ..Inventory::default()
    };
    for image in &images {
        let repo = text(image, "Repository");
        let tag = text(image, "Tag");
        let id = text(image, "ID");
        let short = id
            .trim_start_matches("sha256:")
            .chars()
            .take(12)
            .collect::<String>();
        let dangling = repo == "<none>";
        let name = if dangling {
            format!("<none> {short}")
        } else {
            format!("{repo}:{tag}")
        };
        // Containers name images by repo:tag, repo (meaning :latest) or ID.
        let users: Vec<_> = containers
            .iter()
            .filter(|c| {
                let used = text(c, "Image");
                (!dangling && (used == name || (tag == "latest" && used == repo)))
                    || (!short.is_empty() && used.trim_start_matches("sha256:").starts_with(&short))
            })
            .map(|c| text(c, "Names"))
            .collect();
        let status = match (users.len(), dangling) {
            (0, true) => "Unused · dangling".to_owned(),
            (0, false) => "Unused".to_owned(),
            (n, _) => crate::inventory::count(n, "container"),
        };
        inventory.images.push(Row {
            state: text(image, "Size"),
            detail: [
                name.clone(),
                String::new(),
                format!("Status: {status}"),
                format!("Size: {}", text(image, "Size")),
                format!("Created: {}", text(image, "CreatedSince")),
                format!("ID: {short}"),
                format!(
                    "Used by: {}",
                    if users.is_empty() {
                        "no container".into()
                    } else {
                        users.join(", ")
                    }
                ),
                String::new(),
                if dangling {
                    "Dangling: no tag points to this image.\n".into()
                } else {
                    String::new()
                },
                "Not shown: image history, layers and labels. Nothing is pulled or removed.".into(),
            ]
            .join("\n")
            .replace("\n\n\n", "\n\n"),
            key: format!("image:{id}:{name}"),
            status,
            name,
        });
    }
    for volume in &volumes {
        let name = text(volume, "Name");
        let users: Vec<_> = containers
            .iter()
            .filter(|c| text(c, "Mounts").split(',').any(|m| m.trim() == name))
            .map(|c| text(c, "Names"))
            .collect();
        let status = if users.is_empty() {
            "Unused".to_owned()
        } else {
            crate::inventory::count(users.len(), "container")
        };
        let display = if name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            format!("{}… (anonymous)", &name[..12])
        } else {
            name.clone()
        };
        inventory.volumes.push(Row {
            state: text(volume, "Driver"),
            detail: [
                display.clone(),
                String::new(),
                format!("Status: {status}"),
                format!(
                    "Driver: {} · scope: {}",
                    text(volume, "Driver"),
                    text(volume, "Scope")
                ),
                format!(
                    "Used by: {}",
                    if users.is_empty() {
                        "no container".into()
                    } else {
                        users.join(", ")
                    }
                ),
                String::new(),
                "Not shown: volume contents and labels. Nothing is removed.".into(),
            ]
            .join("\n"),
            key: format!("volume:{name}"),
            status,
            name: display,
        });
    }
    inventory.containers = container_rows;
    Ok(inventory)
}

fn container_row(c: &Map<String, Value>) -> Row {
    let name = text(c, "Names");
    let state = text(c, "State");
    let image = text(c, "Image");
    // Only the Compose project is taken from labels; other labels may hold anything.
    let project = text(c, "Labels")
        .split(',')
        .find_map(|l| l.strip_prefix("com.docker.compose.project="))
        .map(str::to_owned);
    let mounts: Vec<_> = text(c, "Mounts")
        .split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
        .collect();
    let label = match state.as_str() {
        "running" => "Running",
        "paused" => "Paused",
        "restarting" => "Restarting",
        "created" => "Created",
        "exited" => "Stopped",
        "dead" => "Dead",
        _ => "Unknown",
    };
    let mut detail = vec![
        name.clone(),
        String::new(),
        format!("State: {label} · {}", text(c, "Status")),
        format!("Image: {image}"),
        format!("Created: {}", text(c, "RunningFor")),
    ];
    let ports = text(c, "Ports");
    if !ports.is_empty() {
        detail.push(format!("Ports: {ports}"));
    }
    if let Some(project) = &project {
        detail.push(format!("Compose project: {project}"));
    }
    if !mounts.is_empty() {
        detail.push(format!("Mounts: {}", mounts.join(", ")));
    }
    detail.push(format!(
        "ID: {}",
        text(c, "ID").chars().take(12).collect::<String>()
    ));
    detail.push(String::new());
    detail.push(
        "Not shown: the command, environment, labels and logs. Nothing is started, stopped or removed."
            .into(),
    );
    Row {
        state: label.into(),
        status: format!(
            "{image}{}",
            project.map(|p| format!(" · {p}")).unwrap_or_default()
        ),
        detail: detail.join("\n"),
        key: format!("container:{}", text(c, "ID")),
        name,
    }
}

/// Descriptor-relative, size-limited read; the file itself is never followed.
#[cfg(unix)]
fn read(path: &Path) -> Option<String> {
    use crate::skills::filesystem;
    let parent = filesystem::resolve(path.parent()?).ok()?;
    let file = filesystem::Directory::open(&parent)
        .ok()?
        .child(path.file_name()?)
        .ok()?;
    filesystem::text(file, 1024 * 1024).ok()
}
#[cfg(not(unix))]
fn read(_: &Path) -> Option<String> {
    None
}

/// What `~/.docker/config.json` says about the setup. Saved logins are dropped here.
fn setup(context: &Context) -> Vec<String> {
    let mut lines = vec![];
    if let Some(name) = &context.context {
        lines.push(format!("Context: {} (DOCKER_CONTEXT)", safe_text(name)));
    }
    if context.remote_host {
        lines.push("DOCKER_HOST is set: Docker may be another machine (value withheld)".into());
    }
    let Some(path) = context.config_dir.as_ref().map(|d| d.join("config.json")) else {
        return lines;
    };
    let config = match std::fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() => {
            lines.push("~/.docker/config.json is a link; not read".into());
            return lines;
        }
        Ok(_) => read(&path).and_then(|t| serde_json::from_str::<Value>(&t).ok()),
        Err(_) => None,
    };
    let Some(Value::Object(config)) = config else {
        return lines;
    };
    if context.context.is_none()
        && let Some(Value::String(current)) = config.get("currentContext")
    {
        lines.push(format!("Context: {}", safe_text(current)));
    }
    let names = |key: &str| -> Vec<String> {
        config
            .get(key)
            .and_then(Value::as_object)
            .map(|m| m.keys().map(|k| safe_text(k)).collect())
            .unwrap_or_default()
    };
    let registries = names("auths");
    if !registries.is_empty() {
        lines.push(format!(
            "Registries with saved logins: {} (credentials withheld)",
            registries.join(", ")
        ));
    }
    if let Some(Value::String(store)) = config.get("credsStore") {
        lines.push(format!(
            "Credential helper: docker-credential-{}",
            safe_text(store)
        ));
    }
    let helpers = names("credHelpers");
    if !helpers.is_empty() {
        lines.push(format!("Per-registry helpers for: {}", helpers.join(", ")));
    }
    lines
}

#[cfg(test)]
mod tests {
    #[test]
    fn commands_only_list() {
        for (_, args) in super::COMMANDS {
            assert!(args.contains(&"ls") || args == ["system", "df", "--format", "{{json .}}"]);
            for forbidden in [
                "rm", "prune", "run", "exec", "start", "stop", "pull", "inspect", "logs",
            ] {
                assert!(!args.contains(&forbidden));
            }
        }
    }
}
