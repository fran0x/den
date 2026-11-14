//! Write actions: off by default, confirmed one at a time, and limited to a closed list.
//!
//! `den` is always read-only. Actions can run only in `dew`, which creates the
//! [`Permit`] and shows WRITE in the top bar; each one runs on Enter, after its dry run.
//! Every action is a fixed command line built from a checked name: no shell, no sudo,
//! stdin closed (a password prompt fails instead of waiting), and never `--force`,
//! `--ignore-dependencies` or `--zap`. Toolchain programs run by the full path Den found
//! on PATH, so the command shown is the one that runs. `den` has no actions at all: no
//! menu, not even commands to copy.
use crate::{
    areas::homebrew::{CollectionError, CommandOutput},
    inventory::safe_text,
};
use std::{
    ffi::OsStr,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant, SystemTime},
};

/// How long a stopping action gets to clean up before it is killed.
pub const GRACE: Duration = Duration::from_secs(10);
/// Lines of output kept for the result dialog.
const OUTPUT_LINES: usize = 200;

/// A Homebrew formula or cask, by a name checked before any command is built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    name: String,
    cask: bool,
}
impl Package {
    /// `None` unless the name looks like a Homebrew name: `node`, `python@3.13`,
    /// `user/tap/tool`. A leading `-` (an option) and any other character are refused.
    pub fn new(name: &str, cask: bool) -> Option<Self> {
        let segments: Vec<_> = name.split('/').collect();
        let valid = name.len() <= 128
            && matches!(segments.len(), 1 | 3)
            && segments.iter().all(|s| {
                s.starts_with(|c: char| c.is_ascii_alphanumeric())
                    && s.chars().all(|c| {
                        c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '+' | '-')
                    })
            });
        valid.then(|| Self {
            name: name.to_owned(),
            cask,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    fn kind(&self) -> &'static str {
        if self.cask { "--cask" } else { "--formula" }
    }
}

/// A Docker container, image or volume, by an ID or name checked before any command is
/// built, with the name shown for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockerObject {
    target: String,
    label: String,
}
impl DockerObject {
    /// A container or untagged image by ID: hex, optionally `sha256:`-prefixed.
    pub fn id(id: &str, label: &str) -> Option<Self> {
        let hex = id.strip_prefix("sha256:").unwrap_or(id);
        ((12..=64).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Self::with(id, label))
    }
    /// A tagged image: `repo:tag`, `registry:5000/team/app:1.2`.
    pub fn image(reference: &str, label: &str) -> Option<Self> {
        let valid = reference.len() <= 255
            && reference.starts_with(|c: char| c.is_ascii_alphanumeric())
            && reference.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '@')
            });
        valid.then(|| Self::with(reference, label))
    }
    /// A volume, by Docker's own naming rule.
    pub fn volume(name: &str, label: &str) -> Option<Self> {
        let valid = (2..=255).contains(&name.len())
            && name.starts_with(|c: char| c.is_ascii_alphanumeric())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        valid.then(|| Self::with(name, label))
    }
    fn with(target: &str, label: &str) -> Self {
        Self {
            target: target.to_owned(),
            label: safe_text(label),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// A toolchain program (`rustup`, `cargo`) by the full path of the copy that wins on PATH.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    path: PathBuf,
    name: &'static str,
}
impl Program {
    /// `None` unless the path is absolute and names exactly this program.
    pub fn found(path: &Path, name: &'static str) -> Option<Self> {
        (path.is_absolute() && path.file_name() == Some(OsStr::new(name))).then(|| Self {
            path: path.to_owned(),
            name,
        })
    }
    /// For an action that cannot run because the program is not on PATH: run by name, it
    /// would fail, and the menu marks it blocked anyway.
    pub fn missing(name: &'static str) -> Self {
        Self {
            path: PathBuf::from(name),
            name,
        }
    }
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// A rustup toolchain, a crate or a global package, by a name checked before any command
/// is built, with the name shown for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name {
    target: String,
    label: String,
}
impl Name {
    /// A toolchain as rustup names its folder: `stable-aarch64-apple-darwin`,
    /// `nightly-2026-01-01-x86_64-unknown-linux-gnu`, `1.88.0-…`.
    pub fn toolchain(name: &str, label: &str) -> Option<Self> {
        let valid = name.len() <= 128
            && name.starts_with(|c: char| c.is_ascii_alphanumeric())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        valid.then(|| Self::with(name, label))
    }
    /// A crate, by crates.io's rule: a letter first, then letters, digits, `_` or `-`.
    pub fn krate(name: &str) -> Option<Self> {
        let valid = name.len() <= 64
            && name.starts_with(|c: char| c.is_ascii_alphabetic())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
        valid.then(|| Self::with(name, name))
    }
    /// An npm package: `prettier`, `@scope/name`, by npm's rule (lowercase, URL-safe).
    pub fn npm(name: &str) -> Option<Self> {
        let part = |s: &str| {
            s.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                && s.chars().all(|c| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || matches!(c, '.' | '_' | '~' | '-')
                })
        };
        let valid = name.len() <= 214
            && match name.strip_prefix('@').map(|rest| rest.split_once('/')) {
                Some(Some((scope, package))) => part(scope) && part(package),
                Some(None) => false,
                None => part(name),
            };
        valid.then(|| Self::with(name, name))
    }
    /// A Python package (uv tool, pipx), by PyPI's rule: letters, digits, `.`, `_` and `-`,
    /// starting and ending with a letter or digit.
    pub fn pypi(name: &str) -> Option<Self> {
        let valid = name.len() <= 128
            && name.starts_with(|c: char| c.is_ascii_alphanumeric())
            && name.ends_with(|c: char| c.is_ascii_alphanumeric())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        valid.then(|| Self::with(name, name))
    }
    fn with(target: &str, label: &str) -> Self {
        Self {
            target: target.to_owned(),
            label: safe_text(label),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Which area an action changes: it is read again afterwards.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Homebrew,
    Docker,
    Toolchains,
}

/// Everything Den can change. Adding an action means adding a variant here, with its
/// command line, preview and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// `brew update`: refresh Homebrew's package data; changes no installed package.
    BrewUpdate,
    BrewUpgrade(Package),
    BrewUninstall(Package),
    /// Remove dependencies nothing needs any more.
    BrewAutoremove,
    /// Remove old versions and downloads.
    BrewCleanup,
    DockerStopContainer(DockerObject),
    /// A container that is not running; Docker refuses a running one without `-f`.
    DockerRemoveContainer(DockerObject),
    /// Images nothing uses; Docker refuses one a container uses without `-f`.
    DockerRemoveImages(Vec<DockerObject>),
    /// A volume nothing uses; Docker refuses one in use.
    DockerRemoveVolume(DockerObject),
    /// `rustup update` for one toolchain; rustup itself is not updated.
    RustupUpdate(Program, Name),
    /// Not the default toolchain, and no folder override uses it.
    RustupUninstall(Program, Name),
    /// `cargo install --locked`: rebuilds a crates.io crate when a newer version exists.
    CargoInstall(Program, Name),
    CargoUninstall(Program, Name),
    /// `npm update -g`: the newest version npm allows; runs the package's install scripts.
    NpmUpdate(Program, Name),
    NpmUninstall(Program, Name),
    UvToolUpgrade(Program, Name),
    UvToolUninstall(Program, Name),
    PipxUpgrade(Program, Name),
    PipxUninstall(Program, Name),
}
impl Action {
    pub fn area(&self) -> Area {
        match self {
            Self::BrewUpdate
            | Self::BrewUpgrade(_)
            | Self::BrewUninstall(_)
            | Self::BrewAutoremove
            | Self::BrewCleanup => Area::Homebrew,
            Self::DockerStopContainer(_)
            | Self::DockerRemoveContainer(_)
            | Self::DockerRemoveImages(_)
            | Self::DockerRemoveVolume(_) => Area::Docker,
            Self::RustupUpdate(..)
            | Self::RustupUninstall(..)
            | Self::CargoInstall(..)
            | Self::CargoUninstall(..)
            | Self::NpmUpdate(..)
            | Self::NpmUninstall(..)
            | Self::UvToolUpgrade(..)
            | Self::UvToolUninstall(..)
            | Self::PipxUpgrade(..)
            | Self::PipxUninstall(..) => Area::Toolchains,
        }
    }
    fn toolchain_program(&self) -> Option<&Program> {
        match self {
            Self::RustupUpdate(p, _)
            | Self::RustupUninstall(p, _)
            | Self::CargoInstall(p, _)
            | Self::CargoUninstall(p, _)
            | Self::NpmUpdate(p, _)
            | Self::NpmUninstall(p, _)
            | Self::UvToolUpgrade(p, _)
            | Self::UvToolUninstall(p, _)
            | Self::PipxUpgrade(p, _)
            | Self::PipxUninstall(p, _) => Some(p),
            _ => None,
        }
    }
    /// What runs: `brew`, `docker`, or a toolchain program by its full path.
    fn executable(&self) -> &OsStr {
        match (self.area(), self.toolchain_program()) {
            (_, Some(p)) => p.path.as_os_str(),
            (Area::Docker, None) => OsStr::new("docker"),
            _ => OsStr::new("brew"),
        }
    }
    /// The program as shown in the dialog and the log.
    pub fn program(&self) -> String {
        safe_text(&Path::new(self.executable()).display().to_string())
    }
    pub fn args(&self) -> Vec<String> {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let with = |w: &[&str], o: &DockerObject| {
            let mut v = words(w);
            v.push(o.target.clone());
            v
        };
        match self {
            Self::BrewUpdate => words(&["update"]),
            Self::BrewUpgrade(p) => words(&["upgrade", p.kind(), p.name()]),
            Self::BrewUninstall(p) => words(&["uninstall", p.kind(), p.name()]),
            Self::BrewAutoremove => words(&["autoremove"]),
            Self::BrewCleanup => words(&["cleanup"]),
            Self::DockerStopContainer(o) => with(&["container", "stop"], o),
            Self::DockerRemoveContainer(o) => with(&["container", "rm"], o),
            Self::DockerRemoveImages(images) => {
                let mut v = words(&["image", "rm"]);
                v.extend(images.iter().map(|o| o.target.clone()));
                v
            }
            Self::DockerRemoveVolume(o) => with(&["volume", "rm"], o),
            Self::RustupUpdate(_, t) => {
                vec!["update".into(), "--no-self-update".into(), t.target.clone()]
            }
            Self::RustupUninstall(_, t) => {
                vec!["toolchain".into(), "uninstall".into(), t.target.clone()]
            }
            Self::CargoInstall(_, c) => vec!["install".into(), "--locked".into(), c.target.clone()],
            Self::CargoUninstall(_, c) => vec!["uninstall".into(), c.target.clone()],
            Self::NpmUpdate(_, n) => vec!["update".into(), "-g".into(), n.target.clone()],
            Self::NpmUninstall(_, n) => vec!["uninstall".into(), "-g".into(), n.target.clone()],
            Self::UvToolUpgrade(_, n) => vec!["tool".into(), "upgrade".into(), n.target.clone()],
            Self::UvToolUninstall(_, n) => {
                vec!["tool".into(), "uninstall".into(), n.target.clone()]
            }
            Self::PipxUpgrade(_, n) => vec!["upgrade".into(), n.target.clone()],
            Self::PipxUninstall(_, n) => vec!["uninstall".into(), n.target.clone()],
        }
    }
    /// The dry run shown before confirming, when the tool has one.
    pub fn preview_args(&self) -> Option<Vec<String>> {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        match self {
            Self::BrewUpgrade(p) => Some(words(&["upgrade", "--dry-run", p.kind(), p.name()])),
            Self::BrewAutoremove => Some(words(&["autoremove", "--dry-run"])),
            Self::BrewCleanup => Some(words(&["cleanup", "--dry-run"])),
            Self::RustupUpdate(..) => Some(words(&["check", "--no-self-update"])),
            // npm's own dry run: the same command, `--dry-run` before the name.
            Self::NpmUpdate(..) | Self::NpmUninstall(..) => {
                let mut args = self.args();
                args.insert(2, "--dry-run".into());
                Some(args)
            }
            _ => None,
        }
    }
    /// The dry run's exit code means it worked: `rustup check` exits 100 when an update
    /// is available.
    fn preview_succeeded(&self, code: Option<i32>) -> bool {
        code == Some(0) || (matches!(self, Self::RustupUpdate(..)) && code == Some(100))
    }
    /// The command as you would type it, for the dialog, the clipboard and the log.
    pub fn command_line(&self) -> String {
        line(&self.program(), &self.args())
    }
    pub fn preview_line(&self) -> Option<String> {
        self.preview_args().map(|a| line(&self.program(), &a))
    }
    pub fn title(&self) -> String {
        match self {
            Self::BrewUpdate => "Update Homebrew's package data".into(),
            Self::BrewUpgrade(p) => format!("Upgrade {}", p.name()),
            Self::BrewUninstall(p) => format!("Uninstall {}", p.name()),
            Self::BrewAutoremove => "Remove dependencies nothing needs".into(),
            Self::BrewCleanup => "Clean up old versions and downloads".into(),
            Self::DockerStopContainer(o) => format!("Stop {}", o.label),
            Self::DockerRemoveContainer(o) => format!("Remove {}", o.label),
            Self::DockerRemoveImages(images) => match images.as_slice() {
                [one] => format!("Remove {}", one.label),
                many => format!("Remove {} unused untagged images", many.len()),
            },
            Self::DockerRemoveVolume(o) => format!("Remove volume {}", o.label),
            Self::RustupUpdate(_, t) => format!("Update {}", t.label),
            Self::RustupUninstall(_, t) => format!("Uninstall toolchain {}", t.label),
            Self::CargoInstall(_, c) => format!("Upgrade {}", c.label),
            Self::CargoUninstall(_, c) => format!("Uninstall {}", c.label),
            Self::NpmUpdate(_, n) | Self::UvToolUpgrade(_, n) | Self::PipxUpgrade(_, n) => {
                format!("Upgrade {}", n.label)
            }
            Self::NpmUninstall(_, n) | Self::UvToolUninstall(_, n) | Self::PipxUninstall(_, n) => {
                format!("Uninstall {}", n.label)
            }
        }
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(match self {
            // These build or download a lot.
            Self::BrewUpgrade(_) | Self::CargoInstall(..) => 30 * 60,
            _ if self.area() == Area::Docker => 2 * 60,
            _ => 10 * 60,
        })
    }
}

fn line(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(unix)]
fn command(action: &Action, args: &[String]) -> std::process::Command {
    let mut command = std::process::Command::new(action.executable());
    command.args(args);
    match action.area() {
        Area::Docker => {
            command.env("DOCKER_CLI_HINTS", "false");
        }
        Area::Homebrew => {
            command
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env("HOMEBREW_NO_ANALYTICS", "1")
                .env("HOMEBREW_NO_ENV_HINTS", "1")
                .env("HOMEBREW_NO_EMOJI", "1")
                .env("HOMEBREW_NO_COLOR", "1");
        }
        Area::Toolchains => {
            command.env("NO_COLOR", "1");
            match action.toolchain_program().map(Program::name) {
                Some("npm") => {
                    command
                        .env("npm_config_fund", "false")
                        .env("npm_config_audit", "false")
                        .env("npm_config_update_notifier", "false")
                        .env("npm_config_color", "false");
                }
                Some("uv") => {
                    command.env("UV_NO_PROGRESS", "1");
                }
                Some("pipx") => {
                    command.env("USE_EMOJI", "0");
                }
                _ => {
                    command
                        .env("CARGO_TERM_COLOR", "never")
                        .env("CARGO_TERM_PROGRESS_WHEN", "never")
                        .env("RUSTUP_TERM_COLOR", "never");
                }
            }
        }
    }
    command
}

/// Proof that Den runs as `dew`. Running an action needs one.
#[derive(Debug)]
pub struct Permit(());
impl Permit {
    /// A permit when writes are allowed: only the command line creates one, for `dew`.
    pub fn granted(allowed: bool) -> Option<Self> {
        allowed.then_some(Self(()))
    }
}

/// A preview or an action running in the background.
pub type Worker = crate::worker::Worker<CommandOutput, CollectionError>;

/// Starts the action's dry run: read-only, like Den's other Homebrew commands.
pub fn start_preview(action: Action) -> Result<Worker, CollectionError> {
    Worker::start_with(move |cancel| preview(&action, cancel))
}

/// Starts the action itself. Only possible with a [`Permit`]; stopping it gives the
/// process [`GRACE`] to clean up.
pub fn start(action: Action, _permit: &Permit) -> Result<Worker, CollectionError> {
    Worker::start_with(move |cancel| execute(&action, cancel))
}

fn preview(action: &Action, cancel: &AtomicBool) -> Result<CommandOutput, CollectionError> {
    let Some(args) = action.preview_args() else {
        return Err(CollectionError::Unavailable);
    };
    #[cfg(unix)]
    {
        crate::areas::homebrew::run_bounded(
            &mut command(action, &args),
            cancel,
            Duration::from_secs(120),
            1024 * 1024,
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (args, cancel);
        Err(CollectionError::Unavailable)
    }
}

fn execute(action: &Action, cancel: &AtomicBool) -> Result<CommandOutput, CollectionError> {
    #[cfg(unix)]
    {
        crate::areas::homebrew::run_write(
            &mut command(action, &action.args()),
            cancel,
            action.timeout(),
            GRACE,
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (action, cancel);
        Err(CollectionError::Unavailable)
    }
}

/// One entry in the actions menu; `blocked` says why it cannot run.
#[derive(Clone, Debug)]
pub struct Item {
    pub action: Action,
    pub blocked: Option<String>,
    /// What it will do, for actions without a dry run: "Removes … (142 MB)".
    pub note: Option<String>,
}
impl Item {
    pub fn new(action: Action) -> Self {
        Self {
            action,
            blocked: None,
            note: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preview {
    /// The tool has no dry run for this action, or it is blocked.
    None,
    Waiting,
    Ready(String),
    Failed(String),
}

#[derive(Debug)]
pub enum Dialog {
    /// The actions for a package (named in the title) and for Homebrew, and the
    /// selected one's dry run. Enter runs the selected action.
    Menu {
        package: Option<String>,
        items: Vec<Item>,
        selected: usize,
        preview: Preview,
    },
    Running {
        action: Action,
        started: Instant,
        stopping: bool,
    },
    Done {
        action: Action,
        /// "Done", "Failed: exit 1", "Stopped", …
        summary: String,
        output: String,
        log: Result<PathBuf, String>,
    },
}

/// What the terminal loop should start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Preview(Action),
    Run(Action),
}

#[derive(Debug, Default)]
pub struct State {
    permit: Option<Permit>,
    pub dialog: Option<Dialog>,
    pub request: Option<Request>,
    pub cancel_requested: bool,
    /// A one-line message for the dialog footer.
    pub notice: Option<String>,
}
impl State {
    pub fn new(permit: Option<Permit>) -> Self {
        Self {
            permit,
            ..Self::default()
        }
    }
    /// Writes are allowed: this is `dew`.
    pub fn allowed(&self) -> bool {
        self.permit.is_some()
    }
    pub fn permit(&self) -> Option<&Permit> {
        self.permit.as_ref()
    }
    pub fn running(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Running { .. }))
    }
    /// Opens the actions with the first one selected and its dry run started.
    pub fn open(&mut self, package: Option<String>, items: Vec<Item>) {
        self.dialog = Some(Dialog::Menu {
            package,
            items,
            selected: 0,
            preview: Preview::None,
        });
        self.select(0);
    }
    /// Selects an action and starts its dry run, where Homebrew has one. Only dew runs
    /// previews: den never opens the menu.
    pub fn select(&mut self, index: usize) {
        let allowed = self.permit.is_some();
        let Some(Dialog::Menu {
            items,
            selected,
            preview,
            ..
        }) = &mut self.dialog
        else {
            return;
        };
        *selected = index.min(items.len().saturating_sub(1));
        *preview = match items.get(*selected) {
            Some(item)
                if allowed && item.blocked.is_none() && item.action.preview_args().is_some() =>
            {
                self.request = Some(Request::Preview(item.action.clone()));
                Preview::Waiting
            }
            _ => {
                self.request = None;
                Preview::None
            }
        };
        self.notice = None;
    }
    /// A dry run finished; kept only if its action is still the selected one.
    pub fn finish_preview(
        &mut self,
        action: &Action,
        result: Result<CommandOutput, CollectionError>,
    ) {
        if let Some(Dialog::Menu {
            items,
            selected,
            preview,
            ..
        }) = &mut self.dialog
            && items.get(*selected).is_some_and(|i| i.action == *action)
        {
            *preview = match result {
                Ok(output) if action.preview_succeeded(output.code) => {
                    Preview::Ready(tail(&output))
                }
                Ok(output) => Preview::Failed(format!("{}\n{}", exit(output.code), tail(&output))),
                Err(error) => Preview::Failed(failure(&error, action)),
            };
        }
    }
    /// Enter: runs the selected action, only in dew, unblocked, after its dry run.
    pub fn run_selected(&mut self) {
        let Some(Dialog::Menu {
            items,
            selected,
            preview,
            ..
        }) = &self.dialog
        else {
            return;
        };
        let Some(item) = items.get(*selected) else {
            return;
        };
        let refusal = if self.permit.is_none() {
            Some("Writes are off: den is read-only; dew makes changes".into())
        } else if let Some(why) = &item.blocked {
            Some(format!("Not available: {why}"))
        } else if *preview == Preview::Waiting {
            Some("Wait for the preview to finish".into())
        } else {
            None
        };
        if let Some(refusal) = refusal {
            self.notice = Some(refusal);
            return;
        }
        let action = item.action.clone();
        self.request = Some(Request::Run(action.clone()));
        self.notice = None;
        self.dialog = Some(Dialog::Running {
            action,
            started: Instant::now(),
            stopping: false,
        });
    }
    /// Esc or Ctrl-C while running: ask the process to stop.
    pub fn stop(&mut self) {
        if let Some(Dialog::Running { stopping, .. }) = &mut self.dialog {
            *stopping = true;
            self.cancel_requested = true;
        }
    }
    /// Records the result in the log and shows it.
    pub fn finish(&mut self, result: Result<CommandOutput, CollectionError>, log: Option<&Path>) {
        let Some(Dialog::Running { action, .. }) = self.dialog.take() else {
            return;
        };
        self.cancel_requested = false;
        let (summary, output) = match &result {
            Ok(output) if output.code == Some(0) => ("Done".to_owned(), tail(output)),
            Ok(output) => (format!("Failed: {}", exit(output.code)), tail(output)),
            Err(CollectionError::Cancelled) => {
                ("Stopped before it finished".to_owned(), String::new())
            }
            Err(error) => (failure(error, &action), String::new()),
        };
        let log = match log {
            Some(log) => append_log(log, SystemTime::now(), &action, &summary)
                .map(|()| log.to_owned())
                .map_err(|e| format!("not logged ({})", e.kind())),
            None => Err("not logged (no home folder)".into()),
        };
        self.dialog = Some(Dialog::Done {
            action,
            summary,
            output,
            log,
        });
    }
    pub fn close(&mut self) {
        self.dialog = None;
        self.request = None;
        self.notice = None;
    }
}

fn exit(code: Option<i32>) -> String {
    code.map_or_else(|| "stopped by a signal".into(), |c| format!("exit {c}"))
}

fn failure(error: &CollectionError, action: &Action) -> String {
    match error {
        CollectionError::Unavailable => match action.toolchain_program() {
            Some(p) => format!("{} is not on PATH", p.name()),
            None => format!("{} is not on PATH", action.program()),
        },
        CollectionError::Timeout => "Stopped: it took too long".into(),
        CollectionError::OutputLimit => "Stopped: too much output".into(),
        CollectionError::Cancelled => "Stopped".into(),
        CollectionError::Failed(why) => format!("Could not run it: {}", safe_text(why)),
        CollectionError::Invalid(why) => format!("Could not run it: {why}"),
    }
}

/// The last lines of stdout and stderr, made safe for the terminal.
fn tail(output: &CommandOutput) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<_> = text.lines().map(safe_text).collect();
    let skip = lines.len().saturating_sub(OUTPUT_LINES);
    let mut kept = lines[skip..].join("\n");
    if skip > 0 {
        kept = format!("… {skip} earlier lines not shown\n{kept}");
    }
    if kept.trim().is_empty() {
        "(no output)".into()
    } else {
        kept
    }
}

/// Den's own folder: `$XDG_STATE_HOME/den`, or `~/.local/state/den`. It holds the action
/// log and the snapshot saved with `S`, and nothing else.
pub fn state_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|state| state.join("den"))
}

