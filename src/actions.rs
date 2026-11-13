//! Write actions: off by default, confirmed one at a time, and limited to a closed list.
//!
//! `den` is always read-only. Actions can run only in `dew`, which creates the
//! [`Permit`] and shows WRITE in the top bar; each one runs on Enter, after its dry run.
//! Every action is a fixed command line built from a checked name: no shell, no sudo,
//! stdin closed (a password prompt fails instead of waiting), and never `--force`,
//! `--ignore-dependencies` or `--zap`. `den` has no actions at all: no menu, not even
//! commands to copy.
use crate::{
    areas::homebrew::{CollectionError, CommandOutput},
    inventory::safe_text,
};
use std::{
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
}
impl Action {
    pub fn program(&self) -> &'static str {
        if self.docker() { "docker" } else { "brew" }
    }
    /// A Docker action: Docker is refreshed afterwards instead of Homebrew.
    pub fn docker(&self) -> bool {
        matches!(
            self,
            Self::DockerStopContainer(_)
                | Self::DockerRemoveContainer(_)
                | Self::DockerRemoveImages(_)
                | Self::DockerRemoveVolume(_)
        )
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
        }
    }
    /// The dry run shown before confirming, when the tool has one.
    pub fn preview_args(&self) -> Option<Vec<String>> {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        match self {
            Self::BrewUpgrade(p) => Some(words(&["upgrade", "--dry-run", p.kind(), p.name()])),
            Self::BrewAutoremove => Some(words(&["autoremove", "--dry-run"])),
            Self::BrewCleanup => Some(words(&["cleanup", "--dry-run"])),
            _ => None,
        }
    }
    /// The command as you would type it, for the dialog, the clipboard and the log.
    pub fn command_line(&self) -> String {
        line(self.program(), &self.args())
    }
    pub fn preview_line(&self) -> Option<String> {
        self.preview_args().map(|a| line(self.program(), &a))
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
        }
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(match self {
            Self::BrewUpgrade(_) => 30 * 60,
            _ if self.docker() => 2 * 60,
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
fn command(program: &str, args: &[String]) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.args(args);
    if program == "docker" {
        command.env("DOCKER_CLI_HINTS", "false");
    } else {
        command
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("HOMEBREW_NO_ANALYTICS", "1")
            .env("HOMEBREW_NO_ENV_HINTS", "1")
            .env("HOMEBREW_NO_EMOJI", "1")
            .env("HOMEBREW_NO_COLOR", "1");
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
            &mut command(action.program(), &args),
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
            &mut command(action.program(), &action.args()),
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
                Ok(output) if output.code == Some(0) => Preview::Ready(tail(&output)),
                Ok(output) => Preview::Failed(format!("{}\n{}", exit(output.code), tail(&output))),
                Err(error) => Preview::Failed(failure(&error, action.program())),
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
            Err(error) => (failure(error, action.program()), String::new()),
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

fn failure(error: &CollectionError, program: &str) -> String {
    match error {
        CollectionError::Unavailable => format!("{program} is not on PATH"),
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

/// `$XDG_STATE_HOME/den/actions.log`, or `~/.local/state/den/actions.log`.
pub fn log_path() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map(|state| state.join("den/actions.log"))
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
            assert!(action.docker());
            assert_eq!(action.preview_args(), None, "Docker has no dry run");
            for word in action.args() {
                assert!(
                    !["-f", "--force", "prune", "--all", "-a"].contains(&word.as_str()),
                    "{line}"
                );
            }
        }
        assert!(!Action::BrewCleanup.docker());
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
