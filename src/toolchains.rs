//! Toolchains: which language tools win on PATH, Rust toolchains and `cargo install`
//! crates, and global packages (npm, uv, pipx, bun, go). Read from files only: no
//! toolchain, package manager or program found on PATH is ever run.
//!
//! Versions come from install paths and package metadata. Registry and git URLs, local
//! source paths and folder-override paths outside the home folder are withheld or shortened.
pub mod state;

use crate::{
    aws::{Read, read_file},
    inventory::{count, safe_text},
    skills::filesystem,
    status::Status,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const FILE_LIMIT: usize = 4 * 1024 * 1024;
const NAMES_LIMIT: usize = 1024;
const PATH_LIMIT: usize = 64;

/// Commands looked up on PATH, grouped by language.
pub const COMMANDS: &[&str] = &[
    "node", "npm", "npx", "pnpm", "yarn", "bun", "deno", "python3", "python", "pip3", "pip", "uv",
    "pipx", "ruby", "gem", "bundle", "go", "java", "javac", "rustup", "rustc", "cargo",
];
/// Binaries in `~/.cargo/bin` that rustup installs as proxies for the active toolchain.
const RUSTUP_PROXIES: &[&str] = &[
    "rustup",
    "rustc",
    "rustdoc",
    "cargo",
    "rust-lldb",
    "rust-gdb",
    "rust-gdbgui",
    "rls",
    "cargo-clippy",
    "clippy-driver",
    "cargo-miri",
    "rust-analyzer",
    "rustfmt",
    "cargo-fmt",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Path,
    Rust,
    Packages,
}
impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Path => "On PATH",
            Self::Rust => "Rust",
            Self::Packages => "Packages",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Context {
    /// PATH entries in order, as given (relative entries are kept to report them).
    pub path: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub rustup_home: Option<PathBuf>,
    pub cargo_home: Option<PathBuf>,
    pub uv_tools: Option<PathBuf>,
    pub uv_python: Option<PathBuf>,
    pub pipx_home: Option<PathBuf>,
    pub bun_install: Option<PathBuf>,
    pub go_bin: Option<PathBuf>,
    /// `NPM_CONFIG_PREFIX`; otherwise the prefix of the `npm` that wins on PATH.
    pub npm_prefix: Option<PathBuf>,
    /// `JAVA_HOME` is set, so `/usr/bin/java` can find a JDK.
    pub java_home: bool,
    /// Fixture root: nothing outside it is read, and paths are shown relative to it.
    pub boundary: Option<PathBuf>,
}
impl Context {
    pub fn local() -> Self {
        let env = |v: &str| {
            std::env::var_os(v)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let home = env("HOME");
        let under = |p: &str| home.as_ref().map(|h| h.join(p));
        let data = env("XDG_DATA_HOME").or_else(|| under(".local/share"));
        Self {
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).take(PATH_LIMIT).collect())
                .unwrap_or_default(),
            rustup_home: env("RUSTUP_HOME").or_else(|| under(".rustup")),
            cargo_home: env("CARGO_HOME").or_else(|| under(".cargo")),
            uv_tools: env("UV_TOOL_DIR").or_else(|| data.as_ref().map(|d| d.join("uv/tools"))),
            uv_python: env("UV_PYTHON_INSTALL_DIR")
                .or_else(|| data.as_ref().map(|d| d.join("uv/python"))),
            pipx_home: env("PIPX_HOME").or_else(|| under(".local/pipx")),
            bun_install: env("BUN_INSTALL").or_else(|| under(".bun")),
            go_bin: env("GOBIN")
                .or_else(|| {
                    env("GOPATH")
                        .and_then(|p| std::env::split_paths(&p).next().map(|p| p.join("bin")))
                })
                .or_else(|| under("go/bin")),
            npm_prefix: env("NPM_CONFIG_PREFIX"),
            java_home: env("JAVA_HOME").is_some(),
            home,
            boundary: None,
        }
    }
    /// Isolated layout: `home/` plus system folders under the root, and a `PATH` file with
    /// one entry per line (`/x` means `<root>/x`, `~/x` means `<root>/home/x`).
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let root = filesystem::resolve(root)?;
        let home = root.join("home");
        let text = match read_file(Some(&root), &root.join("PATH"), 64 * 1024) {
            Read::Text(text) => text,
            Read::Absent => return Err(io::Error::other("fixture has no PATH file")),
            Read::Problem(why) => return Err(io::Error::other(format!("fixture PATH: {why}"))),
        };
        let path = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(PATH_LIMIT)
            .map(|l| {
                if let Some(rest) = l.strip_prefix("~/") {
                    home.join(rest)
                } else if let Some(rest) = l.strip_prefix('/') {
                    root.join(rest)
                } else {
                    PathBuf::from(l)
                }
            })
            .collect();
        Ok(Self {
            path,
            rustup_home: Some(home.join(".rustup")),
            cargo_home: Some(home.join(".cargo")),
            uv_tools: Some(home.join(".local/share/uv/tools")),
            uv_python: Some(home.join(".local/share/uv/python")),
            pipx_home: Some(home.join(".local/pipx")),
            bun_install: Some(home.join(".bun")),
            go_bin: Some(home.join("go/bin")),
            npm_prefix: None,
            java_home: false,
            home: Some(home),
            boundary: Some(root),
        })
    }
    /// The path as the machine sees it: fixture paths lose their root.
    fn logical(&self, path: &Path) -> PathBuf {
        match self
            .boundary
            .as_ref()
            .and_then(|r| path.strip_prefix(r).ok())
        {
            Some(rest) => Path::new("/").join(rest),
            None => path.to_path_buf(),
        }
    }
    pub fn short(&self, path: &Path) -> String {
        let logical = self.logical(path);
        let home = self.home.as_ref().map(|h| self.logical(h));
        match home.as_ref().and_then(|h| logical.strip_prefix(h).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => "~".into(),
            Some(rest) => safe_text(&format!("~/{}", rest.display())),
            None => safe_text(&logical.display().to_string()),
        }
    }
    fn inside(&self, path: &Path) -> bool {
        self.boundary.as_ref().is_none_or(|r| path.starts_with(r))
    }
    /// A system path such as `/usr/bin` or `/Library/Java`, inside the fixture when set.
    fn system(&self, path: &str) -> PathBuf {
        match &self.boundary {
            Some(root) => root.join(path.trim_start_matches('/')),
            None => PathBuf::from(path),
        }
    }
}

