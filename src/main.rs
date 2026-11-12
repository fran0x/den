use std::{env, io, path::PathBuf, process::ExitCode};

use clap::{Args, Parser};
use den::{
    app::{App, CloudSection, Module},
    areas::homebrew,
    inventory::{Inventory, safe_text},
    source,
    tui::terminal,
};

const AFTER_HELP: &str = "\
Without a source, Den inspects this machine; Tab switches Homebrew, Agents, Cloud, Docker \
and Toolchains, Shift+Tab switches sections, / searches, ? shows help and q quits.

Read-only unless started with --allow-writes; even then, W turns writes on, x opens the \
actions, and each one runs only after you confirm it with y. Otherwise Den runs only brew info, missing, leaves and autoremove --dry-run, and docker \
container ls, image ls, volume ls and system df while the Docker area is open. Kubeconfig \
and toolchain files are read, never used. Homebrew may fetch metadata.";

/// A read-only terminal inspector for your developer machine.
#[derive(Parser, Debug)]
#[command(name = "den", version, about, after_help = AFTER_HELP)]
struct Cli {
    /// Project folder for Agents [default: the current folder]
    #[arg(long, value_name = "DIR", conflicts_with = "source")]
    project: Option<PathBuf>,

    /// Inspect $CODEX_HOME/NAME.config.toml; no session is launched
    #[arg(
        long,
        value_name = "NAME",
        value_parser = profile,
        conflicts_with_all = [
            "demo", "snapshot", "brew_snapshot", "skills_fixture", "claude_fixture",
            "aws_fixture", "kube_fixture", "docker_fixture", "toolchains_fixture",
        ],
    )]
    codex_profile: Option<String>,

    /// Start in Agents › Skills; Homebrew loads when opened
    #[arg(long)]
    skills: bool,

    /// Allow write actions (upgrade, uninstall, cleanup); each still needs W, then y
    #[arg(long, conflicts_with = "source")]
    allow_writes: bool,

    #[command(flatten)]
    source: Source,
}

/// Offline sources: synthetic or saved data instead of this machine. At most one.
#[derive(Args, Debug)]
#[group(id = "source", multiple = false)]
struct Source {
    /// Embedded synthetic inventory; nothing is collected
    #[arg(long, help_heading = "Offline sources")]
    demo: bool,
    /// A Den JSON inventory (schema_version 1)
    #[arg(long, value_name = "FILE", help_heading = "Offline sources")]
    snapshot: Option<PathBuf>,
    /// Raw Homebrew JSON, without running Homebrew
    #[arg(long, value_name = "FILE", help_heading = "Offline sources")]
    brew_snapshot: Option<PathBuf>,
    /// Isolated skill roots (user, project, admin, system)
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    skills_fixture: Option<PathBuf>,
    /// Isolated Codex home, project and system folders
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    codex_fixture: Option<PathBuf>,
    /// Isolated Codex and Claude Code folders
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    claude_fixture: Option<PathBuf>,
    /// Isolated home/.aws for Cloud › AWS
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    aws_fixture: Option<PathBuf>,
    /// Isolated home/.kube/config for Cloud › Kubernetes
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    kube_fixture: Option<PathBuf>,
    /// Saved docker JSON lines instead of running docker
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    docker_fixture: Option<PathBuf>,
    /// Isolated PATH, home and system folders for Toolchains
    #[arg(long, value_name = "DIR", help_heading = "Offline sources")]
    toolchains_fixture: Option<PathBuf>,
}

/// Where Den's data comes from: this machine, or exactly one offline source.
#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Live,
    Demo,
    Snapshot(PathBuf),
    BrewSnapshot(PathBuf),
    Skills(PathBuf),
    Codex(PathBuf),
    Claude(PathBuf),
    Aws(PathBuf),
    Kube(PathBuf),
    Docker(PathBuf),
    Toolchains(PathBuf),
}

impl Source {
    /// The group allows at most one source, so the order here does not matter.
    fn mode(self) -> Mode {
        let Self {
            demo,
            snapshot,
            brew_snapshot,
            skills_fixture,
            codex_fixture,
            claude_fixture,
            aws_fixture,
            kube_fixture,
            docker_fixture,
            toolchains_fixture,
        } = self;
        if demo {
            return Mode::Demo;
        }
        snapshot
            .map(Mode::Snapshot)
            .or(brew_snapshot.map(Mode::BrewSnapshot))
            .or(skills_fixture.map(Mode::Skills))
            .or(codex_fixture.map(Mode::Codex))
            .or(claude_fixture.map(Mode::Claude))
            .or(aws_fixture.map(Mode::Aws))
            .or(kube_fixture.map(Mode::Kube))
            .or(docker_fixture.map(Mode::Docker))
            .or(toolchains_fixture.map(Mode::Toolchains))
            .unwrap_or(Mode::Live)
    }
}

fn profile(name: &str) -> Result<String, String> {
    if den::areas::agents::codex::identifier(name) {
        Ok(name.to_owned())
    } else {
        Err("use up to 64 letters, digits, '-' or '_'".into())
    }
}

