// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Online checks with `u`: asked before anything is sent, only package names, newer
//! versions marked where Homebrew's are, kept privately for the next run, and nothing in
//! demo or fixture modes. The registries are never contacted here: answers are given to
//! the app as a finished check would give them.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    actions::{Dialog, Permit, State},
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
    assert!(draw(&mut a).contains("U:check online"));
    key(&mut a, KeyCode::Char('U'));
    let screen = draw(&mut a);
    assert!(
        screen.contains(
            "Check online? Enter:check Esc:cancel · 8 names to index.crates.io, \
             registry.npmjs.org, pypi.org, endoflife.date · Toolchains (Homebrew Agents Docker)"
        ),
        "{screen}"
    );
    // Esc: nothing is asked for and nothing is written.
    key(&mut a, KeyCode::Esc);
    assert!(draw(&mut a).contains("Nothing was checked online"));
    assert!(a.online.request.is_none());
    assert!(!t.file().exists());

    key(&mut a, KeyCode::Char('U'));
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
            // Homebrew's node 22.1.0 and the stable toolchain's Rust 1.85.0.
            "endoflife.date:nodejs",
            "endoflife.date:rust",
        ]
    );
    assert!(draw(&mut a).contains("checking 8 packages online… Esc:stop"));
    // Esc while it runs asks the check to stop.
    key(&mut a, KeyCode::Esc);
    assert!(a.online.stop);
}

#[test]
fn newer_versions_are_marked_and_kept_for_the_next_run() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()));
    key(&mut a, KeyCode::Char('U'));
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
    assert!(!draw(&mut a).contains("U:check online"));
    key(&mut a, KeyCode::Char('U'));
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
    key(&mut a, KeyCode::Char('U'));
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
    assert!(draw(&mut a).contains("U:check online"));
    key(&mut a, KeyCode::Char('U'));
    assert!(draw(&mut a).contains("· 4 names to formulae.brew.sh"));
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
    assert!(draw(&mut a).contains("U:check online"));
    key(&mut a, KeyCode::Char('U'));
    assert!(draw(&mut a).contains(
        "2 names to registry-1.docker.io, ghcr.io · Docker (Homebrew Agents Toolchains)"
    ));
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

#[test]
fn runtimes_and_rust_channels_are_checked_against_their_release_lines() {
    let t = Temp::new();
    let mut a = session(Some(&t.file()));
    let lines = |json: &str| match online::releases(json.as_bytes()) {
        Ok(found) => Answer::Releases(found),
        Err(why) => Answer::Failed(why),
    };
    a.finish_online(
        Ok(vec![
            (
                package(Registry::EndOfLife, "nodejs"),
                lines(
                    r#"{"result":{"releases":[
                    {"name":"26","isEol":false,"eolFrom":"2029-04-30","latest":{"name":"26.13.1"}},
                    {"name":"22","isEol":true,"eolFrom":"2026-04-30","latest":{"name":"22.9.0"}}]}}"#,
                ),
            ),
            (
                package(Registry::EndOfLife, "rust"),
                lines(
                    r#"{"result":{"releases":[
                    {"name":"1.99","isEol":false,"eolFrom":null,"latest":{"name":"1.99.1"}},
                    {"name":"1.85","isEol":true,"eolFrom":"2026-04-03","latest":{"name":"1.85.1"}}]}}"#,
                ),
            ),
        ]),
        SystemTime::now(),
    );
    // Rust's stable channel follows the newest release.
    a.navigate(Destination::Rust);
    key(&mut a, KeyCode::Down);
    a.toolchains
        .select_key("toolchain:stable-aarch64-apple-darwin");
    let screen = draw(&mut a);
    assert!(screen.contains("↑ 1.99.1 · Ready · default"), "{screen}");
    assert!(
        screen.contains("Version: 1.85.0 (from its manifest)"),
        "{screen}"
    );
    assert!(
        screen.contains("↑ 1.99.1 is the newest release · from endoflife.date"),
        "{screen}"
    );
    // A pinned toolchain is left alone.
    assert!(!screen.contains("↑ 1.99.1 · Installed"), "{screen}");
    // Homebrew's node: Homebrew says whether it has a newer version; the release line
    // says that its support has ended.
    a.navigate(Destination::OnPath);
    a.toolchains.select_key("command:node");
    let screen = draw(&mut a);
    assert!(screen.contains("End of life · Ready · 22.1.0"), "{screen}");
    assert!(screen.contains("1 end of life"), "{screen}");
    assert!(
        screen.contains("22 reached end of life on 2026-04-30 · from"),
        "{screen}"
    );
    // Homebrew decides node's upgrade (it may move to a newer line), so the newest 22
    // patch is not counted as newer.
    assert!(!screen.contains("newer online"), "{screen}");
}

