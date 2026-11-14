// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Online checks with `u`: asked before anything is sent, only package names, newer
//! versions marked where Homebrew's are, kept privately for the next run, and nothing in
//! demo or fixture modes. The registries are never contacted here: answers are given to
//! the app as a finished check would give them.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::App,
    areas::toolchains,
    inventory::Inventory,
    online::{self, Answer, Package, Registry},
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::SystemTime,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "den-online-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn file(&self) -> PathBuf {
        self.0.join("state/den/online.json")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(a: &mut App) -> String {
    let (w, h) = (170, 34);
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| t.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// A session that has read the Toolchains fixture, keeping online answers at `path`.
fn session(path: Option<&Path>) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "online test",
    );
    if let Some(path) = path
        && let Some(notice) = a.online.use_file(path.to_owned())
    {
        a.actions.notice = Some(notice);
    }
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    let read = toolchains::discover(&context, &AtomicBool::new(false)).unwrap();
    a.toolchains.configure(context);
    a.toolchains.finish(Ok(read));
    a.toolchains.requested = false;
    a.navigate(Destination::Packages);
    a
}
fn package(registry: Registry, name: &str) -> Package {
    Package::new(registry, name).unwrap()
}
/// What the registries would answer for the fixture.
fn answers() -> Vec<(Package, Answer)> {
    vec![
        (
            package(Registry::CratesIo, "bacon"),
            Answer::Latest("3.27.0".into()),
        ),
        (package(Registry::Npm, "@acme/cli"), Answer::NotFound),
        (
            package(Registry::Npm, "npm"),
            Answer::Failed("timed out".into()),
        ),
        (
            package(Registry::Npm, "prettier"),
            Answer::Latest("3.5.0".into()),
        ),
        (
            package(Registry::PyPI, "httpie"),
            Answer::Latest("3.2.4".into()),
        ),
        (
            package(Registry::PyPI, "ruff"),
            Answer::Latest("0.17.5".into()),
        ),
    ]
}

#[test]
fn u_asks_first_and_names_what_it_sends() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()));
    assert!(draw(&mut a).contains("u:check online"));
    key(&mut a, KeyCode::Char('u'));
    let screen = draw(&mut a);
    assert!(
        screen.contains(
            "Check online? Enter:check Esc:cancel · sends 6 package names to index.crates.io, \
             registry.npmjs.org, pypi.org; versions stay here"
        ),
        "{screen}"
    );
    // Esc: nothing is asked for and nothing is written.
    key(&mut a, KeyCode::Esc);
    assert!(draw(&mut a).contains("Nothing was checked online"));
    assert!(a.online.request.is_none());
    assert!(!t.file().exists());

    key(&mut a, KeyCode::Char('u'));
    key(&mut a, KeyCode::Enter);
    let asked: Vec<String> = a
        .online
        .request
        .clone()
        .unwrap()
        .iter()
        .map(Package::key)
        .collect();
    // Crates from git or a folder are not looked up.
    assert_eq!(
        asked,
        [
            "crates.io:bacon",
            "npm:@acme/cli",
            "npm:npm",
            "npm:prettier",
            "PyPI:httpie",
            "PyPI:ruff",
        ]
    );
    assert!(draw(&mut a).contains("checking 6 packages online… Esc:stop"));
    // Esc while it runs asks the check to stop.
    key(&mut a, KeyCode::Esc);
    assert!(a.online.stop);
}