fn main() -> ExitCode {
    // Parse errors, --help and --version are reported by clap (usage errors exit with 2).
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("den: {}", safe_text(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> io::Result<()> {
    let mut app = app(cli)?;
    den::tui::ui::set_theme(den::tui::ui::Theme::from_env());
    terminal::run(&mut app)
}

/// An app with no Homebrew inventory, for offline sources that inspect one area.
fn offline(label: &'static str) -> App {
    App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        label,
    )
}

fn app(cli: Cli) -> io::Result<App> {
    let profile = cli.codex_profile;
    let permit = den::actions::Permit::from_flag(cli.allow_writes);
    let (mut app, start) = match cli.source.mode() {
        Mode::Live => {
            let mut app = App::live_homebrew();
            let project = match cli.project {
                Some(project) => project,
                None => env::current_dir()?,
            };
            app.codex
                .configure(den::areas::agents::codex::Context::local(
                    project.clone(),
                    profile,
                )?);
            app.claude
                .configure(den::areas::agents::claude::Context::local(project)?);
            app.aws.configure(den::areas::cloud::aws::Context::local()?);
            app.kube
                .configure(den::areas::cloud::kube::Context::local());
            app.toolchains
                .configure(den::areas::toolchains::Context::local());
            app.docker.configure(den::areas::docker::Context::local());
            (app, None)
        }
        Mode::Demo => (App::new(source::demo()?, "synthetic demo"), None),
        Mode::Snapshot(path) => (
            App::new(source::load_snapshot(&path)?, "snapshot (unverified)"),
            None,
        ),
        Mode::BrewSnapshot(path) => (
            App::new(
                homebrew::load_snapshot(&path).map_err(io::Error::other)?,
                "Homebrew snapshot",
            ),
            None,
        ),
        Mode::Skills(path) => {
            let mut app = offline("fixture (Homebrew unavailable)");
            app.skills
                .configure(den::areas::agents::skills::Config::fixture(&path)?);
            (app, Some(Module::Agents))
        }
        Mode::Codex(path) => {
            let mut app = offline("isolated Codex fixture");
            app.codex
                .configure(den::areas::agents::codex::Context::fixture(&path, profile)?);
            (app, Some(Module::Agents))
        }
        Mode::Claude(path) => {
            let mut app = offline("isolated Claude fixture");
            app.codex
                .configure(den::areas::agents::codex::Context::fixture(&path, None)?);
            app.claude
                .configure(den::areas::agents::claude::Context::fixture(&path)?);
            (app, Some(Module::Agents))
        }
        Mode::Aws(path) => {
            let mut app = offline("isolated AWS fixture");
            app.aws
                .configure(den::areas::cloud::aws::Context::fixture(&path)?);
            (app, Some(Module::Cloud))
        }
        Mode::Kube(path) => {
            let mut app = offline("isolated Kubernetes fixture");
            app.kube
                .configure(den::areas::cloud::kube::Context::fixture(&path)?);
            app.cloud = CloudSection::Kubernetes;
            (app, Some(Module::Cloud))
        }
        Mode::Docker(path) => {
            let mut app = offline("isolated Docker fixture");
            app.docker
                .configure(den::areas::docker::Context::fixture(&path)?);
            (app, Some(Module::Docker))
        }
        Mode::Toolchains(path) => {
            let mut app = offline("isolated toolchains fixture");
            app.toolchains
                .configure(den::areas::toolchains::Context::fixture(&path)?);
            (app, Some(Module::Toolchains))
        }
    };
    if let Some(module) = start.or(cli.skills.then_some(Module::Agents)) {
        app.module = module;
    }
    app.actions = den::actions::State::new(permit);
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, error::ErrorKind};

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("den").chain(args.iter().copied()))
    }
    fn kind(args: &[&str]) -> ErrorKind {
        parse(args).map(|_| ()).unwrap_err().kind()
    }

    #[test]
    fn definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn sources_map_to_modes() {
        assert_eq!(parse(&[]).unwrap().source.mode(), Mode::Live);
        assert_eq!(parse(&["--demo"]).unwrap().source.mode(), Mode::Demo);
        assert_eq!(
            parse(&["--kube-fixture", "k"]).unwrap().source.mode(),
            Mode::Kube("k".into())
        );
        assert_eq!(
            parse(&["--toolchains-fixture", "t"]).unwrap().source.mode(),
            Mode::Toolchains("t".into())
        );
    }

    #[test]
    fn mistakes_are_named_not_generic() {
        assert_eq!(
            kind(&["--demo", "--aws-fixture", "a"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["--demo", "--project", "p"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["--demo", "--codex-profile", "review"]),
            ErrorKind::ArgumentConflict
        );
        // Writes only ever apply to this machine, never to offline data.
        assert_eq!(
            kind(&["--allow-writes", "--snapshot", "s.json"]),
            ErrorKind::ArgumentConflict
        );
        assert!(parse(&["--allow-writes"]).unwrap().allow_writes);
        assert!(!parse(&[]).unwrap().allow_writes);
        assert_eq!(kind(&["--project"]), ErrorKind::InvalidValue);
        assert_eq!(
            kind(&["--codex-profile", "../x"]),
            ErrorKind::ValueValidation
        );
        let typo = parse(&["--porject", "/tmp"]).map(|_| ()).unwrap_err();
        assert_eq!(typo.kind(), ErrorKind::UnknownArgument);
        assert!(typo.to_string().contains("--project"), "{typo}");
    }

    #[test]
    fn help_and_version_work_anywhere() {
        assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
        assert_eq!(kind(&["--demo", "--help"]), ErrorKind::DisplayHelp);
    }

    #[test]
    fn a_codex_profile_works_live_and_with_the_codex_fixture() {
        let cli = parse(&["--codex-profile", "review"]).unwrap();
        assert_eq!(cli.codex_profile.as_deref(), Some("review"));
        let cli = parse(&["--codex-fixture", "c", "--codex-profile", "review"]).unwrap();
        assert_eq!(cli.source.mode(), Mode::Codex("c".into()));
    }
}