/// What a row is, for counting; the `kind` column is free text (provider or manager).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A command found on PATH.
    Command,
    /// A PATH entry, folder or file that could not be used.
    Setup,
    Toolchain,
    Crate,
    Package,
}

#[derive(Clone, Debug)]
pub struct Row {
    pub class: Class,
    pub name: String,
    /// Provider, manager or kind: the middle column.
    pub kind: String,
    pub status: Status,
    pub detail: String,
    /// Stable identity for selection across refreshes.
    pub key: String,
}

#[derive(Default)]
pub struct Inventory {
    pub path: Vec<Row>,
    pub rust: Vec<Row>,
    pub packages: Vec<Row>,
    pub default_toolchain: Option<String>,
    /// Packages per manager, in display order.
    pub managers: Vec<(String, usize)>,
    pub coverage: Vec<String>,
}
impl Inventory {
    pub fn rows(&self, section: Section) -> &[Row] {
        match section {
            Section::Path => &self.path,
            Section::Rust => &self.rust,
            Section::Packages => &self.packages,
        }
    }
}
pub type Collector = crate::worker::Worker<Inventory, io::Error>;

/// Reads one section's files and folders. A missing file or folder is simply absent; an
/// unreadable folder or an invalid file becomes a Problem row, so the section never
/// silently looks emptier or more settled than the machine is.
struct Reader<'a> {
    context: &'a Context,
    problems: RefCell<Vec<Row>>,
}
impl std::ops::Deref for Reader<'_> {
    type Target = Context;
    fn deref(&self) -> &Context {
        self.context
    }
}
impl<'a> Reader<'a> {
    fn new(context: &'a Context) -> Self {
        Self {
            context,
            problems: RefCell::new(vec![]),
        }
    }
    fn problem(&self, path: &Path, kind: &str, why: &str) {
        let key = format!("problem:{}", path.display());
        let mut problems = self.problems.borrow_mut();
        if problems.iter().any(|r| r.key == key) {
            return;
        }
        let short = self.short(path);
        problems.push(Row {
            class: Class::Setup,
            name: short.clone(),
            kind: kind.into(),
            status: Status::problem(why),
            detail: format!(
                "{short}\n\nDen could not use this: {why}. What it describes is missing here, so this section may be incomplete."
            ),
            key,
        });
    }
    fn text(&self, path: &Path) -> Option<String> {
        match read_file(self.boundary.as_deref(), path, FILE_LIMIT) {
            Read::Text(t) => Some(t),
            Read::Absent => None,
            Read::Problem(why) => {
                self.problem(path, "File", why);
                None
            }
        }
    }
    fn toml<T: serde::de::DeserializeOwned>(&self, path: &Path) -> Option<T> {
        let text = self.text(path)?;
        toml::from_str(&text)
            .map_err(|_| self.problem(path, "File", "not valid TOML"))
            .ok()
    }
    fn json(&self, path: &Path) -> Option<serde_json::Value> {
        let text = self.text(path)?;
        serde_json::from_str(&text)
            .map_err(|_| self.problem(path, "File", "not valid JSON"))
            .ok()
    }
    /// Entry names of a folder, sorted and bounded; empty when it is missing or outside.
    fn names(&self, dir: &Path) -> Vec<String> {
        #[cfg(unix)]
        {
            let resolved = match filesystem::resolve(dir) {
                Ok(resolved) => resolved,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return vec![],
                Err(_) => {
                    self.problem(dir, "Folder", "the folder cannot be followed");
                    return vec![];
                }
            };
            if !self.inside(&resolved) {
                return vec![];
            }
            match filesystem::Directory::open(&resolved).and_then(|d| d.names(NAMES_LIMIT)) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => vec![],
                Err(_) => {
                    self.problem(dir, "Folder", "the folder cannot be listed");
                    vec![]
                }
                Ok((names, limited)) => {
                    if limited {
                        self.problem(
                            dir,
                            "Folder",
                            "more than 1,024 entries; the rest are not shown",
                        );
                    }
                    names
                        .into_iter()
                        .filter_map(|n| n.into_string().ok())
                        .filter(|n| !n.starts_with('.'))
                        .collect()
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = dir;
            vec![]
        }
    }
    /// The section's rows with its problems first.
    fn finish(self, rows: Vec<Row>) -> Vec<Row> {
        let mut all = self.problems.into_inner();
        all.extend(rows);
        all
    }
}

pub fn discover(context: &Context, cancel: &AtomicBool) -> io::Result<Inventory> {
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("collection cancelled"));
    }
    let reader = Reader::new(context);
    let (rows, default_toolchain) = rust(&reader);
    let rust = reader.finish(rows);
    let reader = Reader::new(context);
    let (rows, npm) = on_path(&reader, default_toolchain.clone());
    let path = reader.finish(rows);
    let reader = Reader::new(context);
    let (rows, managers) = packages(&reader, npm);
    let packages = reader.finish(rows);
    Ok(Inventory {
        path,
        rust,
        packages,
        default_toolchain,
        managers,
        coverage: vec![
            "Read: PATH folders, rustup settings, ~/.cargo/.crates2.json and package metadata (package.json, uv receipts, dist-info folder names).".into(),
            "Nothing is run: versions come from install paths and metadata, so a program installed directly in a system folder shows no version.".into(),
            "Not checked: newer versions (that needs registries), disk use, and per-folder selections such as rust-toolchain.toml, .nvmrc or .python-version.".into(),
        ],
    })
}

