use std::{env, ffi::OsString, io, path::PathBuf, process::ExitCode};

use den::{app::App, homebrew, inventory::safe_text, source, terminal};

const USAGE: &str = "Den - read-only local environment inspector (UX-2A)\n\n\
Usage: den [--demo | --snapshot FILE | --brew-snapshot FILE]\n\
       den [--project DIR] [--codex-profile NAME] [--skills]\n\
       den --skills-fixture DIR\n\
       den --help\n\n\
Without arguments, collects local Homebrew inventory; g opens the section switcher for Homebrew and Codex.\n\
--project DIR    Select project context (default: current working directory).\n\
--codex-profile NAME  Inspect a separate profile file; no session is launched.\n\
--codex-fixture DIR  Isolated home/.codex, project and system fixture.\n\
--skills         Start in Skills; Homebrew is collected only when opened.\n\
--skills-fixture DIR  Discover only an isolated user/project/admin/system fixture.\n\
--demo           Load embedded synthetic inventory without running Homebrew.\n\
--snapshot FILE  Load a local JSON inventory (schema_version: 1).\n\
--brew-snapshot FILE  Load raw Homebrew JSON without running Homebrew.\n\
--help, -h       Show this help without entering the TUI.\n\n\
Keys: g opens Sections; / searches; arrows/j/k select; Enter opens; Esc goes back.\n\
Tab switches inventory/overview or inspector sections/content; ? opens contextual help.\n\
Inspectors provide Overview and specialized sections, including Evidence. No display modes.\n\
F2/F3 and number keys remain secondary shortcuts; q/Ctrl-C quits.\n\
Read-only commands: brew info, missing, leaves, autoremove --dry-run.\n\
Snapshot modes never invoke Homebrew or scan live Codex files. Homebrew may fetch metadata.";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("den: {}", safe_text(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let mut project = None;
    let mut start_skills = false;
    let mut profile = None;
    let mut mode: Option<(&str, PathBuf)> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.to_str() {
            Some("--project") if project.is_none() => {
                project = Some(PathBuf::from(iter.next().ok_or_else(usage)?))
            }
            Some("--codex-profile") if profile.is_none() => {
                let value = iter
                    .next()
                    .and_then(|s| s.to_str())
                    .filter(|s| den::codex::identifier(s))
                    .ok_or_else(usage)?;
                profile = Some(value.to_owned());
            }
            Some("--skills") if !start_skills => start_skills = true,
            Some("--demo") if mode.is_none() => mode = Some(("demo", PathBuf::new())),
            Some(
                flag @ ("--snapshot" | "--brew-snapshot" | "--skills-fixture" | "--codex-fixture"),
            ) if mode.is_none() => {
                mode = Some((flag, PathBuf::from(iter.next().ok_or_else(usage)?)))
            }
            _ => return Err(usage()),
        }
    }
    if (mode.is_some() && project.is_some())
        || (profile.is_some()
            && mode
                .as_ref()
                .is_some_and(|(flag, _)| *flag != "--codex-fixture"))
    {
        return Err(usage());
    }
    let mut app = match mode {
        None => {
            let mut app = App::live_homebrew();
            app.codex.configure(den::codex::Context::local(
                project.unwrap_or(env::current_dir()?),
                profile.clone(),
            )?);
            app
        }
        Some(("demo", _)) => App::new(source::demo()?, "synthetic demo"),
        Some(("--snapshot", path)) => {
            App::new(source::load_snapshot(&path)?, "snapshot (unverified)")
        }
        Some(("--brew-snapshot", path)) => App::new(
            homebrew::load_snapshot(&path).map_err(io::Error::other)?,
            "Homebrew snapshot",
        ),
        Some(("--codex-fixture", path)) => {
            let mut app = App::new(
                den::inventory::Inventory {
                    schema_version: 1,
                    resources: vec![],
                },
                "isolated Codex fixture",
            );
            app.codex
                .configure(den::codex::Context::fixture(&path, profile.clone())?);
            start_skills = true;
            app
        }
        Some(("--skills-fixture", path)) => {
            let mut app = App::new(
                den::inventory::Inventory {
                    schema_version: 1,
                    resources: vec![],
                },
                "fixture (Homebrew unavailable)",
            );
            app.skills.configure(den::skills::Config::fixture(&path)?);
            start_skills = true;
            app
        }
        _ => return Err(usage()),
    };
    if start_skills {
        app.module = den::app::Module::Skills;
    }
    terminal::run(&mut app)
}

fn usage() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: den [--project DIR] [--codex-profile NAME] [--skills] | --demo | --snapshot FILE | --brew-snapshot FILE | --skills-fixture DIR",
    )
}