#[test]
fn newer_versions_are_marked_and_kept_for_the_next_run() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()));
    key(&mut a, KeyCode::Char('u'));
    key(&mut a, KeyCode::Enter);
    a.online.request = None;
    a.finish_online(Ok(answers()), SystemTime::now());
    let screen = draw(&mut a);
    assert!(
        screen.contains("Checked 6 packages online: 3 newer · 1 failed"),
        "{screen}"
    );
    // The notice clears the rest of the status line.
    assert!(!screen.contains("failed 1 · pipx"), "{screen}");
    key(&mut a, KeyCode::Down);
    let screen = draw(&mut a);
    assert!(screen.contains("↑ 0.17.5 · Ready · 0.9.1"), "{screen}");
    assert!(screen.contains("↑ 3.5.0 · Ready · 3.4.2"), "{screen}");
    assert!(
        screen.contains("2 newer online, checked just now"),
        "{screen}"
    );
    // The panel says where and when.
    a.toolchains.select_key("uv tool:ruff");
    let screen = draw(&mut a);
    assert!(
        screen.contains("↑ 0.17.5 available on PyPI · checked online just now"),
        "{screen}"
    );
    a.toolchains.select_key("pipx:httpie");
    assert!(draw(&mut a).contains("Up to date on PyPI (newest 3.2.4)"));
    a.toolchains.select_key("npm -g:npm");
    assert!(draw(&mut a).contains("Online check failed: timed out"));
    a.toolchains.select_key("npm -g:@acme/cli");
    assert!(draw(&mut a).contains("Not found on npm"));

    // A newer version is not a change.
    assert!(a.toolchains.changes.changes().is_empty());

    // Saved privately, and read on the next run without checking again.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&t.file()), 0o600);
        assert_eq!(mode(t.file().parent().unwrap()), 0o700);
    }
    let saved = fs::read_to_string(t.file()).unwrap();
    assert!(!saved.contains("0.9.1"), "installed versions stay out");
    let mut b = session(Some(&t.file()));
    assert!(draw(&mut b).contains("↑ 0.17.5 · Ready · 0.9.1"));
    assert!(b.online.request.is_none());
}

#[test]
fn rust_crates_show_their_newer_version() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()));
    a.finish_online(Ok(answers()), SystemTime::now());
    a.navigate(Destination::Rust);
    // The notice gives way to the status line at the next key.
    key(&mut a, KeyCode::Down);
    let screen = draw(&mut a);
    assert!(screen.contains("↑ 3.27.0 · Ready · 3.26.0"), "{screen}");
    assert!(screen.contains("1 newer online"), "{screen}");
}

#[test]
fn only_live_data_is_checked() {
    let mut a = session(None);
    assert!(!draw(&mut a).contains("u:check online"));
    key(&mut a, KeyCode::Char('u'));
    assert!(draw(&mut a).contains("Online checks use live data only"));
    assert!(a.online.asking.is_empty());
}

#[test]
fn nothing_to_check_before_toolchains_is_read() {
    let t = Temp::new();
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "online test",
    );
    a.online.use_file(t.file());
    key(&mut a, KeyCode::Char('u'));
    assert!(draw(&mut a).contains("Nothing to check online yet: open Toolchains first"));
}

#[test]
fn a_file_den_cannot_read_is_reported_and_left_alone() {
    for (content, why) in [
        (&b"{not json"[..], "it is not Den's online results"),
        (
            br#"{"version":2,"packages":{}}"#,
            "it was saved by a newer Den",
        ),
        (
            br#"{"version":1,"packages":{},"extra":1}"#,
            "it is not Den's online results",
        ),
    ] {
        let t = Temp::new();
        fs::create_dir_all(t.file().parent().unwrap()).unwrap();
        fs::write(t.file(), content).unwrap();
        let mut a = session(Some(&t.file()));
        let screen = draw(&mut a);
        assert!(
            screen.contains(&format!("Online results not used: {why}")),
            "{screen}"
        );
        assert_eq!(fs::read(t.file()).unwrap(), content);
    }
}

