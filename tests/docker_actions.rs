// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Docker clean-up in dew: what each row offers, that nothing in use is offered, that Enter
//! waits for a choice, and that Docker (not Homebrew) is refreshed afterwards.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    actions::{Dialog, Permit, Request, State},
    app::App,
    areas::{
        docker::{self, Section},
        homebrew::CommandOutput,
    },
    inventory::Inventory,
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const FIXTURE: &str = "fixtures/docker";

fn app_from(root: &Path, dew: bool) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated Docker fixture",
    );
    let context = docker::Context::fixture(root).unwrap();
    a.docker.configure(context.clone());
    a.docker
        .finish(docker::discover(&context, &AtomicBool::new(false)));
    a.docker.requested = false;
    a.actions = State::new(Permit::granted(dew));
    a.navigate(Destination::Containers);
    a
}
fn app(dew: bool) -> App {
    app_from(Path::new(FIXTURE), dew)
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
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
/// Opens the section with the named row selected, then the actions.
fn actions_for(a: &mut App, section: Section, name: &str) -> Vec<String> {
    a.navigate(match section {
        Section::Containers => Destination::Containers,
        Section::Images => Destination::Images,
        Section::Volumes => Destination::Volumes,
    });
    let key_of = a
        .docker
        .rows()
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("{name} row"))
        .key
        .clone();
    a.docker.select_key(&key_of);
    key(a, KeyCode::Char('x'));
    let lines = match &a.actions.dialog {
        Some(Dialog::Menu { items, .. }) => items.iter().map(|i| i.action.command_line()).collect(),
        other => panic!("no menu: {other:?}"),
    };
    key(a, KeyCode::Esc);
    lines
}

const NGINX: &str = "aaaaaaaaaaaa1111111111111111111111111111111111111111111111111111";
const DB: &str = "bbbbbbbbbbbb2222222222222222222222222222222222222222222222222222";

#[test]
fn each_row_offers_only_what_docker_allows_without_force() {
    let mut a = app(true);
    // A running container can be stopped; a stopped one removed; a paused one neither.
    assert_eq!(
        actions_for(&mut a, Section::Containers, "web-nginx-1"),
        [format!("docker container stop {NGINX}")]
    );
    assert_eq!(
        actions_for(&mut a, Section::Containers, "web-db-1"),
        [format!("docker container rm {DB}")]
    );
    assert!(actions_for(&mut a, Section::Containers, "scratch").is_empty());
    // Images: unused ones by tag, untagged ones by ID; anything a container uses: nothing.
    assert_eq!(
        actions_for(&mut a, Section::Images, "redis:7"),
        ["docker image rm redis:7"]
    );
    assert_eq!(
        actions_for(&mut a, Section::Images, "<none> 888888888888"),
        ["docker image rm sha256:888888888888888888888888888888888888888888888888888888888888888"]
    );
    assert!(actions_for(&mut a, Section::Images, "postgres:16").is_empty());
    // Volumes: only unused ones.
    assert_eq!(
        actions_for(&mut a, Section::Volumes, "0123456789ab… (anonymous)"),
        ["docker volume rm 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"]
    );
    assert!(actions_for(&mut a, Section::Volumes, "web_data").is_empty());
}

#[test]
fn x_then_enter_never_removes_and_docker_refreshes_after() {
    let mut a = app(true);
    a.navigate(Destination::Containers);
    a.docker.select_key(&format!("container:{DB}"));
    assert!(draw(&mut a, 140, 32).contains("x:actions"));
    key(&mut a, KeyCode::Char('x'));
    let screen = draw(&mut a, 140, 32);
    assert!(
        screen.contains("to see exactly what it removes"),
        "{screen}"
    );
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.actions.request, None, "Enter waits for a choice");
    // Choosing shows what goes; only then does Enter run it.
    key(&mut a, KeyCode::Home);
    let screen = draw(&mut a, 140, 32);
    assert!(
        screen.contains("Removes the container web-db-1"),
        "{screen}"
    );
    key(&mut a, KeyCode::Enter);
    let Some(Request::Run(action)) = a.actions.request.take() else {
        panic!("{:?}", a.actions.request);
    };
    assert_eq!(action.command_line(), format!("docker container rm {DB}"));

    let log = std::env::temp_dir().join(format!("den-docker-actions-{}", std::process::id()));
    a.finish_action(
        Ok(CommandOutput {
            code: Some(0),
            stdout: format!("{DB}\n").into_bytes(),
            stderr: vec![],
        }),
        Some(&log.join("actions.log")),
    );
    assert!(a.docker.requested, "Docker is listed again");
    assert!(!a.refresh_requested, "Homebrew is not");
    assert!(draw(&mut a, 140, 32).contains("Done"));
    let _ = fs::remove_dir_all(&log);
}

#[test]
fn den_offers_no_docker_actions() {
    let mut a = app(false);
    a.docker.select_key(&format!("container:{DB}"));
    assert!(!draw(&mut a, 140, 32).contains("x:actions"));
    key(&mut a, KeyCode::Char('x'));
    assert!(a.actions.dialog.is_none());
}

/// A copy of the fixture with a second untagged image nothing uses.
struct Copy(PathBuf);
impl Copy {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("den-docker-dangling-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("home/.docker")).unwrap();
        for f in [
            "containers.jsonl",
            "images.jsonl",
            "volumes.jsonl",
            "disk.jsonl",
        ] {
            fs::copy(Path::new(FIXTURE).join(f), root.join(f)).unwrap();
        }
        let mut images = fs::read_to_string(root.join("images.jsonl")).unwrap();
        images.push_str(
            r#"{"Containers":"N/A","CreatedSince":"6 months ago","ID":"sha256:777777777777aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","Repository":"<none>","Size":"5MB","Tag":"<none>"}"#,
        );
        images.push('\n');
        fs::write(root.join("images.jsonl"), images).unwrap();
        Self(root)
    }
}
impl Drop for Copy {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn every_unused_untagged_image_can_go_at_once() {
    let copy = Copy::new();
    let mut a = app_from(&copy.0, true);
    let lines = actions_for(&mut a, Section::Images, "redis:7");
    assert_eq!(
        lines,
        [
            "docker image rm redis:7".to_owned(),
            "docker image rm sha256:888888888888888888888888888888888888888888888888888888888888888 sha256:777777777777aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        ]
    );
    // The untagged image a container uses is never part of it.
    assert!(!lines[1].contains("dddddddddddd"));
    // With only one, the row's own action is enough.
    let mut a = app(true);
    assert_eq!(actions_for(&mut a, Section::Images, "redis:7").len(), 1);
}
