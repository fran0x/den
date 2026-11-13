use super::CollectionError;
#[cfg(all(test, unix))]
mod verification;
use crate::areas::homebrew::diagnostics::Check;
use crate::inventory::Inventory;
use std::sync::atomic::AtomicBool;
pub type Collector<T = Inventory> = crate::worker::Worker<T, CollectionError>;

impl Collector {
    pub fn start() -> Result<Self, CollectionError> {
        Self::start_with(collect)
    }
}

#[cfg(unix)]
pub fn collect(cancel: &AtomicBool) -> Result<Inventory, CollectionError> {
    let bytes = unix::execute(&mut command(), cancel, unix::Limits::default())?;
    super::parse(&bytes)
}

#[cfg(not(unix))]
pub fn collect(_: &AtomicBool) -> Result<Inventory, CollectionError> {
    Err(CollectionError::Unavailable)
}

#[cfg(unix)]
fn command() -> std::process::Command {
    let mut command = std::process::Command::new("brew");
    command
        .args(["info", "--json=v2", "--installed"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1");
    command
}

/// The same bounded runner for other read-only tools (Docker): the caller fixes the
/// program and arguments; the child gets its own process group, a timeout, output caps
/// and cancellation, and is killed with its descendants when dropped.
#[cfg(unix)]
pub(crate) fn run_bounded(
    command: &mut std::process::Command,
    cancel: &AtomicBool,
    timeout: std::time::Duration,
    stdout: usize,
) -> Result<CommandOutput, CollectionError> {
    unix::capture(
        command,
        cancel,
        unix::Limits {
            timeout,
            stdout,
            stderr: 64 * 1024,
            grace: None,
        },
    )
}

/// A write action: like [`run_bounded`], but on cancel or timeout the process group is
/// asked to stop (SIGTERM) and given `grace` before it is killed.
#[cfg(unix)]
pub(crate) fn run_write(
    command: &mut std::process::Command,
    cancel: &AtomicBool,
    timeout: std::time::Duration,
    grace: std::time::Duration,
) -> Result<CommandOutput, CollectionError> {
    unix::capture(
        command,
        cancel,
        unix::Limits {
            timeout,
            stdout: 1024 * 1024,
            stderr: 1024 * 1024,
            grace: Some(grace),
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Closed command set: no caller can omit --dry-run or supply shell arguments.
pub fn diagnose(check: Check, cancel: &AtomicBool) -> Result<CommandOutput, CollectionError> {
    #[cfg(unix)]
    {
        let mut command = diagnostic_command(check);
        unix::capture(
            &mut command,
            cancel,
            unix::Limits {
                stdout: 256 * 1024,
                ..unix::Limits::default()
            },
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (check, cancel);
        Err(CollectionError::Unavailable)
    }
}

#[cfg(unix)]
fn diagnostic_command(check: Check) -> std::process::Command {
    let mut command = std::process::Command::new("brew");
    command
        .args(check.args())
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_COLOR", "1");
    command
}

#[cfg(unix)]
mod unix {
    use super::CollectionError;
    use std::{
        io::{self, Read},
        os::{fd::AsRawFd, unix::process::CommandExt},
        process::{Child, Command, Stdio},
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };

    pub(super) struct Limits {
        pub timeout: Duration,
        pub stdout: usize,
        pub stderr: usize,
        /// Writes only: on cancel or timeout, ask the process group to stop (SIGTERM)
        /// and wait this long before killing it, so a package manager can clean up.
        pub grace: Option<Duration>,
    }
    impl Default for Limits {
        fn default() -> Self {
            Self {
                timeout: Duration::from_secs(30),
                stdout: super::super::MAX_INPUT_BYTES,
                stderr: 64 * 1024,
                grace: None,
            }
        }
    }

    fn terminate(process: &Process) {
        // SAFETY: the child was placed in its own process group using process_group(0);
        // the negative PID addresses only that group.
        unsafe {
            libc::kill(-(process.0.id() as i32), libc::SIGTERM);
        }
    }

    struct Process(Child);
    impl Drop for Process {
        fn drop(&mut self) {
            // SAFETY: the child was placed in its own process group using process_group(0).
            // Negative PID addresses that group, including descendants holding pipe handles.
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGKILL);
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn nonblocking(pipe: &impl AsRawFd) -> Result<(), CollectionError> {
        // SAFETY: the pipe owns this valid descriptor for the duration of both fcntl calls.
        let result = unsafe {
            let flags = libc::fcntl(pipe.as_raw_fd(), libc::F_GETFL);
            if flags < 0 {
                -1
            } else {
                libc::fcntl(pipe.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK)
            }
        };
        if result == -1 {
            Err(CollectionError::Failed(
                "cannot configure process pipes".into(),
            ))
        } else {
            Ok(())
        }
    }

    fn drain(
        pipe: &mut impl Read,
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<bool, CollectionError> {
        let mut buffer = [0; 8192];
        // Bound each turn too: continuously writing children must not starve cancellation.
        for _ in 0..16 {
            match pipe.read(&mut buffer) {
                Ok(0) => return Ok(true),
                Ok(count) => {
                    if count > limit.saturating_sub(output.len()) {
                        return Err(CollectionError::OutputLimit);
                    }
                    output.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(CollectionError::Failed("cannot read process output".into())),
            }
        }
        Ok(false)
    }

    /// Waits until an open pipe has output or closes, at most `limit`, so the loop reacts to
    /// output at once instead of sleeping a fixed time. With no pipe open it just waits.
    fn wait_for_output(pipes: &[i32], limit: Duration) {
        let mut polls: Vec<libc::pollfd> = pipes
            .iter()
            .map(|&fd| libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            })
            .collect();
        let count = libc::nfds_t::try_from(polls.len()).unwrap_or(0);
        let millis = i32::try_from(limit.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: `polls` holds `count` initialized pollfd entries and outlives the call.
        // An error (EINTR) or timeout only ends the wait early; the caller loops anyway.
        unsafe {
            libc::poll(polls.as_mut_ptr(), count, millis);
        }
    }

    pub(super) fn execute(
        command: &mut Command,
        cancel: &AtomicBool,
        limits: Limits,
    ) -> Result<Vec<u8>, CollectionError> {
        let output = capture(command, cancel, limits)?;
        if output.code == Some(0) {
            return Ok(output.stdout);
        }
        Err(CollectionError::Failed(match output.code {
            Some(code) => format!("brew exited with code {code}; raw diagnostics withheld"),
            None => "brew terminated by signal; raw diagnostics withheld".into(),
        }))
    }

    pub(super) fn capture(
        command: &mut Command,
        cancel: &AtomicBool,
        limits: Limits,
    ) -> Result<super::CommandOutput, CollectionError> {
        if cancel.load(Ordering::Relaxed) {
            return Err(CollectionError::Cancelled);
        }
        let start = Instant::now();
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    CollectionError::Unavailable
                } else {
                    CollectionError::Failed(format!("cannot start brew ({:?})", error.kind()))
                }
            })?;
        let mut process = Process(child);
        let mut stdout = process
            .0
            .stdout
            .take()
            .ok_or_else(|| CollectionError::Failed("missing stdout pipe".into()))?;
        let mut stderr = process
            .0
            .stderr
            .take()
            .ok_or_else(|| CollectionError::Failed("missing stderr pipe".into()))?;
        nonblocking(&stdout)?;
        nonblocking(&stderr)?;
        let (mut output, mut diagnostics) = (Vec::new(), Vec::new());
        let (mut stdout_done, mut stderr_done) = (false, false);
        // Why and since when a write is being asked to stop.
        let mut stopping: Option<(Instant, CollectionError)> = None;
        loop {
            let stop = if cancel.load(Ordering::Relaxed) {
                Some(CollectionError::Cancelled)
            } else if start.elapsed() >= limits.timeout {
                Some(CollectionError::Timeout)
            } else {
                None
            };
            if let Some(reason) = stop {
                match (limits.grace, &stopping) {
                    (Some(_), None) => {
                        terminate(&process);
                        stopping = Some((Instant::now(), reason));
                    }
                    (Some(grace), Some((since, _))) if since.elapsed() < grace => {}
                    _ => return Err(stopping.map_or(reason, |(_, first)| first)),
                }
            }
            if !stdout_done {
                stdout_done = drain(&mut stdout, &mut output, limits.stdout)?;
            }
            if !stderr_done {
                stderr_done = drain(&mut stderr, &mut diagnostics, limits.stderr)?;
            }
            if let Some(status) = process
                .0
                .try_wait()
                .map_err(|_| CollectionError::Failed("cannot wait for brew".into()))?
                && stdout_done
                && stderr_done
            {
                if let Some((_, reason)) = stopping {
                    return Err(reason);
                }
                return Ok(super::CommandOutput {
                    code: status.code(),
                    stdout: output,
                    stderr: diagnostics,
                });
            }
            let open: Vec<i32> = [
                (stdout_done, stdout.as_raw_fd()),
                (stderr_done, stderr.as_raw_fd()),
            ]
            .into_iter()
            .filter_map(|(done, fd)| (!done).then_some(fd))
            .collect();
            // Output wakes the loop at once; at most 20 ms passes between checks for
            // cancellation and the time limit. Once both pipes are closed, only the exit
            // status is left, so check again soon.
            let limit = if open.is_empty() { 1 } else { 20 };
            wait_for_output(&open, Duration::from_millis(limit));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{io::Write, thread};

        // A test executable serves as the fake child; no shell, Homebrew or network needed.
        #[test]
        fn child_fixture() {
            let Ok(mode) = std::env::var("DEN_TEST_CHILD") else {
                return;
            };
            match mode.as_str() {
                "missing" => {
                    io::stdout().write_all(b"source: absent\n").unwrap();
                    io::stderr().write_all(b"test stderr\n").unwrap();
                    std::process::exit(1);
                }
                "exit" => std::process::exit(7),
                "stdout" => {
                    io::stdout().write_all(&[b'x'; 8192]).unwrap();
                }
                "stderr" => {
                    io::stderr().write_all(&[b'x'; 8192]).unwrap();
                }
                "wait" => thread::sleep(Duration::from_secs(5)),
                "ignore-term" => {
                    // SAFETY: setting a signal disposition to SIG_IGN in this short-lived
                    // child process; no handler code runs.
                    unsafe {
                        libc::signal(libc::SIGTERM, libc::SIG_IGN);
                    }
                    thread::sleep(Duration::from_secs(5));
                }
                _ => {}
            }
        }
        fn fake(mode: &str) -> Command {
            // Test names leave out the crate name: `areas::homebrew::...::child_fixture`.
            let module = module_path!().split_once("::").unwrap().1;
            let name = format!("{module}::child_fixture");
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", &name, "--nocapture"])
                .env("DEN_TEST_CHILD", mode);
            command
        }
        fn limits() -> Limits {
            Limits {
                timeout: Duration::from_secs(2),
                stdout: 1024,
                stderr: 1024,
                grace: None,
            }
        }

        #[test]
        fn missing_brew_and_nonzero_exit_are_errors() {
            let cancel = AtomicBool::new(false);
            assert_eq!(
                execute(
                    &mut Command::new("/nonexistent-den-test/brew"),
                    &cancel,
                    limits()
                ),
                Err(CollectionError::Unavailable)
            );
            assert!(
                matches!(execute(&mut fake("exit"), &cancel, limits()), Err(CollectionError::Failed(message)) if message.contains("code 7"))
            );
        }
        #[test]
        fn diagnostic_capture_retains_expected_nonzero_output_and_status() {
            let output = capture(&mut fake("missing"), &AtomicBool::new(false), limits()).unwrap();
            assert_eq!(output.code, Some(1));
            assert!(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .contains("source: absent\n")
            );
            assert_eq!(output.stderr, b"test stderr\n");
        }
        #[test]
        fn stdout_and_stderr_are_bounded() {
            for mode in ["stdout", "stderr"] {
                assert_eq!(
                    execute(&mut fake(mode), &AtomicBool::new(false), limits()),
                    Err(CollectionError::OutputLimit)
                );
            }
        }
        #[test]
        fn successful_command_with_invalid_json_is_not_an_empty_inventory() {
            let output = execute(&mut fake("invalid"), &AtomicBool::new(false), limits()).unwrap();
            assert!(matches!(
                super::super::super::parse(&output),
                Err(CollectionError::Invalid(_))
            ));
        }
        /// Cancels after 300 ms, from another thread, and times the capture.
        fn stopped(
            mode: &str,
            grace: Duration,
        ) -> (
            Result<super::super::CommandOutput, CollectionError>,
            Duration,
        ) {
            let cancel = AtomicBool::new(false);
            let start = Instant::now();
            let result = thread::scope(|s| {
                s.spawn(|| {
                    thread::sleep(Duration::from_millis(300));
                    cancel.store(true, Ordering::Relaxed);
                });
                capture(
                    &mut fake(mode),
                    &cancel,
                    Limits {
                        timeout: Duration::from_secs(20),
                        grace: Some(grace),
                        ..limits()
                    },
                )
            });
            (result, start.elapsed())
        }

        #[test]
        fn a_write_is_asked_to_stop_before_it_is_killed() {
            // SIGTERM ends this child at once: well inside an 8-second grace.
            let (result, took) = stopped("wait", Duration::from_secs(8));
            assert_eq!(result, Err(CollectionError::Cancelled));
            assert!(took < Duration::from_secs(4), "{took:?}");
            // A child that ignores SIGTERM is killed once the grace runs out.
            let (result, took) = stopped("ignore-term", Duration::from_secs(1));
            assert_eq!(result, Err(CollectionError::Cancelled));
            assert!(
                took >= Duration::from_millis(1200) && took < Duration::from_secs(4),
                "{took:?}"
            );
        }

        #[test]
        fn timeout_kills_and_reaps_child() {
            let limits = Limits {
                timeout: Duration::from_millis(100),
                ..limits()
            };
            let start = Instant::now();
            assert_eq!(
                execute(&mut fake("wait"), &AtomicBool::new(false), limits),
                Err(CollectionError::Timeout)
            );
            assert!(start.elapsed() < Duration::from_secs(2));
        }
        #[test]
        fn cancellation_interrupts_running_process() {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let worker_cancel = cancel.clone();
            let worker =
                thread::spawn(move || execute(&mut fake("wait"), &worker_cancel, limits()));
            thread::sleep(Duration::from_millis(50));
            cancel.store(true, Ordering::Relaxed);
            assert_eq!(worker.join().unwrap(), Err(CollectionError::Cancelled));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::Ordering, thread};
    #[cfg(unix)]
    #[test]
    fn diagnostics_have_closed_read_only_arguments_and_fixed_environment() {
        for (check, args) in [
            (Check::Missing, vec!["missing"]),
            (Check::Leaves, vec!["leaves"]),
            (Check::Cleanup, vec!["autoremove", "--dry-run"]),
        ] {
            let command = diagnostic_command(check);
            assert_eq!(command.get_program(), "brew");
            assert_eq!(command.get_args().collect::<Vec<_>>(), args);
            for name in [
                "HOMEBREW_NO_AUTO_UPDATE",
                "HOMEBREW_NO_ANALYTICS",
                "HOMEBREW_NO_COLOR",
            ] {
                assert!(
                    command
                        .get_envs()
                        .any(|(k, v)| k == name && v == Some(std::ffi::OsStr::new("1")))
                );
            }
        }
    }
    #[cfg(unix)]
    #[test]
    fn command_is_fixed_and_disables_updates_and_analytics() {
        let command = command();
        assert_eq!(command.get_program(), "brew");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["info", "--json=v2", "--installed"]
        );
        assert_eq!(command.get_envs().count(), 2);
        assert!(
            command
                .get_envs()
                .all(|(_, v)| v == Some(std::ffi::OsStr::new("1")))
        );
    }
    #[test]
    fn dropping_worker_cancels_it() {
        let collector: Collector = Collector::start_with(|cancel| {
            while !cancel.load(Ordering::Relaxed) {
                thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(CollectionError::Cancelled)
        })
        .unwrap();
        drop(collector);
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "opt-in local Homebrew verification; prints only aggregate counts"]
    fn live_homebrew_matches_installation_records() {
        let bytes = unix::execute(
            &mut command(),
            &AtomicBool::new(false),
            unix::Limits::default(),
        )
        .expect("live collection must succeed");
        let raw: serde_json::Value = serde_json::from_slice(&bytes).expect("valid Homebrew JSON");
        let inventory = super::super::parse(&bytes).expect("live inventory must normalize");
        super::super::reconciliation::verify(&raw, &inventory).print();
    }
}
