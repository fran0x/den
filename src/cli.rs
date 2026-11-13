//! The command line shared by `den` (read-only) and `dew` (writes allowed). The two
//! commands differ only in [`Program`]: `den` cannot create a write permit at all.
use std::{env, io, path::PathBuf, process::ExitCode};

use clap::{Args, CommandFactory, FromArgMatches, Parser, error::ErrorKind};

use crate::{
    actions::Permit,
    app::{App, CloudSection, Module},
    areas::homebrew,
    inventory::{Inventory, safe_text},
    source,
    tui::terminal,
};

/// Which command is running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    /// Read-only, always.
    Den,
    /// Den with Homebrew write actions allowed; each is still confirmed.
    Dew,
}
impl Program {
    fn name(self) -> &'static str {
        match self {
            Self::Den => "den",
            Self::Dew => "dew",
        }
    }
    fn about(self) -> &'static str {
        match self {
            Self::Den => "A read-only terminal inspector for your developer machine.",
            Self::Dew => {
                "Den with Homebrew write actions allowed: upgrade, uninstall and clean up, each \
                 confirmed."
            }
        }
    }
    fn after_help(self) -> &'static str {
        match self {
            Self::Den => {
                "\
Without a source, Den inspects this machine; Tab switches Homebrew, Agents, Cloud, Docker \
and Toolchains, Shift+Tab switches sections, / searches, ? shows help and q quits.

Read-only: Den runs only brew info, missing, leaves and autoremove --dry-run, and docker \
container ls, image ls, volume ls and system df while the Docker area is open. Kubeconfig \
and toolchain files are read, never used. Homebrew may fetch metadata. To make changes, \
run dew instead."
            }
            Self::Dew => {
                "\
dew is den with write actions allowed (the top bar shows WRITE): x in Homebrew shows the \
package and its actions, each with its command and a dry run, and Enter runs the chosen \
one. dew takes no offline source: it only changes this machine."
            }
        }
    }
}

#[derive(Parser, Debug)]
#[command(version)]
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

    #[command(flatten)]
    source: Source,
}

/// Offline sources: synthetic or saved data instead of this machine. At most one.
#[derive(Args, Debug, Clone)]
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
    if crate::areas::agents::codex::identifier(name) {
        Ok(name.to_owned())
    } else {
        Err("use up to 64 letters, digits, '-' or '_'".into())
    }
}

/// Runs `den` or `dew`. Parse errors, --help and --version are reported by clap (usage
/// errors exit with 2).
pub fn main(program: Program) -> ExitCode {
    let cli = match parse(program, env::args_os()) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    match run(program, cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}: {}", program.name(), safe_text(&error.to_string()));
            ExitCode::FAILURE
        }
    }
}

fn parse<I, T>(program: Program, args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut command = Cli::command()
        .name(program.name())
        .bin_name(program.name())
        .about(program.about())
        .after_help(program.after_help());
    if program == Program::Dew {
        // dew refuses offline sources, so its help does not offer them.
        command = command.mut_args(|arg| {
            let offline = arg.get_help_heading() == Some("Offline sources");
            arg.hide(offline)
        });
    }
    let matches = command.try_get_matches_from_mut(args)?;
    let cli = Cli::from_arg_matches(&matches)?;
    if program == Program::Dew && cli.source.clone().mode() != Mode::Live {
        return Err(command.error(
            ErrorKind::ArgumentConflict,
            "dew changes this machine, so it takes no offline source; use den to browse one",
        ));
    }
    Ok(cli)
}