/// `$XDG_STATE_HOME/den/actions.log`, or `~/.local/state/den/actions.log`.
pub fn log_path() -> Option<PathBuf> {
    state_dir().map(|dir| dir.join("actions.log"))
}

/// Replaces the file at `path` with `bytes`, for Den's own files. The new content goes to
/// a private file beside it first and is then renamed over it, so a crash leaves the old
/// file or the new one, never half of one. A link at `path` is refused: not followed, not
/// replaced.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(io::Error::other("no folder to save in"));
        };
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other("it is a link; Den leaves it as it is"));
            }
            Ok(meta) if !meta.is_file() => return Err(io::Error::other("it is not a file")),
            _ => {}
        }
        let temp = dir.join(format!(
            ".{}.{}.tmp",
            name.to_string_lossy(),
            std::process::id()
        ));
        let written = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written
    }
    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        Err(io::Error::other("saving needs macOS or Linux"))
    }
}

/// One line per action: UTC time, command and result; never its output. The folder is
/// private to you and the log is never followed through a link.
pub fn append_log(path: &Path, when: SystemTime, action: &Action, result: &str) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        if let Some(dir) = path.parent() {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        writeln!(
            file,
            "{}\t{}\t{}",
            utc(when),
            action.command_line(),
            safe_text(result)
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (path, when, action, result);
        Err(io::Error::other("logging needs macOS or Linux"))
    }
}

