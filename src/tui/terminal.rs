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
    areas::homebrew::diagnostics::Check,
    areas::homebrew::{Collector, CommandOutput, diagnose},
    tui::ui,
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
        let mut skills: Option<crate::areas::agents::skills::Collector> = None;
        let mut codex: Option<crate::areas::agents::codex::Collector> = None;
        let mut claude: Option<crate::areas::agents::claude::Collector> = None;
        let mut aws: Option<crate::areas::cloud::aws::Collector> = None;
        let mut kube: Option<crate::areas::cloud::kube::Collector> = None;
        let mut toolchains: Option<crate::areas::toolchains::Collector> = None;
        let mut docker: Option<crate::areas::docker::Collector> = None;
        let mut diagnostic: Option<(Check, u64, Collector<CommandOutput>)> = None;
        // A write action's preview (false) or the action itself (true), with its action.
        let mut action: Option<(bool, crate::actions::Action, crate::actions::Worker)> = None;
        let log = crate::actions::log_path();
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
                app.homebrew_data = crate::areas::homebrew::cache_dir()
                    .and_then(|c| crate::areas::homebrew::data_refreshed(&c));
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
            // Opening Agents collects Codex and Claude Code; each has its own worker. The
            // Problems overview reads every file-based area too (not Docker, which runs docker).
            if (app.module == crate::app::Module::Agents || app.problems.is_some())
                && app.codex.requested
                && codex.is_none()
                && let Some(context) = app.codex.context.clone()
            {
                app.codex.requested = false;
                app.codex.running = true;
                app.skills.running = true;
                match crate::areas::agents::codex::Collector::start_with(move |cancel| {
                    crate::areas::agents::codex::discover(&context, cancel)
                }) {
                    Ok(worker) => codex = Some(worker.detach_on_drop()),
                    Err(error) => app.finish_codex(Err(error)),
                }
            }
            if let Some(result) = codex
                .as_ref()
                .and_then(crate::areas::agents::codex::Collector::poll)
            {
                app.finish_codex(result);
                app.skills.running = false;
                codex = None;
            }
            if (app.module == crate::app::Module::Agents || app.problems.is_some())
                && app.claude.requested
                && claude.is_none()
                && let Some(context) = app.claude.context.clone()
            {
                app.claude.requested = false;
                app.claude.running = true;
                match crate::areas::agents::claude::Collector::start_with(move |cancel| {
                    crate::areas::agents::claude::discover(&context, cancel)
                }) {
                    Ok(worker) => claude = Some(worker.detach_on_drop()),
                    Err(error) => app.finish_claude(Err(error)),
                }
            }
            if (app.module == crate::app::Module::Cloud || app.problems.is_some())
                && app.aws.requested
                && aws.is_none()
                && let Some(context) = app.aws.context.clone()
            {
                app.aws.requested = false;
                app.aws.running = true;
                match crate::areas::cloud::aws::Collector::start_with(move |cancel| {
                    crate::areas::cloud::aws::discover(&context, cancel)
                }) {
                    Ok(worker) => aws = Some(worker.detach_on_drop()),
                    Err(error) => app.aws.finish(Err(error)),
                }
            }
            if (app.module == crate::app::Module::Cloud || app.problems.is_some())
                && app.kube.requested
                && kube.is_none()
                && let Some(context) = app.kube.context.clone()
            {
                app.kube.requested = false;
                app.kube.running = true;
                match crate::areas::cloud::kube::Collector::start_with(move |cancel| {
                    crate::areas::cloud::kube::discover(&context, cancel)
                }) {
                    Ok(worker) => kube = Some(worker.detach_on_drop()),
                    Err(error) => app.kube.finish(Err(error)),
                }
            }
            if let Some(result) = kube
                .as_ref()
                .and_then(crate::areas::cloud::kube::Collector::poll)
            {
                app.kube.finish(result);
                kube = None;
            }
            // Toolchains are read only when their area is open.
            if (app.module == crate::app::Module::Toolchains || app.problems.is_some())
                && app.toolchains.requested
                && toolchains.is_none()
                && let Some(context) = app.toolchains.context.clone()
            {
                app.toolchains.requested = false;
                app.toolchains.running = true;
                match crate::areas::toolchains::Collector::start_with(move |cancel| {
                    crate::areas::toolchains::discover(&context, cancel)
                }) {
                    Ok(worker) => toolchains = Some(worker.detach_on_drop()),
                    Err(error) => app.toolchains.finish(Err(error)),
                }
            }
            if let Some(result) = toolchains
                .as_ref()
                .and_then(crate::areas::toolchains::Collector::poll)
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
                match crate::areas::docker::Collector::start_with(move |cancel| {
                    crate::areas::docker::discover(&context, cancel)
                }) {
                    Ok(worker) => docker = Some(worker),
                    Err(error) => app.docker.finish(Err(error)),
                }
            }
            if let Some(result) = docker
                .as_ref()
                .and_then(crate::areas::docker::Collector::poll)
            {
                app.docker.finish(result);
                docker = None;
            }
            if let Some(result) = aws
                .as_ref()
                .and_then(crate::areas::cloud::aws::Collector::poll)
            {
                app.aws.finish(result);
                aws = None;
            }
            if let Some(result) = claude
                .as_ref()
                .and_then(crate::areas::agents::claude::Collector::poll)
            {
                app.finish_claude(result);
                claude = None;
            }
            if (app.module == crate::app::Module::Agents || app.problems.is_some())
                && app.codex.context.is_none()
                && app.skills.requested
                && skills.is_none()
                && let Some(config) = app.skills.config.clone()
            {
                app.skills.requested = false;
                app.skills.running = true;
                match crate::areas::agents::skills::Collector::start_with(move |cancel| {
                    crate::areas::agents::skills::discover(&config, cancel)
                }) {
                    Ok(worker) => skills = Some(worker.detach_on_drop()),
                    Err(error) => app.finish_skills(Err(error)),
                }
            }
            if let Some(result) = skills
                .as_ref()
                .and_then(crate::areas::agents::skills::Collector::poll)
            {
                app.finish_skills(result);
                skills = None;
            }
            // A new request replaces a preview still running (the selection moved), but
            // never a running action.
            if !matches!(action, Some((true, ..)))
                && let Some(request) = app.actions.request.take()
            {
                use crate::actions::Request;
                action = None;
                let (run, chosen) = match &request {
                    Request::Preview(a) => (false, a.clone()),
                    Request::Run(a) => (true, a.clone()),
                };
                let started = match request {
                    Request::Preview(a) => crate::actions::start_preview(a),
                    // Only dew creates a permit.
                    Request::Run(a) => match app.actions.permit() {
                        Some(permit) => crate::actions::start(a, permit),
                        None => Err(crate::areas::homebrew::CollectionError::Invalid(
                            "den is read-only",
                        )),
                    },
                };
                match started {
                    Ok(worker) => action = Some((run, chosen, worker)),
                    Err(error) if run => app.finish_action(Err(error), log.as_deref()),
                    Err(error) => app.actions.finish_preview(&chosen, Err(error)),
                }
            }
            if let Some((run, _, worker)) = &action {
                if *run && app.actions.cancel_requested {
                    worker.cancel();
                }
                // A closed dialog no longer needs its preview: dropping stops it.
                if !*run
                    && !matches!(
                        app.actions.dialog,
                        Some(crate::actions::Dialog::Menu { .. })
                    )
                {
                    action = None;
                }
            }
            if let Some((run, chosen, worker)) = &action
                && let Some(result) = worker.poll()
            {
                if *run {
                    app.finish_action(result, log.as_deref());
                } else {
                    app.actions.finish_preview(chosen, result);
                }
                action = None;
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
                || docker.is_some()
                || action.is_some();
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
                // A bare Esc may be the start of a sequence the OS split (F3, Page Down).
                let keys = crate::tui::keys::complete(key, || {
                    if event::poll(crate::tui::keys::ESC_WAIT)? {
                        event::read().map(Some)
                    } else {
                        Ok(None)
                    }
                })?;
                for key in keys {
                    app.handle_key(key);
                }
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