// ---------------------------------------------------------------------------------------
// On PATH

/// One copy of a command on PATH.
struct Found {
    path: PathBuf,
    real: PathBuf,
    provider: String,
    version: Option<String>,
    system: bool,
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    path.is_file()
}

/// The absolute PATH folders, without repeats, and the relative entries.
fn folders(context: &Context) -> (Vec<PathBuf>, Vec<String>) {
    let mut dirs: Vec<PathBuf> = vec![];
    let mut relative = vec![];
    for p in &context.path {
        if !p.is_absolute() {
            relative.push(safe_text(&p.display().to_string()));
        } else if !dirs.contains(p) {
            dirs.push(p.clone());
        }
    }
    (dirs, relative)
}

/// Returns the rows and the folder holding the winning `npm` (for npm's global prefix).
fn on_path(context: &Reader<'_>, toolchain: Option<String>) -> (Vec<Row>, Option<PathBuf>) {
    let (dirs, relative) = folders(context);
    let mut rows = vec![];
    let mut npm = None;
    for entry in &relative {
        rows.push(Row {
            class: Class::Setup,
            name: if entry.is_empty() { "(empty)".into() } else { entry.clone() },
            kind: "PATH entry".into(),
            status: Status::problem("relative PATH entry"),
            detail: format!(
                "PATH entry '{entry}'\n\nA relative entry makes the shell look for commands in whatever folder you are in, so a file there can run in place of a real command. Use absolute folders only."
            ),
            key: format!("relative:{entry}"),
        });
    }
    // Manager folders that hold programs but are not on PATH.
    let home = context.home.clone();
    let mut managed: Vec<(PathBuf, &str)> = vec![];
    if let Some(c) = &context.cargo_home {
        managed.push((c.join("bin"), "cargo and rustup"));
    }
    if let Some(g) = &context.go_bin {
        managed.push((g.clone(), "go install"));
    }
    if let Some(b) = &context.bun_install {
        managed.push((b.join("bin"), "bun"));
    }
    if let Some(h) = &home {
        managed.push((h.join(".local/bin"), "uv tool and pipx"));
    }
    for (dir, who) in managed {
        let programs = context.names(&dir);
        if programs.is_empty() || dirs.iter().any(|d| same_dir(d, &dir)) {
            continue;
        }
        let short = context.short(&dir);
        rows.push(Row {
            class: Class::Setup,
            name: short.clone(),
            kind: "Folder".into(),
            status: Status::problem("not on PATH"),
            detail: format!(
                "{short}\n\nPrograms installed with {who} go here, but the folder is not on PATH, so the shell does not find them by name: {}.",
                programs
                    .iter()
                    .take(12)
                    .map(|n| safe_text(n))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            key: format!("folder:{}", dir.display()),
        });
    }
    for name in COMMANDS {
        let copies: Vec<Found> = dirs
            .iter()
            .map(|d| d.join(name))
            .filter(|p| std::fs::symlink_metadata(p).is_ok() && executable(p))
            .filter_map(|p| {
                let real = filesystem::resolve(&p).ok()?;
                context.inside(&real).then_some((p, real))
            })
            .map(|(path, real)| {
                let (provider, version, system) = provider(context, name, &path, &real, &toolchain);
                Found {
                    path,
                    real,
                    provider,
                    version,
                    system,
                }
            })
            .collect();
        let Some(winner) = copies.first() else {
            continue;
        };
        if *name == "npm" {
            npm = winner.path.parent().map(Path::to_path_buf);
        }
        rows.push(command_row(context, name, &copies));
    }
    (rows, npm)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (filesystem::resolve(a), filesystem::resolve(b)),
            (Ok(x), Ok(y)) if x == y
        )
}