fn run(program: Program, cli: Cli) -> io::Result<()> {
    let mut app = app(program, cli)?;
    crate::tui::ui::set_theme(crate::tui::ui::Theme::from_env());
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

fn app(program: Program, cli: Cli) -> io::Result<App> {
    let profile = cli.codex_profile;
    // The only place a write permit is created: running dew.
    let permit = Permit::granted(program == Program::Dew);
    let (mut app, start) = match cli.source.mode() {
        Mode::Live => {
            let mut app = App::live_homebrew();
            let project = match cli.project {
                Some(project) => project,
                None => env::current_dir()?,
            };
            app.codex
                .configure(crate::areas::agents::codex::Context::local(
                    project.clone(),
                    profile,
                )?);
            app.claude
                .configure(crate::areas::agents::claude::Context::local(project)?);
            app.aws
                .configure(crate::areas::cloud::aws::Context::local()?);
            app.kube
                .configure(crate::areas::cloud::kube::Context::local());
            app.toolchains
                .configure(crate::areas::toolchains::Context::local());
            app.docker.configure(crate::areas::docker::Context::local());
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
                .configure(crate::areas::agents::skills::Config::fixture(&path)?);
            (app, Some(Module::Agents))
        }
        Mode::Codex(path) => {
            let mut app = offline("isolated Codex fixture");
            app.codex
                .configure(crate::areas::agents::codex::Context::fixture(
                    &path, profile,
                )?);
            (app, Some(Module::Agents))
        }
        Mode::Claude(path) => {
            let mut app = offline("isolated Claude fixture");
            app.codex
                .configure(crate::areas::agents::codex::Context::fixture(&path, None)?);
            app.claude
                .configure(crate::areas::agents::claude::Context::fixture(&path)?);
            (app, Some(Module::Agents))
        }
        Mode::Aws(path) => {
            let mut app = offline("isolated AWS fixture");
            app.aws
                .configure(crate::areas::cloud::aws::Context::fixture(&path)?);
            (app, Some(Module::Cloud))
        }
        Mode::Kube(path) => {
            let mut app = offline("isolated Kubernetes fixture");
            app.kube
                .configure(crate::areas::cloud::kube::Context::fixture(&path)?);
            app.cloud = CloudSection::Kubernetes;
            (app, Some(Module::Cloud))
        }
        Mode::Docker(path) => {
            let mut app = offline("isolated Docker fixture");
            app.docker
                .configure(crate::areas::docker::Context::fixture(&path)?);
            (app, Some(Module::Docker))
        }
        Mode::Toolchains(path) => {
            let mut app = offline("isolated toolchains fixture");
            app.toolchains
                .configure(crate::areas::toolchains::Context::fixture(&path)?);
            (app, Some(Module::Toolchains))
        }
    };
    if let Some(module) = start.or(cli.skills.then_some(Module::Agents)) {
        app.module = module;
    }
    app.actions = crate::actions::State::new(permit);
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn den(args: &[&str]) -> Result<Cli, clap::Error> {
        super::parse(
            Program::Den,
            std::iter::once("den").chain(args.iter().copied()),
        )
    }
    fn dew(args: &[&str]) -> Result<Cli, clap::Error> {
        super::parse(
            Program::Dew,
            std::iter::once("dew").chain(args.iter().copied()),
        )
    }
    fn kind(args: &[&str]) -> ErrorKind {
        den(args).map(|_| ()).unwrap_err().kind()
    }

    #[test]
    fn definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn sources_map_to_modes() {
        assert_eq!(den(&[]).unwrap().source.mode(), Mode::Live);
        assert_eq!(den(&["--demo"]).unwrap().source.mode(), Mode::Demo);
        assert_eq!(
            den(&["--kube-fixture", "k"]).unwrap().source.mode(),
            Mode::Kube("k".into())
        );
        assert_eq!(
            den(&["--toolchains-fixture", "t"]).unwrap().source.mode(),
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
        // den has no way to allow writes.
        assert_eq!(kind(&["--allow-writes"]), ErrorKind::UnknownArgument);
        assert_eq!(kind(&["--project"]), ErrorKind::InvalidValue);
        assert_eq!(
            kind(&["--codex-profile", "../x"]),
            ErrorKind::ValueValidation
        );
        let typo = den(&["--porject", "/tmp"]).map(|_| ()).unwrap_err();
        assert_eq!(typo.kind(), ErrorKind::UnknownArgument);
        assert!(typo.to_string().contains("--project"), "{typo}");
    }

    #[test]
    fn dew_allows_writes_on_this_machine_only() {
        assert!(dew(&[]).is_ok());
        assert!(dew(&["--project", "p", "--skills"]).is_ok());
        let offline = dew(&["--snapshot", "s.json"]).map(|_| ()).unwrap_err();
        assert_eq!(offline.kind(), ErrorKind::ArgumentConflict);
        assert!(offline.to_string().contains("use den"), "{offline}");
        assert_eq!(
            dew(&["--demo"]).map(|_| ()).unwrap_err().kind(),
            ErrorKind::ArgumentConflict
        );
        // The permit exists only for dew.
        let den_app = app(Program::Den, den(&["--demo"]).unwrap()).unwrap();
        assert!(!den_app.actions.allowed());
        let help = dew(&["--help"]).map(|_| ()).unwrap_err().to_string();
        assert!(
            help.contains("dew is den with write actions allowed"),
            "{help}"
        );
        assert!(!help.contains("--snapshot"), "{help}");
        let help = den(&["--help"]).map(|_| ()).unwrap_err().to_string();
        assert!(help.contains("run dew instead"), "{help}");
        assert!(help.contains("--snapshot"), "{help}");
    }

    #[test]
    fn help_and_version_work_anywhere() {
        assert_eq!(kind(&["--version"]), ErrorKind::DisplayVersion);
        assert_eq!(kind(&["--demo", "--help"]), ErrorKind::DisplayHelp);
    }

    #[test]
    fn a_codex_profile_works_live_and_with_the_codex_fixture() {
        let cli = den(&["--codex-profile", "review"]).unwrap();
        assert_eq!(cli.codex_profile.as_deref(), Some("review"));
        let cli = den(&["--codex-fixture", "c", "--codex-profile", "review"]).unwrap();
        assert_eq!(cli.source.mode(), Mode::Codex("c".into()));
    }
}