#[cfg(unix)]
#[test]
fn a_link_is_never_followed_or_replaced() {
    let t = Temp::new();
    let target = t.0.join("elsewhere.json");
    fs::write(&target, br#"{"version":1,"packages":{}}"#).unwrap();
    fs::create_dir_all(t.file().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, t.file()).unwrap();
    let mut a = session(Some(&t.file()));
    assert!(draw(&mut a).contains("Online results not used: it is a link"));
    a.finish_online(Ok(answers()), SystemTime::now());
    assert!(draw(&mut a).contains("not saved: it is a link"));
    assert_eq!(
        fs::read(&target).unwrap(),
        br#"{"version":1,"packages":{}}"#
    );
}

#[test]
fn the_file_holds_only_names_and_answers() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()));
    a.finish_online(Ok(answers()), SystemTime::now());
    let file = online::load(&t.file()).unwrap().unwrap();
    assert_eq!(file.version, online::FILE_VERSION);
    assert_eq!(
        file.packages.keys().collect::<Vec<_>>(),
        [
            "PyPI:httpie",
            "PyPI:ruff",
            "crates.io:bacon",
            "npm:@acme/cli",
            "npm:npm",
            "npm:prettier",
        ]
    );
}

/// A session that has read the Homebrew fixture, keeping online answers at `path`.
fn homebrew_session(path: &Path) -> App {
    let inventory = den::areas::homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap();
    let mut a = App::new(inventory, "online test");
    a.online.use_file(path.to_owned());
    a
}
fn search(a: &mut App, text: &str) {
    key(a, KeyCode::Char('/'));
    for c in text.chars() {
        key(a, KeyCode::Char(c));
    }
    key(a, KeyCode::Enter);
}

#[test]
fn homebrew_packages_from_the_main_taps_are_checked() {
    let t = Temp::new();
    let mut a = homebrew_session(&t.file());
    assert!(draw(&mut a).contains("u:check online"));
    key(&mut a, KeyCode::Char('u'));
    assert!(draw(&mut a).contains("sends 4 package names to formulae.brew.sh; versions stay here"));
    key(&mut a, KeyCode::Enter);
    let asked: Vec<String> = a
        .online
        .request
        .take()
        .unwrap()
        .iter()
        .map(Package::key)
        .collect();
    // Other taps (example/tools) and packages with no tap are not on formulae.brew.sh.
    assert_eq!(
        asked,
        [
            "Homebrew:pcre2",
            "Homebrew:python@3.12",
            "Homebrew:ripgrep",
            "Homebrew cask:requested-app",
        ]
    );
    a.finish_online(
        Ok(vec![
            (
                package(Registry::Homebrew, "pcre2"),
                Answer::Latest("10.45".into()),
            ),
            (
                package(Registry::Homebrew, "python@3.12"),
                Answer::Latest("3.12.8".into()),
            ),
            (
                package(Registry::Homebrew, "ripgrep"),
                Answer::Latest("100.0".into()),
            ),
            (
                package(Registry::HomebrewCask, "requested-app"),
                Answer::Latest("2.1".into()),
            ),
        ]),
        SystemTime::now(),
    );
    assert!(draw(&mut a).contains("Checked 4 packages online: 3 newer"));
    key(&mut a, KeyCode::Down);
    let screen = draw(&mut a);
    assert!(
        screen.contains("3 newer online, checked just now"),
        "{screen}"
    );
    // The online answer is newer than Homebrew's own data (99.0), so it wins.
    assert!(screen.contains("↑ 100.0 · Requested"), "{screen}");
    assert!(screen.contains("↑ 2.1 · "), "{screen}");
    search(&mut a, "ripgrep");
    let screen = draw(&mut a);
    assert!(
        screen.contains("↑ 100.0 available on Homebrew · checked online just now"),
        "{screen}"
    );
    assert!(!screen.contains("as of last brew update"), "{screen}");
}

#[test]
fn homebrew_data_is_used_until_checked_online() {
    let t = Temp::new();
    let mut a = homebrew_session(&t.file());
    search(&mut a, "ripgrep");
    let screen = draw(&mut a);
    assert!(screen.contains("↑ 99.0 · Requested"), "{screen}");
    assert!(
        screen.contains("↑ 99.0 available · as of last brew update"),
        "{screen}"
    );
    // A failed check leaves Homebrew's own data in place.
    a.finish_online(
        Ok(vec![(
            package(Registry::Homebrew, "ripgrep"),
            Answer::Failed("timed out".into()),
        )]),
        SystemTime::now(),
    );
    let screen = draw(&mut a);
    assert!(
        screen.contains("↑ 99.0 available · as of last brew update"),
        "{screen}"
    );
    assert!(
        screen.contains("Online check failed: timed out"),
        "{screen}"
    );
}