/// Who provides this copy, its version when the path shows it, and whether it is the OS's.
fn provider(
    context: &Reader<'_>,
    name: &str,
    path: &Path,
    real: &Path,
    toolchain: &Option<String>,
) -> (String, Option<String>, bool) {
    let real_l = context.logical(real);
    let dir_l = context.logical(path.parent().unwrap_or(path));
    let home = context.home.as_ref().map(|h| context.logical(h));
    let parts: Vec<String> = real_l
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let after = |key: &[&str]| -> Option<String> {
        parts
            .windows(key.len() + 1)
            .find(|w| w[..key.len()].iter().zip(key).all(|(a, b)| a == b))
            .map(|w| safe_text(&w[key.len()]))
    };
    let in_home = |rel: &str| {
        home.as_ref()
            .is_some_and(|h| real_l.starts_with(h.join(rel)))
    };
    let dir_is = |rel: &str| home.as_ref().is_some_and(|h| dir_l == h.join(rel));
    // From where the file really lives.
    if let Some(i) = parts.iter().position(|p| p == "Cellar")
        && parts.len() > i + 2
    {
        return ("Homebrew".into(), Some(safe_text(&parts[i + 2])), false);
    }
    if in_home(".local/share/mise/installs") {
        return (
            "mise".into(),
            parts
                .iter()
                .position(|p| p == "installs")
                .and_then(|i| parts.get(i + 2))
                .map(|v| safe_text(v)),
            false,
        );
    }
    if in_home(".asdf/installs") {
        return (
            "asdf".into(),
            parts
                .iter()
                .position(|p| p == "installs")
                .and_then(|i| parts.get(i + 2))
                .map(|v| safe_text(v)),
            false,
        );
    }
    if in_home(".nvm/versions") {
        return ("nvm".into(), after(&["versions", "node"]), false);
    }
    if in_home(".pyenv/versions") {
        return ("pyenv".into(), after(&["versions"]), false);
    }
    if let Some(uv) = &context.uv_python
        && real.starts_with(uv)
    {
        let dir = after(&["python"]).unwrap_or_default();
        return ("uv python".into(), uv_python_version(&dir), false);
    }
    if let Some(tools) = &context.uv_tools
        && real.starts_with(tools)
    {
        return ("uv tool".into(), None, false);
    }
    // A Node package's script (npm, npx, corepack): its package.json has the version.
    let node_version = parts
        .iter()
        .rposition(|p| p == "node_modules")
        .and_then(|i| {
            let scoped = parts.get(i + 1).is_some_and(|p| p.starts_with('@'));
            let depth = i + if scoped { 3 } else { 2 };
            let up = parts.len().checked_sub(depth)?;
            let keep = real.components().count().checked_sub(up)?;
            let dir: PathBuf = real.components().take(keep).collect();
            package_version(context, &dir)
        });
    // From the PATH folder.
    if let Some(c) = &context.cargo_home
        && dir_l == context.logical(&c.join("bin"))
    {
        return if RUSTUP_PROXIES.contains(&name) {
            (
                "rustup".into(),
                toolchain
                    .as_ref()
                    .map(|t| format!("{} (default toolchain)", split_host(t).0)),
                false,
            )
        } else {
            ("cargo install".into(), None, false)
        };
    }
    if dir_is(".local/share/mise/shims") {
        return ("mise shim".into(), None, false);
    }
    if dir_is(".asdf/shims") {
        return ("asdf shim".into(), None, false);
    }
    if dir_is(".pyenv/shims") {
        return ("pyenv shim".into(), None, false);
    }
    if dir_is(".volta/bin") {
        return ("Volta shim".into(), None, false);
    }
    if dir_is(".bun/bin") {
        return ("bun".into(), None, false);
    }
    if context
        .go_bin
        .as_ref()
        .is_some_and(|g| dir_l == context.logical(g))
    {
        return ("go install".into(), None, false);
    }
    let system = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        .iter()
        .any(|s| dir_l == Path::new(s))
        || real_l.starts_with("/System");
    if system {
        let os = if cfg!(target_os = "macos") || context.boundary.is_some() {
            "macOS system"
        } else {
            "system"
        };
        return (os.into(), None, true);
    }
    if dir_l.starts_with("/opt/homebrew") {
        return ("Homebrew".into(), node_version, false);
    }
    (context.short(&dir_l), node_version, false)
}