/// A real channel manifest is about 1 MB with tens of thousands of lines: more than shown
/// text may have. Its version is still read, and the toolchain is no Problem.
#[test]
fn a_full_size_channel_manifest_is_read() {
    let t = Temp::new();
    let rustlib =
        t.0.join("home/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib");
    fs::create_dir_all(&rustlib).unwrap();
    fs::write(t.0.join("PATH"), "/usr/bin\n").unwrap();
    let mut manifest = String::from("manifest-version = \"2\"\n");
    for i in 0..3000 {
        manifest.push_str(&format!(
            "\n[pkg.cargo.target.t{i}]\navailable = true\nurl = \"x\"\nhash = \"y\"\ncomponents = []\nextensions = []\n"
        ));
    }
    manifest.push_str("\n[pkg.rust]\nversion = \"1.97.1 (8bab26f4f 2026-07-14)\"\n");
    assert!(manifest.lines().count() > 20_000);
    fs::write(rustlib.join("multirust-channel-manifest.toml"), manifest).unwrap();
    let context = toolchains::Context::fixture(&t.0).unwrap();
    let read = toolchains::discover(&context, &AtomicBool::new(false)).unwrap();
    let stable = read
        .rust
        .iter()
        .find(|r| r.key == "toolchain:stable-aarch64-apple-darwin")
        .unwrap();
    assert_eq!(
        stable.online.as_ref().map(|l| l.installed.as_str()),
        Some("1.97.1")
    );
    assert!(read.rust.iter().all(|r| !r.status.is_problem()));
}

#[test]
fn dew_offers_to_pull_only_a_tag_with_a_newer_build() {
    let t = Temp::new();
    let mut a = docker_session(&t);
    a.actions = State::new(Permit::granted(true));
    let menu = |a: &mut App, key_name: &str| -> Vec<String> {
        a.docker.select_key(key_name);
        key(a, KeyCode::Enter);
        let lines = match &a.actions.dialog {
            Some(Dialog::Menu { items, .. }) => {
                items.iter().map(|i| i.action.command_line()).collect()
            }
            _ => vec![],
        };
        key(a, KeyCode::Esc);
        lines
    };
    // Before any online check, nothing says a newer build exists.
    assert!(
        !menu(&mut a, "image:grafana/loki:3.5")
            .iter()
            .any(|l| l.contains("pull"))
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
    let loki = menu(&mut a, "image:grafana/loki:3.5");
    assert_eq!(
        loki.first().map(String::as_str),
        Some("docker image pull grafana/loki:3.5"),
        "{loki:?}"
    );
    // The same build as the registry's, and a single name never checked: no pull.
    for other in ["image:ghcr.io/astral-sh/uv:latest", "image:my-app:local"] {
        assert!(
            !menu(&mut a, other).iter().any(|l| l.contains("pull")),
            "{other}"
        );
    }
    // den offers no actions at all.
    a.actions = State::new(Permit::granted(false));
    assert!(menu(&mut a, "image:grafana/loki:3.5").is_empty());
}
