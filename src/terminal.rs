use std::io::{self, IsTerminal};

use crossterm::{
    cursor::Show,
    event::{self, Event},
    execute,
    style::{Attribute, ResetColor, SetAttribute},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{DefaultTerminal, Terminal, backend::CrosstermBackend};

use crate::{
    app::App,
    diagnostics::Check,
    homebrew::{Collector, CommandOutput, diagnose},
    ui,
};

/// Created before initialization so partial setup errors also restore the terminal.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        restore();
    }
}

pub fn run(app: &mut App) -> io::Result<()> {
    with_terminal(|terminal| {
        let mut collector: Option<Collector> = None;
        let mut skills: Option<crate::skills::Collector> = None;
        let mut codex: Option<crate::codex::Collector> = None;
        let mut claude: Option<crate::claude::Collector> = None;
        let mut aws: Option<crate::aws::Collector> = None;
        let mut kube: Option<crate::kube::Collector> = None;
        let mut toolchains: Option<crate::toolchains::Collector> = None;
        let mut docker: Option<crate::docker::Collector> = None;
        let mut diagnostic: Option<(Check, u64, Collector<CommandOutput>)> = None;
        while !app.quit {
            if app.module == crate::app::Module::Homebrew
                && app.refresh_requested
                && collector.is_none()
                && diagnostic.is_none()
            {
                app.begin_collection();
                match Collector::start() {
                    Ok(worker) => collector = Some(worker),
                    Err(error) => app.finish_collection(Err(error)),
                }
            }
            if let Some(result) = collector.as_ref().and_then(Collector::poll) {
                app.finish_collection(result);
                collector = None;
            }
            if collector.is_none()
                && diagnostic.is_none()
                && let Some(check) = app.begin_check()
            {
                let epoch = app.inventory_epoch;
                match Collector::start_with(move |cancel| diagnose(check, cancel)) {
                    Ok(worker) => diagnostic = Some((check, epoch, worker)),
                    Err(error) => app.finish_check(check, Err(error), epoch),
                }
            }
            if let Some((check, epoch, worker)) = &diagnostic
                && let Some(result) = worker.poll()
            {
                app.finish_check(*check, result, *epoch);
                diagnostic = None;
            }
            // Opening Agents collects Codex and Claude Code; each has its own worker.
            if app.module == crate::app::Module::Agents
                && app.codex.requested
                && codex.is_none()
                && let Some(context) = app.codex.context.clone()
            {
                app.codex.requested = false;
                app.codex.running = true;
                app.skills.running = true;
                match crate::codex::Collector::start_with(move |cancel| {
                    crate::codex::discover(&context, cancel)
                }) {
                    Ok(worker) => codex = Some(worker.detach_on_drop()),
                    Err(error) => app.finish_codex(Err(error)),
                }
            }
            if let Some(result) = codex.as_ref().and_then(crate::codex::Collector::poll) {
                app.finish_codex(result);
                app.skills.running = false;
                codex = None;
            }
            if app.module == crate::app::Module::Agents
                && app.claude.requested
                && claude.is_none()
                && let Some(context) = app.claude.context.clone()
            {
                app.claude.requested = false;
                app.claude.running = true;
                match crate::claude::Collector::start_with(move |cancel| {
                    crate::claude::discover(&context, cancel)
                }) {
                    Ok(worker) => claude = Some(worker.detach_on_drop()),
                    Err(error) => app.finish_claude(Err(error)),
                }
            }
            if app.module == crate::app::Module::Cloud
                && app.aws.requested
                && aws.is_none()
                && let Some(context) = app.aws.context.clone()
            {
                app.aws.requested = false;
                app.aws.running = true;
                match crate::aws::Collector::start_with(move |cancel| {
                    crate::aws::discover(&context, cancel)
                }) {
                    Ok(worker) => aws = Some(worker.detach_on_drop()),
                    Err(error) => app.aws.finish(Err(error)),
                }
            }
            if app.module == crate::app::Module::Cloud
                && app.kube.requested
                && kube.is_none()
                && let Some(context) = app.kube.context.clone()
            {
                app.kube.requested = false;
                app.kube.running = true;
                match crate::kube::Collector::start_with(move |cancel| {
                    crate::kube::discover(&context, cancel)
                }) {
                    Ok(worker) => kube = Some(worker.detach_on_drop()),
                    Err(error) => app.kube.finish(Err(error)),
                }
            }
            if let Some(result) = kube.as_ref().and_then(crate::kube::Collector::poll) {
                app.kube.finish(result);
                kube = None;
            }
            // Toolchains are read only when their area is open.
            if app.module == crate::app::Module::Toolchains
                && app.toolchains.requested
                && toolchains.is_none()
                && let Some(context) = app.toolchains.context.clone()
            {
                app.toolchains.requested = false;
                app.toolchains.running = true;
                match crate::toolchains::Collector::start_with(move |cancel| {
                    crate::toolchains::discover(&context, cancel)
                }) {
                    Ok(worker) => toolchains = Some(worker.detach_on_drop()),
                    Err(error) => app.toolchains.finish(Err(error)),
                }
            }
            if let Some(result) = toolchains
                .as_ref()
                .and_then(crate::toolchains::Collector::poll)
            {
                app.toolchains.finish(result);
                toolchains = None;
            }
            // Docker is collected only when its area is open: it runs the docker CLI.
            // While it stays open, live listings refresh so outside changes show up.
            if app.module == crate::app::Module::Docker {
                app.docker.tick(std::time::Instant::now());
            }
            if app.module == crate::app::Module::Docker
                && app.docker.requested
                && docker.is_none()
                && let Some(context) = app.docker.context.clone()
            {
                app.docker.requested = false;
                app.docker.running = true;
                match crate::docker::Collector::start_with(move |cancel| {
                    crate::docker::discover(&context, cancel)
                }) {
                    Ok(worker) => docker = Some(worker),
                    Err(error) => app.docker.finish(Err(error)),
                }
            }
            if let Some(result) = docker.as_ref().and_then(crate::docker::Collector::poll) {
                app.docker.finish(result);
                docker = None;
            }
            if let Some(result) = aws.as_ref().and_then(crate::aws::Collector::poll) {
                app.aws.finish(result);
                aws = None;
            }
            if let Some(result) = claude.as_ref().and_then(crate::claude::Collector::poll) {
                app.finish_claude(result);
                claude = None;
            }
            if app.module == crate::app::Module::Agents
                && app.codex.context.is_none()
                && app.skills.requested
                && skills.is_none()
                && let Some(config) = app.skills.config.clone()
            {
                app.skills.requested = false;
                app.skills.running = true;
                match crate::skills::Collector::start_with(move |cancel| {
                    crate::skills::discover(&config, cancel)
                }) {
                    Ok(worker) => skills = Some(worker.detach_on_drop()),
                    Err(error) => app.finish_skills(Err(error)),
                }
            }
            if let Some(result) = skills.as_ref().and_then(crate::skills::Collector::poll) {
                app.finish_skills(result);
                skills = None;
            }
            terminal.draw(|frame| ui::render(frame, app))?;
            // Poll during collection, and wait for the next Docker refresh while its area
            // is open; otherwise block on input.
            let collecting = collector.is_some()
                || diagnostic.is_some()
                || skills.is_some()
                || codex.is_some()
                || claude.is_some()
                || aws.is_some()
                || kube.is_some()
                || toolchains.is_some()
                || docker.is_some();
            let wait = if collecting {
                // About one frame: results show up promptly; idle still blocks on input.
                Some(std::time::Duration::from_millis(16))
            } else if app.module == crate::app::Module::Docker {
                app.docker.next_refresh(std::time::Instant::now())
            } else {
                None
            };
            if wait.map_or(Ok(true), event::poll)?
                && let Event::Key(key) = event::read()?
            {
                app.handle_key(key);
            }
        }
        Ok(())
    })
}

fn with_terminal(run: impl FnOnce(&mut DefaultTerminal) -> io::Result<()>) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other(
            "interactive terminal required; use --help for usage",
        ));
    }
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous_hook(info);
    }));
    let _restore = Restore;
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.hide_cursor()?;
    run(&mut terminal)
}

fn restore() {
    // Attempt both independently: an output failure must not prevent raw-mode cleanup.
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        ResetColor,
        SetAttribute(Attribute::Reset),
        Show,
        LeaveAlternateScreen
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a real PTY; run scripts/smoke_terminal.py"]
    fn restores_after_io_error() {
        let result = with_terminal(|_| Err(io::Error::other("injected event/draw failure")));
        assert_eq!(
            result.unwrap_err().to_string(),
            "injected event/draw failure"
        );
        assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    }

    #[test]
    #[ignore = "requires a real PTY; run scripts/smoke_terminal.py"]
    fn restores_after_panic() {
        let result = std::panic::catch_unwind(|| {
            let _ = with_terminal(|_| panic!("injected panic"));
        });
        assert!(result.is_err());
        assert!(!crossterm::terminal::is_raw_mode_enabled().unwrap());
    }
}