/// `cpython-3.13.14-macos-aarch64-none` → `3.13.14`.
fn uv_python_version(dir: &str) -> Option<String> {
    let mut parts = dir.splitn(3, '-');
    parts.next()?;
    parts.next().map(safe_text)
}

fn command_row(context: &Reader<'_>, name: &str, copies: &[Found]) -> Row {
    let winner = &copies[0];
    let mut problems = vec![];
    let real = context.logical(&winner.real);
    let stub = |path: &str| real == Path::new(path);
    let macos = cfg!(target_os = "macos") || context.boundary.is_some();
    if macos
        && (stub("/usr/bin/java") || stub("/usr/bin/javac"))
        && !context.java_home
        && context
            .names(&context.system("/Library/Java/JavaVirtualMachines"))
            .is_empty()
    {
        problems.push("macOS stub: no JDK is installed".to_owned());
    }
    if macos
        && (stub("/usr/bin/python3") || stub("/usr/bin/pip3"))
        && !context
            .system("/Library/Developer/CommandLineTools")
            .exists()
        && !context.system("/Applications/Xcode.app").exists()
    {
        problems.push("macOS stub: the Command Line Tools are not installed".to_owned());
    }
    if winner.system
        && let Some(hidden) = copies.iter().skip(1).find(|c| !c.system)
    {
        problems.push(format!(
            "the system copy hides {}'s (PATH order)",
            hidden.provider
        ));
    }
    let version = winner.version.clone();
    let mut lines = vec![
        safe_text(name),
        String::new(),
        format!("Runs: {}", context.short(&winner.path)),
    ];
    if winner.real != winner.path {
        lines.push(format!("Links to: {}", context.short(&winner.real)));
    }
    lines.push(format!("From: {}", winner.provider));
    lines.push(format!(
        "Version: {}",
        version
            .as_deref()
            .unwrap_or("unknown (not shown by its path; Den does not run it)")
    ));
    if copies.len() > 1 {
        lines.push(String::new());
        lines.push(format!("Hidden by it ({}):", copies.len() - 1));
        for c in &copies[1..] {
            lines.push(format!(
                "  {} · {}{}",
                context.short(&c.path),
                c.provider,
                c.version
                    .as_ref()
                    .map_or_else(String::new, |v| format!(" · {v}"))
            ));
        }
    }
    if !problems.is_empty() {
        lines.push(String::new());
        lines.push(format!("Problems: {}", problems.join("; ")));
    }
    let short_version = version
        .as_deref()
        .map(|v| v.split(" (").next().unwrap_or(v).to_owned());
    let status = match problems.first() {
        Some(p) => Status::problem(p.as_str()),
        None => {
            let mut s = short_version
                .as_deref()
                .unwrap_or("version unknown")
                .to_owned();
            if copies.len() > 1 {
                s.push_str(&format!(" · hides {}", copies.len() - 1));
            }
            Status::ready(s)
        }
    };
    Row {
        class: Class::Command,
        name: safe_text(name),
        kind: winner.provider.clone(),
        status,
        detail: lines.join("\n"),
        key: format!("command:{name}"),
    }
}

// ---------------------------------------------------------------------------------------
// Rust