/// `2026-11-12T18:05:09Z`.
fn utc(when: SystemTime) -> String {
    let secs = when
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(name: &str) -> Package {
        Package::new(name, false).unwrap()
    }

    #[test]
    fn command_lines_are_fixed() {
        let node = formula("node");
        let firefox = Package::new("firefox", true).unwrap();
        let cases = [
            (Action::BrewUpdate, "brew update", None),
            (
                Action::BrewUpgrade(node.clone()),
                "brew upgrade --formula node",
                Some("brew upgrade --dry-run --formula node"),
            ),
            (
                Action::BrewUpgrade(firefox.clone()),
                "brew upgrade --cask firefox",
                Some("brew upgrade --dry-run --cask firefox"),
            ),
            (
                Action::BrewUninstall(node),
                "brew uninstall --formula node",
                None,
            ),
            (
                Action::BrewUninstall(firefox),
                "brew uninstall --cask firefox",
                None,
            ),
            (
                Action::BrewAutoremove,
                "brew autoremove",
                Some("brew autoremove --dry-run"),
            ),
            (
                Action::BrewCleanup,
                "brew cleanup",
                Some("brew cleanup --dry-run"),
            ),
        ];
        for (action, line, preview) in cases {
            assert_eq!(action.command_line(), line);
            assert_eq!(action.preview_line().as_deref(), preview);
            for word in action.args() {
                assert!(
                    ![
                        "--force",
                        "-f",
                        "--ignore-dependencies",
                        "--zap",
                        "--greedy"
                    ]
                    .contains(&word.as_str()),
                    "{line}"
                );
            }
        }
    }

    #[test]
    fn rust_command_lines_are_fixed_and_run_the_copy_found_on_path() {
        let rustup = Program::found(Path::new("/home/me/.cargo/bin/rustup"), "rustup").unwrap();
        let cargo = Program::found(Path::new("/home/me/.cargo/bin/cargo"), "cargo").unwrap();
        let stable = Name::toolchain("stable-aarch64-apple-darwin", "stable").unwrap();
        let bacon = Name::krate("bacon").unwrap();
        let cases = [
            (
                Action::RustupUpdate(rustup.clone(), stable.clone()),
                "/home/me/.cargo/bin/rustup update --no-self-update stable-aarch64-apple-darwin",
                Some("/home/me/.cargo/bin/rustup check --no-self-update"),
                "Update stable",
            ),
            (
                Action::RustupUninstall(rustup, stable),
                "/home/me/.cargo/bin/rustup toolchain uninstall stable-aarch64-apple-darwin",
                None,
                "Uninstall toolchain stable",
            ),
            (
                Action::CargoInstall(cargo.clone(), bacon.clone()),
                "/home/me/.cargo/bin/cargo install --locked bacon",
                None,
                "Upgrade bacon",
            ),
            (
                Action::CargoUninstall(cargo, bacon),
                "/home/me/.cargo/bin/cargo uninstall bacon",
                None,
                "Uninstall bacon",
            ),
        ];
        for (action, line, preview, title) in cases {
            assert_eq!(action.area(), Area::Toolchains);
            assert_eq!(action.command_line(), line);
            assert_eq!(action.preview_line().as_deref(), preview);
            assert_eq!(action.title(), title);
            for word in action.args() {
                assert!(
                    !["--force", "-f", "self", "--force-non-host"].contains(&word.as_str()),
                    "{line}"
                );
            }
        }
        // Only a full path naming exactly that program.
        assert!(Program::found(Path::new("rustup"), "rustup").is_none());
        assert!(Program::found(Path::new("/bin/sh"), "cargo").is_none());
        assert!(Program::found(Path::new("/x/cargo-clippy"), "cargo").is_none());
    }

    #[test]
    fn only_toolchain_and_crate_names_become_commands() {
        for good in [
            "stable-aarch64-apple-darwin",
            "nightly-2026-01-01-x86_64-unknown-linux-gnu",
            "1.88.0-aarch64-apple-darwin",
        ] {
            assert!(Name::toolchain(good, good).is_some(), "{good}");
        }
        for good in ["bacon", "cargo-nextest", "ripgrep", "a_b"] {
            assert!(Name::krate(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "-v",
            "--force",
            "a b",
            "a;rm",
            "$(x)",
            "../x",
            "a/b",
            "\u{1b}[31m",
            "nöde",
        ] {
            assert!(Name::toolchain(bad, bad).is_none(), "{bad:?}");
            assert!(Name::krate(bad).is_none(), "{bad:?}");
        }
        assert!(Name::krate("1password").is_none(), "a letter first");
        assert!(Name::krate(&"a".repeat(65)).is_none());
        // The label shown is made safe; the target is used as given.
        let t = Name::toolchain("stable", "st\u{1b}[31mable").unwrap();
        assert!(!t.label().contains('\u{1b}'));
    }

    #[test]
    fn package_command_lines_are_fixed() {
        let at = |name: &'static str| Program::found(&Path::new("/p").join(name), name).unwrap();
        let cli = Name::npm("@acme/cli").unwrap();
        let ruff = Name::pypi("ruff").unwrap();
        let cases = [
            (
                Action::NpmUpdate(at("npm"), cli.clone()),
                "/p/npm update -g @acme/cli",
                Some("/p/npm update -g --dry-run @acme/cli"),
            ),
            (
                Action::NpmUninstall(at("npm"), cli),
                "/p/npm uninstall -g @acme/cli",
                Some("/p/npm uninstall -g --dry-run @acme/cli"),
            ),
            (
                Action::UvToolUpgrade(at("uv"), ruff.clone()),
                "/p/uv tool upgrade ruff",
                None,
            ),
            (
                Action::UvToolUninstall(at("uv"), ruff.clone()),
                "/p/uv tool uninstall ruff",
                None,
            ),
            (
                Action::PipxUpgrade(at("pipx"), ruff.clone()),
                "/p/pipx upgrade ruff",
                None,
            ),
            (
                Action::PipxUninstall(at("pipx"), ruff),
                "/p/pipx uninstall ruff",
                None,
            ),
        ];
        for (action, line, preview) in cases {
            assert_eq!(action.area(), Area::Toolchains);
            assert_eq!(action.command_line(), line);
            assert_eq!(action.preview_line().as_deref(), preview);
            for word in action.args() {
                assert!(
                    !["--force", "-f", "--all", "--global-style"].contains(&word.as_str()),
                    "{line}"
                );
            }
        }
    }

    #[test]
    fn only_npm_and_python_package_names_become_commands() {
        for good in ["prettier", "@acme/cli", "typescript", "a.b_c~d-e", "@a/b"] {
            assert!(Name::npm(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "-g",
            "--force",
            "Prettier",
            "@acme",
            "@acme/",
            "@/x",
            "a/b",
            "../x",
            "a b",
            "a;rm",
            "$(x)",
            "\u{1b}[31m",
            "@a/b/c",
        ] {
            assert!(Name::npm(bad).is_none(), "{bad:?}");
        }
        for good in ["ruff", "httpie", "Django", "zope.interface", "a_b-c"] {
            assert!(Name::pypi(good).is_some(), "{good}");
        }
        for bad in [
            "",
            "-v",
            "--force",
            "a b",
            "a;rm",
            "ruff-",
            ".x",
            "a/b",
            "\u{1b}[31m",
        ] {
            assert!(Name::pypi(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn rustup_check_finding_an_update_is_a_finished_dry_run() {
        let rustup = Program::found(Path::new("/r/rustup"), "rustup").unwrap();
        let update = Action::RustupUpdate(rustup, Name::toolchain("stable", "stable").unwrap());
        let mut s = State::new(Permit::granted(true));
        s.open(None, vec![Item::new(update.clone())]);
        let output = |code| CommandOutput {
            code: Some(code),
            stdout: b"stable - Update available : 1.97.1 -> 1.98.0\n".to_vec(),
            stderr: vec![],
        };
        s.finish_preview(&update, Ok(output(100)));
        assert!(matches!(
            &s.dialog,
            Some(Dialog::Menu { preview: Preview::Ready(text), .. }) if text.contains("1.98.0")
        ));
        s.finish_preview(&update, Ok(output(1)));
        assert!(matches!(
            &s.dialog,
            Some(Dialog::Menu {
                preview: Preview::Failed(_),
                ..
            })
        ));
        // Exit 100 means nothing special for other tools.
        assert!(!Action::BrewCleanup.preview_succeeded(Some(100)));
    }

    #[test]
    fn only_homebrew_names_become_commands() {
        for good in [
            "node",
            "python@3.13",
            "gtk+3",
            "libxml2",
            "user/tap/tool",
            "a.b_c-d",
        ] {
            assert!(Package::new(good, false).is_some(), "{good}");
        }
        for bad in [
            "",
            "-rf",
            "--force",
            "node --force",
            "node;rm",
            "$(x)",
            "a/b",
            "../x",
            "/abs",
            "\u{1b}[31m",
            "nöde",
        ] {
            assert!(Package::new(bad, false).is_none(), "{bad:?}");
        }
        assert!(Package::new(&"a".repeat(129), false).is_none());
    }

    #[test]
    fn docker_command_lines_are_fixed() {
        let id = "aaaaaaaaaaaa1111111111111111111111111111111111111111111111111111";
        let container = DockerObject::id(id, "web-nginx-1").unwrap();
        let image = DockerObject::image("redis:7", "redis:7").unwrap();
        let untagged = DockerObject::id("sha256:888888888888", "<none> 888888888888").unwrap();
        let volume = DockerObject::volume("web_data", "web_data").unwrap();
        let cases = [
            (
                Action::DockerStopContainer(container.clone()),
                format!("docker container stop {id}"),
                "Stop web-nginx-1",
            ),
            (
                Action::DockerRemoveContainer(container),
                format!("docker container rm {id}"),
                "Remove web-nginx-1",
            ),
            (
                Action::DockerRemoveImages(vec![image.clone()]),
                "docker image rm redis:7".into(),
                "Remove redis:7",
            ),
            (
                Action::DockerRemoveImages(vec![untagged.clone(), image]),
                "docker image rm sha256:888888888888 redis:7".into(),
                "Remove 2 unused untagged images",
            ),
            (
                Action::DockerRemoveVolume(volume),
                "docker volume rm web_data".into(),
                "Remove volume web_data",
            ),
        ];
        for (action, line, title) in cases {
            assert_eq!(action.command_line(), line);
            assert_eq!(action.title(), title);
            assert_eq!(action.area(), Area::Docker);
            assert_eq!(action.preview_args(), None, "Docker has no dry run");
            for word in action.args() {
                assert!(
                    !["-f", "--force", "prune", "--all", "-a"].contains(&word.as_str()),
                    "{line}"
                );
            }
        }
        assert_eq!(Action::BrewCleanup.area(), Area::Homebrew);
    }

    #[test]
    fn only_docker_ids_and_names_become_commands() {
        assert!(DockerObject::id("0123456789ab", "x").is_some());
        assert!(DockerObject::id(&format!("sha256:{}", "f".repeat(64)), "x").is_some());
        for bad in [
            "",
            "abc",
            "-f",
            "0123456789ag",
            &"a".repeat(65),
            "sha256:",
            "../x",
        ] {
            assert!(DockerObject::id(bad, "x").is_none(), "{bad:?}");
        }
        for good in [
            "redis:7",
            "nginx",
            "registry:5000/team/app:1.2",
            "app@sha256:abcd",
        ] {
            assert!(DockerObject::image(good, "x").is_some(), "{good}");
        }
        for bad in ["", "-f", "--force", "a b", "a;rm", "$(x)", "\u{1b}[31m"] {
            assert!(DockerObject::image(bad, "x").is_none(), "{bad:?}");
            assert!(DockerObject::volume(bad, "x").is_none(), "{bad:?}");
        }
        assert!(DockerObject::volume("web_data", "x").is_some());
        assert!(
            DockerObject::volume("a", "x").is_none(),
            "Docker needs two characters"
        );
        assert!(DockerObject::volume("a/b", "x").is_none());
        // The label shown is made safe; the target is used as given.
        let o = DockerObject::image("redis:7", "redis\u{1b}[31m").unwrap();
        assert!(!o.label().contains('\u{1b}'));
    }

    #[test]
    fn the_first_action_is_selected_and_enter_runs_it() {
        let id = "bbbbbbbbbbbb2222222222222222222222222222222222222222222222222222";
        let remove = Action::DockerRemoveContainer(DockerObject::id(id, "web-db-1").unwrap());
        let mut s = State::new(Permit::granted(true));
        s.open(None, vec![item(remove.clone()), item(Action::BrewUpdate)]);
        assert!(matches!(s.dialog, Some(Dialog::Menu { selected: 0, .. })));
        s.run_selected();
        assert_eq!(s.request, Some(Request::Run(remove)));
    }

    fn item(action: Action) -> Item {
        Item::new(action)
    }

    #[test]
    fn only_dew_runs_the_selected_action_after_its_preview() {
        let upgrade = Action::BrewUpgrade(formula("node"));
        let items = || vec![item(upgrade.clone()), item(Action::BrewUpdate)];
        // Without a permit (den): no preview runs, and Enter refuses.
        let mut s = State::new(None);
        s.open(None, items());
        assert_eq!(s.request, None);
        s.run_selected();
        assert_eq!(s.request, None);
        assert!(s.notice.as_deref().unwrap().contains("dew makes changes"));

        // With a permit (dew): opening starts the first action's dry run.
        let mut s = State::new(Permit::granted(true));
        s.open(None, items());
        assert_eq!(s.request.take(), Some(Request::Preview(upgrade.clone())));
        s.run_selected();
        assert_eq!(s.request, None, "not before the preview finishes");
        // A late preview of another action is ignored.
        s.finish_preview(
            &Action::BrewCleanup,
            Ok(CommandOutput {
                code: Some(0),
                stdout: b"other\n".to_vec(),
                stderr: vec![],
            }),
        );
        s.run_selected();
        assert_eq!(s.request, None);
        s.finish_preview(
            &upgrade,
            Ok(CommandOutput {
                code: Some(0),
                stdout: b"Would upgrade node 26.13 -> 26.14\n".to_vec(),
                stderr: vec![],
            }),
        );
        s.run_selected();
        assert_eq!(s.request.take(), Some(Request::Run(upgrade.clone())));
        assert!(s.running());

        // An action without a dry run runs straight away.
        let mut s = State::new(Permit::granted(true));
        s.open(None, items());
        s.select(1);
        assert_eq!(s.request, None);
        s.run_selected();
        assert_eq!(s.request, Some(Request::Run(Action::BrewUpdate)));
    }

    #[test]
    fn blocked_actions_never_run() {
        let mut s = State::new(Permit::granted(true));
        s.open(
            None,
            vec![Item {
                action: Action::BrewUninstall(formula("openssl@3")),
                blocked: Some("required by 12 installed packages".into()),
                note: None,
            }],
        );
        assert_eq!(s.request, None);
        s.select(0);
        s.run_selected();
        assert_eq!(s.request, None);
        assert!(s.notice.as_deref().unwrap().contains("required by 12"));
    }

    #[test]
    fn results_are_logged_without_output() {
        let dir = std::env::temp_dir().join(format!("den-actions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("state/den/actions.log");
        let mut s = State::new(Permit::granted(true));
        s.open(None, vec![item(Action::BrewUpdate)]);
        s.run_selected();
        s.finish(
            Ok(CommandOutput {
                code: Some(1),
                stdout: b"SECRET-OUTPUT\n".to_vec(),
                stderr: b"Error: \x1b[31mno network\n".to_vec(),
            }),
            Some(&log),
        );
        let Some(Dialog::Done {
            summary, output, ..
        }) = &s.dialog
        else {
            panic!("{:?}", s.dialog);
        };
        assert_eq!(summary, "Failed: exit 1");
        assert!(output.contains("no network") && !output.contains('\x1b'));
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.ends_with("\tbrew update\tFailed: exit 1\n"), "{text}");
        assert!(!text.contains("SECRET"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&log), 0o600);
            assert_eq!(mode(log.parent().unwrap()), 0o700);
            // A link in place of the log is refused, not followed.
            std::fs::remove_file(&log).unwrap();
            std::os::unix::fs::symlink(dir.join("elsewhere"), &log).unwrap();
            assert!(append_log(&log, SystemTime::now(), &Action::BrewUpdate, "Done").is_err());
            assert!(!dir.join("elsewhere").exists());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn known_answers() {
        let at = |s| utc(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_794_506_709), "2026-11-12T18:05:09Z");
    }
}
