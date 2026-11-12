// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Newer versions named without contacting anything: Homebrew's data and Codex's own
//! update check, shown in Agents, Toolchains and Homebrew.
use den::{
    app::App,
    areas::{
        agents::{claude, codex},
        homebrew, toolchains,
    },
    inventory::{Inventory, Resource},
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, SystemTime},
};

fn package(kind: &str, name: &str, version: &str, latest: &str, outdated: bool) -> Resource {
    let homebrew = if kind == "cask" {
        serde_json::json!({"type": "cask", "full_token": name, "display_names": [],
            "tap": null, "homepage": null, "recorded_version": version,
            "installed_on_request": true, "latest": latest, "outdated": outdated})
    } else {
        serde_json::json!({"type": "formula", "full_name": name, "tap": null,
            "homepage": null, "linked_keg": version, "installed": [], "dependencies": {},
            "latest": latest, "outdated": outdated})
    };
    serde_json::from_value(serde_json::json!({
        "id": format!("{kind}:{name}"), "name": name, "kind": kind, "version": version,
        "path": null, "description": "", "origin": "", "reason": "", "homebrew": homebrew
    }))
    .unwrap()
}
fn homebrew(resources: Vec<Resource>) -> Inventory {
    Inventory {
        schema_version: 1,
        resources,
    }
}
fn draw(a: &mut App, w: u16, h: u16) -> String {
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
/// A temporary folder removed on drop.
struct Temp(PathBuf);
impl Temp {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("den-updates-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const AGENTS: &str = "fixtures/claude";

fn agents(resources: Vec<Resource>) -> App {
    let mut a = App::new(homebrew(resources), "isolated Claude fixture");
    let context = codex::Context::fixture(Path::new(AGENTS), None).unwrap();
    a.codex.configure(context.clone());
    a.codex.finish(
        codex::discover(&context, &AtomicBool::new(false)),
        &mut a.skills,
    );
    a.codex.requested = false;
    let context = claude::Context::fixture(Path::new(AGENTS)).unwrap();
    a.claude.configure(context.clone());
    a.claude
        .finish(claude::discover(&context, &AtomicBool::new(false)));
    a.claude.requested = false;
    a.navigate(Destination::Skills);
    a
}

#[test]
fn codex_s_own_update_check_is_read_from_version_json() {
    let context = codex::Context::fixture(Path::new(AGENTS), None).unwrap();
    let i = codex::discover(&context, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        i.update_check,
        Some(codex::UpdateCheck {
            latest: "0.200.0".into(),
            checked: Some("2026-11-10".into()),
        })
    );
    // Anything unexpected is ignored rather than shown.
    let t = Temp::new("codex");
    fs::create_dir_all(t.0.join("project")).unwrap();
    fs::create_dir_all(t.0.join("home/.codex")).unwrap();
    for text in [
        "not json",
        r#"{"latest_version": "\u001b[31m1.0"}"#,
        r#"{"latest_version": "latest"}"#,
        r#"{"last_checked_at": "2026-11-10"}"#,
    ] {
        fs::write(t.0.join("home/.codex/version.json"), text).unwrap();
        let context = codex::Context::fixture(&t.0, None).unwrap();
        let i = codex::discover(&context, &AtomicBool::new(false)).unwrap();
        assert_eq!(i.update_check, None, "{text}");
    }
}

#[test]
fn agents_names_newer_versions_of_tools_homebrew_installed() {
    // Homebrew installed both: Codex's own check knows a newer Codex, Homebrew a newer
    // Claude Code.
    let mut a = agents(vec![
        package("cask", "codex", "0.167.0", "0.167.0", false),
        package("cask", "claude-code", "2.1.309", "2.1.400", true),
    ]);
    let screen = draw(&mut a, 160, 30);
    assert!(
        screen.contains("↑ Codex 0.200.0 and Claude Code 2.1.400 available"),
        "{screen}"
    );
    // Up to date: nothing is said.
    let mut a = agents(vec![package("cask", "codex", "0.200.0", "0.200.0", false)]);
    assert!(!draw(&mut a, 160, 30).contains("available"));
    // Not installed with Homebrew: the installed version is unknown, so nothing either.
    let mut a = agents(vec![]);
    assert!(!draw(&mut a, 160, 30).contains("available"));
}

fn toolchains(resources: Vec<Resource>) -> App {
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    let mut a = App::new(homebrew(resources), "isolated toolchains fixture");
    a.toolchains.configure(context.clone());
    a.toolchains
        .finish(toolchains::discover(&context, &AtomicBool::new(false)));
    a.toolchains.requested = false;
    a.navigate(Destination::OnPath);
    a
}

#[test]
fn toolchains_marks_commands_homebrew_can_update() {
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    let i = toolchains::discover(&context, &AtomicBool::new(false)).unwrap();
    let node = i.path.iter().find(|r| r.name == "node").unwrap();
    assert_eq!(
        node.homebrew,
        Some(toolchains::Package {
            name: "node".into(),
            cask: false,
        })
    );
    // Only the copy that runs counts: the system python3 hides Homebrew's.
    let python = i.path.iter().find(|r| r.name == "python3").unwrap();
    assert_eq!(python.homebrew, None);

    let mut a = toolchains(vec![package("formula", "node", "22.1.0", "22.12.0", true)]);
    a.toolchains.select_key("command:node");
    let screen = draw(&mut a, 160, 36);
    assert!(screen.contains("↑ 22.12.0 · Ready"), "{screen}");
    assert!(screen.contains("· 1 update"), "{screen}");
    assert!(
        screen.contains("↑ 22.12.0 available from Homebrew"),
        "{screen}"
    );
    // A cask is not the formula of the same name, and no Homebrew data means no mark.
    let mut a = toolchains(vec![package("cask", "node", "22.1.0", "22.12.0", true)]);
    assert!(!draw(&mut a, 160, 36).contains('↑'));
    let mut a = toolchains(vec![]);
    assert!(!draw(&mut a, 160, 36).contains('↑'));
}

#[test]
fn homebrew_data_age_comes_from_its_package_files_only() {
    let t = Temp::new("cache");
    assert_eq!(homebrew::data_refreshed(&t.0), None);
    let internal = t.0.join("api/internal");
    fs::create_dir_all(&internal).unwrap();
    let at = |path: &Path, ago: u64| {
        fs::write(path, "{}").unwrap();
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(ago))
            .unwrap();
    };
    at(&t.0.join("api/formula.jws.json"), 7200);
    at(&internal.join("packages.arm64.jws.json"), 3600);
    // Other files and links do not count, however recent.
    at(&internal.join("executables.txt"), 0);
    std::os::unix::fs::symlink(
        internal.join("executables.txt"),
        internal.join("link.jws.json"),
    )
    .unwrap();
    let refreshed = homebrew::data_refreshed(&t.0).unwrap();
    let age = SystemTime::now().duration_since(refreshed).unwrap();
    assert!((3590..3700).contains(&age.as_secs()), "{age:?}");

    let mut a = App::new(homebrew(vec![]), "live");
    a.finish_collection(Ok(homebrew(vec![package(
        "formula", "node", "26.13.0", "26.14.0", true,
    )])));
    a.homebrew_data = Some(refreshed);
    let message = a.collection_message();
    assert!(
        message.contains("1 outdated · data from 1 h ago"),
        "{message}"
    );
}