#[derive(serde::Deserialize, Default)]
struct RustupSettings {
    default_toolchain: Option<String>,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

fn settings(context: &Reader<'_>) -> RustupSettings {
    context
        .rustup_home
        .as_ref()
        .and_then(|h| context.toml(&h.join("settings.toml")))
        .unwrap_or_default()
}

/// `1.88.0-aarch64-apple-darwin` → (`1.88.0`, `aarch64-apple-darwin`).
pub fn split_host(toolchain: &str) -> (&str, Option<&str>) {
    const ARCHES: &[&str] = &[
        "aarch64",
        "x86_64",
        "i686",
        "i586",
        "arm",
        "armv7",
        "riscv64gc",
        "powerpc64le",
        "s390x",
        "loongarch64",
    ];
    ARCHES
        .iter()
        .filter_map(|a| toolchain.find(&format!("-{a}-")))
        .min()
        .map_or((toolchain, None), |i| {
            (&toolchain[..i], Some(&toolchain[i + 1..]))
        })
}

/// The installed toolchain a short name such as `stable` selects.
fn toolchain_matches(dir: &str, name: &str) -> bool {
    dir == name || dir.strip_prefix(name).is_some_and(|r| r.starts_with('-'))
}

fn rust(context: &Reader<'_>) -> (Vec<Row>, Option<String>) {
    let mut rows = vec![];
    let settings = settings(context);
    let default = settings.default_toolchain.clone();
    if let Some(home) = &context.rustup_home {
        let base = home.join("toolchains");
        for name in context.names(&base) {
            let is_default = default
                .as_deref()
                .is_some_and(|d| toolchain_matches(&name, d));
            let overrides: Vec<String> = settings
                .overrides
                .iter()
                .filter(|(_, t)| toolchain_matches(&name, t))
                .map(|(dir, _)| context.short(Path::new(dir)))
                .collect();
            let rustlib = base.join(&name).join("lib/rustlib");
            let components: Vec<String> = context
                .text(&rustlib.join("components"))
                .map(|t| t.lines().take(64).map(safe_text).collect())
                .unwrap_or_default();
            let targets: Vec<String> = context
                .names(&rustlib)
                .into_iter()
                .filter(|n| {
                    n.matches('-').count() >= 2
                        && !n.starts_with("manifest-")
                        && !n.starts_with("multirust")
                        && !n.starts_with("rust-installer")
                })
                .map(|n| safe_text(&n))
                .collect();
            let (short, host) = split_host(&name);
            let components: Vec<String> = components
                .into_iter()
                .map(|c| match host {
                    Some(h) => c
                        .strip_suffix(&format!("-{h}"))
                        .map_or(c.clone(), str::to_owned),
                    None => c,
                })
                .collect();
            let mut lines = vec![safe_text(&name), String::new()];
            if let Some(h) = host {
                lines.push(format!("Host: {}", safe_text(h)));
            }
            lines.push(format!(
                "Default: {}",
                if is_default {
                    "yes (rustup default)"
                } else {
                    "no"
                }
            ));
            if !overrides.is_empty() {
                lines.push(format!("Folder overrides: {}", overrides.join(", ")));
            }
            if !targets.is_empty() {
                lines.push(format!("Targets: {}", targets.join(", ")));
            }
            if !components.is_empty() {
                lines.push(format!("Components: {}", components.join(", ")));
            }
            if !is_default && overrides.is_empty() {
                lines.push(String::new());
                lines.push("Not the default and no folder override selects it. A project's rust-toolchain.toml may still pin it; Den does not search for those.".into());
            }
            let status = if is_default {
                Status::ready("default")
            } else if !overrides.is_empty() {
                Status::ready(format!("override for {}", count(overrides.len(), "folder")))
            } else {
                Status::installed()
            };
            rows.push(Row {
                class: Class::Toolchain,
                name: safe_text(short),
                kind: "Toolchain".into(),
                status,
                detail: lines.join("\n"),
                key: format!("toolchain:{name}"),
            });
        }
        // The default first, then by name.
        rows.sort_by_key(|r| r.status != Status::ready("default"));
    }
    if let Some(cargo) = &context.cargo_home {
        rows.extend(crates(context, cargo));
    }
    (rows, default.map(|d| safe_text(split_host(&d).0)))
}

fn crates(context: &Reader<'_>, cargo: &Path) -> Vec<Row> {
    let Some(json) = context.json(&cargo.join(".crates2.json")) else {
        return vec![];
    };
    let Some(installs) = json.get("installs").and_then(|i| i.as_object()) else {
        return vec![];
    };
    let bin_dir = cargo.join("bin");
    let present = context.names(&bin_dir);
    let mut rows = vec![];
    for (id, entry) in installs.iter().take(NAMES_LIMIT) {
        // "name version (source)"
        let mut parts = id.splitn(3, ' ');
        let (Some(name), Some(version)) = (parts.next(), parts.next()) else {
            continue;
        };
        let source = parts.next().unwrap_or("").trim_matches(['(', ')']);
        let source = if source == "registry+https://github.com/rust-lang/crates.io-index"
            || source.starts_with("sparse+https://index.crates.io")
        {
            "crates.io"
        } else if source.starts_with("registry+") || source.starts_with("sparse+") {
            "another registry (URL withheld)"
        } else if source.starts_with("git+") {
            "git (URL withheld)"
        } else if source.starts_with("path+") {
            "a local folder (path withheld)"
        } else {
            "unknown"
        };
        let list = |key: &str| -> Vec<String> {
            entry
                .get(key)
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .take(64)
                        .map(safe_text)
                        .collect()
                })
                .unwrap_or_default()
        };
        let bins = list("bins");
        let missing: Vec<&String> = bins.iter().filter(|b| !present.contains(b)).collect();
        let rustc = entry
            .get("rustc")
            .and_then(|v| v.as_str())
            .and_then(|r| r.lines().next())
            .map(safe_text);
        let mut lines = vec![
            safe_text(name),
            String::new(),
            format!("Version: {}", safe_text(version)),
            format!("Source: {source}"),
            format!(
                "Programs: {}",
                if bins.is_empty() {
                    "none".into()
                } else {
                    bins.join(", ")
                }
            ),
        ];
        let features = list("features");
        if !features.is_empty() {
            lines.push(format!("Features: {}", features.join(", ")));
        }
        if entry.get("all_features").and_then(|v| v.as_bool()) == Some(true) {
            lines.push("Features: all".into());
        }
        if let Some(profile) = entry.get("profile").and_then(|v| v.as_str()) {
            lines.push(format!("Profile: {}", safe_text(profile)));
        }
        if let Some(r) = &rustc {
            lines.push(format!("Built with: {r}"));
        }
        let status = if missing.is_empty() {
            Status::ready(safe_text(version))
        } else {
            let names: Vec<_> = missing.iter().map(|s| s.as_str()).collect();
            lines.push(String::new());
            lines.push(format!(
                "Problems: recorded as installed, but missing from {}: {}",
                context.short(&bin_dir),
                names.join(", ")
            ));
            Status::problem(format!("program missing: {}", names.join(", ")))
        };
        rows.push(Row {
            class: Class::Crate,
            name: safe_text(name),
            kind: "cargo install".into(),
            status,
            detail: lines.join("\n"),
            key: format!("crate:{name}"),
        });
    }
    rows
}

