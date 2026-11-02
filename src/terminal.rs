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
            if app.module == crate::app::Module::Skills
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
                    Err(error) => app.codex.finish(Err(error), &mut app.skills),
                }
            }
            if let Some(result) = codex.as_ref().and_then(crate::codex::Collector::poll) {
                app.codex.finish(result, &mut app.skills);
                app.skills.running = false;
                codex = None;
            }
            if app.module == crate::app::Module::Skills
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
                    Err(error) => app.skills.finish(Err(error)),
                }
            }
            if let Some(result) = skills.as_ref().and_then(crate::skills::Collector::poll) {
                app.skills.finish(result);
                skills = None;
            }
            terminal.draw(|frame| ui::render(frame, app))?;
            // Poll only during collection; otherwise preserve Stage 0's blocking input.
            if ((collector.is_none()
                && diagnostic.is_none()
                && skills.is_none()
                && codex.is_none())
                || event::poll(std::time::Duration::from_millis(50))?)
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