const D_LOCAL: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const D_NEW: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

/// A session that has read the Docker fixture plus images pulled from Docker Hub and
/// ghcr.io and one built here.
fn docker_session(t: &Temp) -> App {
    let root = t.0.join("docker");
    fs::create_dir_all(root.join("home/.docker")).unwrap();
    for f in [
        "containers.jsonl",
        "images.jsonl",
        "volumes.jsonl",
        "disk.jsonl",
    ] {
        fs::copy(Path::new("fixtures/docker").join(f), root.join(f)).unwrap();
    }
    let mut images = fs::read_to_string(root.join("images.jsonl")).unwrap();
    for (repo, tag) in [
        ("grafana/loki", "3.5"),
        ("ghcr.io/astral-sh/uv", "latest"),
        ("my-app", "local"),
    ] {
        images.push_str(&format!(
            r#"{{"ID":"{D_LOCAL}","Digest":"{D_LOCAL}","Repository":"{repo}","Tag":"{tag}","Size":"1MB","CreatedSince":"1 day ago"}}"#
        ));
        images.push('\n');
    }
    fs::write(root.join("images.jsonl"), images).unwrap();
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "online test",
    );
    a.online.use_file(t.file());
    let context = den::areas::docker::Context::fixture(&root).unwrap();
    a.docker.configure(context.clone());
    a.docker.finish(den::areas::docker::discover(
        &context,
        &AtomicBool::new(false),
    ));
    a.docker.requested = false;
    a.navigate(Destination::Images);
    a
}

#[test]
fn docker_images_with_a_namespace_are_checked_for_a_newer_build() {
    let t = Temp::new();
    let mut a = docker_session(&t);
    assert!(draw(&mut a).contains("u:check online"));
    key(&mut a, KeyCode::Char('u'));
    assert!(
        draw(&mut a)
            .contains("sends 2 package names to registry-1.docker.io, ghcr.io; versions stay here")
    );
    key(&mut a, KeyCode::Enter);
    let asked: Vec<String> = a
        .online
        .request
        .take()
        .unwrap()
        .iter()
        .map(Package::key)
        .collect();
    // Single names (nginx, postgres, my-app) may be built here: they are not sent.
    assert_eq!(
        asked,
        ["Docker Hub:grafana/loki:3.5", "ghcr.io:astral-sh/uv:latest"]
    );
    a.finish_online(
        Ok(vec![
            (
                Package::new(Registry::DockerHub, "grafana/loki:3.5").unwrap(),
                Answer::Latest(D_NEW.into()),
            ),
            (
                Package::new(Registry::Ghcr, "astral-sh/uv:latest").unwrap(),
                Answer::Latest(D_LOCAL.into()),
            ),
        ]),
        SystemTime::now(),
    );
    assert!(draw(&mut a).contains("Checked 2 packages online: 1 newer"));
    a.docker.select_key("image:grafana/loki:3.5");
    let screen = draw(&mut a);
    assert!(screen.contains("↑ newer build · Unused"), "{screen}");
    assert!(
        screen.contains("↑ 3.5 now points to a newer build on Docker Hub (222222222222)"),
        "{screen}"
    );
    // Down moves to ghcr.io/astral-sh/uv, and the notice gives way to the status line.
    key(&mut a, KeyCode::Down);
    let screen = draw(&mut a);
    assert!(
        screen.contains("1 newer online, checked just now"),
        "{screen}"
    );
    assert!(screen.contains("Same build as on ghcr.io"), "{screen}");
}