// ---------------------------------------------------------------------------------------
// Global packages

fn package_version(context: &Reader<'_>, dir: &Path) -> Option<String> {
    let json = context.json(&dir.join("package.json"))?;
    json.get("version")?.as_str().map(safe_text)
}

/// The version from `<name>-<version>.dist-info` in a virtual environment.
fn dist_version(
    context: &Reader<'_>,
    venv: &Path,
    package: &str,
) -> (Option<String>, Option<String>) {
    let normal = package.to_ascii_lowercase().replace(['-', '.'], "_");
    for lib in context.names(&venv.join("lib")) {
        if !lib.starts_with("python") {
            continue;
        }
        let site = venv.join("lib").join(&lib).join("site-packages");
        for entry in context.names(&site) {
            let Some(stem) = entry.strip_suffix(".dist-info") else {
                continue;
            };
            if let Some((n, v)) = stem.rsplit_once('-')
                && n.to_ascii_lowercase().replace(['-', '.'], "_") == normal
            {
                return (Some(safe_text(v)), Some(safe_text(&lib)));
            }
        }
        return (None, Some(safe_text(&lib)));
    }
    (None, None)
}

fn package_row(name: &str, manager: &str, version: Option<String>, mut lines: Vec<String>) -> Row {
    let mut detail = vec![
        safe_text(name),
        String::new(),
        format!("Installed with: {manager}"),
    ];
    detail.push(format!(
        "Version: {}",
        version.as_deref().unwrap_or("unknown")
    ));
    detail.append(&mut lines);
    Row {
        class: Class::Package,
        name: safe_text(name),
        kind: manager.into(),
        status: Status::ready(version.as_deref().unwrap_or("version unknown")),
        detail: detail.join("\n"),
        key: format!("{manager}:{name}"),
    }
}

