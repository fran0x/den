//! Write actions: off by default, confirmed one at a time, and limited to a closed list.
//!
//! `den` is always read-only. Actions can run only in `dew` (which creates the [`Permit`]) and write mode is switched on with `W` (shown as WRITE in
//! the top bar), and each one is confirmed with `y` after its preview. Every action is a
//! fixed command line built from a checked name: no shell, no sudo, stdin closed (a
//! password prompt fails instead of waiting), and never `--force`, `--ignore-dependencies`
//! or `--zap`. Without the permit, the same menu only copies commands.
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
}
impl Action {
    pub fn program(&self) -> &'static str {
        "brew"
    }
    pub fn args(&self) -> Vec<String> {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        match self {
            Self::BrewUpdate => words(&["update"]),
            Self::BrewUpgrade(p) => words(&["upgrade", p.kind(), p.name()]),
            Self::BrewUninstall(p) => words(&["uninstall", p.kind(), p.name()]),
            Self::BrewAutoremove => words(&["autoremove"]),
            Self::BrewCleanup => words(&["cleanup"]),
        }
    }
    /// The dry run shown before confirming, when the tool has one.
    pub fn preview_args(&self) -> Option<Vec<String>> {
        let words = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        match self {
            Self::BrewUpgrade(p) => Some(words(&["upgrade", "--dry-run", p.kind(), p.name()])),
            Self::BrewAutoremove => Some(words(&["autoremove", "--dry-run"])),
            Self::BrewCleanup => Some(words(&["cleanup", "--dry-run"])),
            Self::BrewUpdate | Self::BrewUninstall(_) => None,
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
        }
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(match self {
            Self::BrewUpgrade(_) => 30 * 60,
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
    command
        .args(args)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_NO_EMOJI", "1")
        .env("HOMEBREW_NO_COLOR", "1");
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preview {
    /// The tool has no dry run for this action.
    None,
    /// Not run: writes are off.
    Skipped,
    Waiting,
    Ready(String),
    Failed(String),
}

#[derive(Debug)]
pub enum Dialog {
    Menu {
        items: Vec<Item>,
        selected: usize,
    },
    Confirm {
        item: Item,
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
    on: bool,
    pub dialog: Option<Dialog>,
    pub request: Option<Request>,
    pub cancel_requested: bool,
    /// Text for the terminal to place on the clipboard (OSC 52).
    pub clipboard: Option<String>,
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
    pub fn allowed(&self) -> bool {
        self.permit.is_some()
    }
    /// Write mode: permitted and switched on.
    pub fn on(&self) -> bool {
        self.permit.is_some() && self.on
    }
    pub fn permit(&self) -> Option<&Permit> {
        self.permit.as_ref().filter(|_| self.on)
    }
    /// `W`: switches write mode; without the flag, says how to get it.
    pub fn toggle(&mut self) -> String {
        if self.permit.is_none() {
            return "Writes are off in den: run dew to make changes, then press W".into();
        }
        self.on = !self.on;
        if self.on {
            "WRITE mode on: x opens actions; each one asks before it runs".into()
        } else {
            "WRITE mode off: Den is read-only again".into()
        }
    }
    pub fn running(&self) -> bool {
        matches!(self.dialog, Some(Dialog::Running { .. }))
    }
    pub fn open(&mut self, items: Vec<Item>) {
        self.notice = None;
        self.dialog = Some(Dialog::Menu { items, selected: 0 });
    }
    /// Copies the command; nothing runs.
    pub fn copy(&mut self, action: &Action) {
        let line = action.command_line();
        self.notice = Some(format!("Copied: {line} (if your terminal allows it)"));
        self.clipboard = Some(line);
    }
    /// Menu → confirmation; the preview starts only in write mode.
    pub fn choose(&mut self, item: Item) {
        let preview = match (&item.blocked, item.action.preview_args(), self.on()) {
            (Some(_), _, _) | (_, None, _) => Preview::None,
            (None, Some(_), false) => Preview::Skipped,
            (None, Some(_), true) => {
                self.request = Some(Request::Preview(item.action.clone()));
                Preview::Waiting
            }
        };
        self.notice = None;
        self.dialog = Some(Dialog::Confirm { item, preview });
    }
    pub fn finish_preview(&mut self, result: Result<CommandOutput, CollectionError>) {
        if let Some(Dialog::Confirm { preview, .. }) = &mut self.dialog {
            *preview = match result {
                Ok(output) if output.code == Some(0) => Preview::Ready(tail(&output)),
                Ok(output) => Preview::Failed(format!("{}\n{}", exit(output.code), tail(&output))),
                Err(error) => Preview::Failed(failure(&error)),
            };
        }
    }
    /// `y` in the confirmation: runs only in write mode, unblocked, after the preview.
    pub fn confirm(&mut self) {
        let Some(Dialog::Confirm { item, preview }) = &self.dialog else {
            return;
        };
        let refusal = if !self.on() {
            Some(self.toggle_hint())
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
    fn toggle_hint(&self) -> String {
        if self.permit.is_none() {
            "Writes are off in den: run dew to make changes. c copies the command".into()
        } else {
            "WRITE mode is off: press W first".into()
        }
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
            Err(error) => (failure(error), String::new()),
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
        self.notice = None;
    }
}

fn exit(code: Option<i32>) -> String {
    code.map_or_else(|| "stopped by a signal".into(), |c| format!("exit {c}"))
}

fn failure(error: &CollectionError) -> String {
    match error {
        CollectionError::Unavailable => "brew is not on PATH".into(),
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

/// OSC 52: asks the terminal to put `text` on the clipboard. Terminals that do not
/// support it ignore the sequence.
pub fn clipboard_sequence(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(TABLE[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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

    fn item(action: Action) -> Item {
        Item {
            action,
            blocked: None,
        }
    }

    #[test]
    fn nothing_runs_without_the_flag_and_write_mode() {
        let upgrade = Action::BrewUpgrade(formula("node"));
        // No flag: the menu copies, previews do not run, and y refuses.
        let mut s = State::new(None);
        assert!(s.toggle().contains("run dew"));
        assert!(!s.on());
        s.choose(item(upgrade.clone()));
        assert_eq!(s.request, None);
        assert!(matches!(
            s.dialog,
            Some(Dialog::Confirm {
                preview: Preview::Skipped,
                ..
            })
        ));
        s.confirm();
        assert_eq!(s.request, None);
        assert!(s.notice.as_deref().unwrap().contains("run dew"));
        s.copy(&upgrade);
        assert_eq!(s.clipboard.as_deref(), Some("brew upgrade --formula node"));

        // The flag alone is not enough: write mode must be on.
        let mut s = State::new(Permit::granted(true));
        s.choose(item(upgrade.clone()));
        s.confirm();
        assert_eq!(s.request, None);
        assert!(s.notice.as_deref().unwrap().contains("press W"));
        assert!(s.permit().is_none());

        // Flag and write mode: the preview runs first, then y runs the action.
        s.toggle();
        assert!(s.on() && s.permit().is_some());
        s.choose(item(upgrade.clone()));
        assert_eq!(s.request.take(), Some(Request::Preview(upgrade.clone())));
        s.confirm();
        assert_eq!(s.request, None, "not before the preview finishes");
        s.finish_preview(Ok(CommandOutput {
            code: Some(0),
            stdout: b"Would upgrade node 26.13 -> 26.14\n".to_vec(),
            stderr: vec![],
        }));
        s.confirm();
        assert_eq!(s.request.take(), Some(Request::Run(upgrade)));
        assert!(s.running());
    }

    #[test]
    fn blocked_actions_never_run() {
        let mut s = State::new(Permit::granted(true));
        s.toggle();
        s.choose(Item {
            action: Action::BrewUninstall(formula("openssl@3")),
            blocked: Some("required by 12 installed packages".into()),
        });
        assert_eq!(s.request, None);
        s.confirm();
        assert_eq!(s.request, None);
        assert!(s.notice.as_deref().unwrap().contains("required by 12"));
    }

    #[test]
    fn results_are_logged_without_output() {
        let dir = std::env::temp_dir().join(format!("den-actions-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = dir.join("state/den/actions.log");
        let mut s = State::new(Permit::granted(true));
        s.toggle();
        s.choose(item(Action::BrewUpdate));
        s.confirm();
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
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"brew update"), "YnJldyB1cGRhdGU=");
        assert_eq!(clipboard_sequence("hi"), "\x1b]52;c;aGk=\x07");
        let at = |s| utc(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_794_506_709), "2026-11-12T18:05:09Z");
    }
}