fn packages(context: &Reader<'_>, npm_dir: Option<PathBuf>) -> (Vec<Row>, Vec<(String, usize)>) {
    let mut rows = vec![];
    let mut managers = vec![];
    let mut add = |manager: &str, found: Vec<Row>| {
        if !found.is_empty() {
            managers.push((manager.to_owned(), found.len()));
            rows.extend(found);
        }
    };

    // npm -g: <prefix>/lib/node_modules, with @scope folders.
    let prefix = context
        .npm_prefix
        .clone()
        .or_else(|| npm_dir.and_then(|d| d.parent().map(Path::to_path_buf)));
    if let Some(prefix) = prefix {
        let modules = prefix.join("lib/node_modules");
        let mut found = vec![];
        for name in context.names(&modules) {
            let dirs: Vec<(String, PathBuf)> = if name.starts_with('@') {
                context
                    .names(&modules.join(&name))
                    .into_iter()
                    .map(|n| (format!("{name}/{n}"), modules.join(&name).join(n)))
                    .collect()
            } else {
                vec![(name.clone(), modules.join(&name))]
            };
            for (name, dir) in dirs {
                let version = package_version(context, &dir);
                found.push(package_row(
                    &name,
                    "npm -g",
                    version,
                    vec![format!("Folder: {}", context.short(&dir))],
                ));
            }
        }
        add("npm -g", found);
    }

    // uv tool: one folder per tool with a receipt.
    if let Some(tools) = &context.uv_tools {
        let mut found = vec![];
        for name in context.names(tools) {
            let dir = tools.join(&name);
            let receipt: Option<toml::Value> = context.toml(&dir.join("uv-receipt.toml"));
            let Some(receipt) = receipt else {
                continue;
            };
            let commands: Vec<String> = receipt
                .get("tool")
                .and_then(|t| t.get("entrypoints"))
                .and_then(|e| e.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|e| e.get("name")?.as_str())
                        .take(64)
                        .map(safe_text)
                        .collect()
                })
                .unwrap_or_default();
            let (version, python) = dist_version(context, &dir, &name);
            let mut lines = vec![format!(
                "Commands: {}",
                if commands.is_empty() {
                    "none".into()
                } else {
                    commands.join(", ")
                }
            )];
            if let Some(p) = python {
                lines.push(format!("Python: {p}"));
            }
            found.push(package_row(&name, "uv tool", version, lines));
        }
        add("uv tool", found);
    }

    // uv python: real folders only; `cpython-3.13-…` links point at the newest patch.
    if let Some(dir) = &context.uv_python {
        let mut found = vec![];
        for name in context.names(dir) {
            let path = dir.join(&name);
            let is_link =
                std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
            if is_link || !path.is_dir() || !name.contains('-') {
                continue;
            }
            let version = uv_python_version(&name);
            let short = name.splitn(3, '-').take(2).collect::<Vec<_>>().join(" ");
            let mut row = package_row(
                &short,
                "uv python",
                version,
                vec![format!("Folder: {}", context.short(&path))],
            );
            row.key = format!("uv python:{name}");
            found.push(row);
        }
        add("uv python", found);
    }

    // pipx: one virtual environment per package.
    if let Some(home) = &context.pipx_home {
        let venvs = home.join("venvs");
        let found = context
            .names(&venvs)
            .into_iter()
            .map(|name| {
                let (version, python) = dist_version(context, &venvs.join(&name), &name);
                package_row(
                    &name,
                    "pipx",
                    version,
                    python.map(|p| format!("Python: {p}")).into_iter().collect(),
                )
            })
            .collect();
        add("pipx", found);
    }

    // bun: the global package.json lists what was added.
    if let Some(bun) = &context.bun_install {
        let global = bun.join("install/global");
        let declared: Vec<String> = context
            .json(&global.join("package.json"))
            .and_then(|j| {
                j.get("dependencies")?
                    .as_object()
                    .map(|d| d.keys().take(NAMES_LIMIT).cloned().collect())
            })
            .unwrap_or_default();
        let found = declared
            .iter()
            .map(|name| {
                let version = package_version(context, &global.join("node_modules").join(name));
                package_row(name, "bun", version, vec![])
            })
            .collect();
        add("bun", found);
    }

    // go install: program names only; versions need `go version -m`, which Den does not run.
    if let Some(bin) = &context.go_bin {
        let found = context
            .names(bin)
            .into_iter()
            .filter(|n| executable(&bin.join(n)))
            .map(|name| {
                package_row(
                    &name,
                    "go install",
                    None,
                    vec![format!("Program: {}", context.short(&bin.join(&name)))],
                )
            })
            .collect();
        add("go install", found);
    }
    (rows, managers)
}
