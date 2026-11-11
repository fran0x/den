// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! When a tool or its configuration is missing, each area says so once, in the list:
//! no repeated message in the status line or panel, and no zero counters.
use den::{
    app::App, aws, claude, codex, docker, homebrew::CollectionError, inventory::Inventory,
    presentation::Destination, ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

fn empty_app() -> App {
    App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated fixture",
    )
}
fn draw(a: &mut App) -> Vec<String> {
    let mut t = Terminal::new(TestBackend::new(140, 30)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    (0..30)
        .map(|y| {
            (0..140)
                .map(|x| t.backend().buffer()[(x, y)].symbol())
                .collect()
        })
        .collect()
}
/// The message is on screen once, and the status line (row 1) is empty.
fn said_once(screen: &[String], message: &str) {
    let text = screen.join("\n");
    assert_eq!(text.matches(message).count(), 1, "{text}");
    assert!(screen[1].trim().is_empty(), "status line: {}", screen[1]);
}

/// An empty home and project, removed afterwards.
struct Empty(std::path::PathBuf);
impl Empty {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("den-missing-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("project")).unwrap();
        Self(root)
    }
}
impl Drop for Empty {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn homebrew_unavailable_is_said_once_without_counts() {
    let mut a = App::live_homebrew();
    a.begin_collection();
    a.finish_collection(Err(CollectionError::Unavailable));
    let screen = draw(&mut a);
    said_once(&screen, "Homebrew unavailable");
    let text = screen.join("\n");
    assert!(
        !text.contains("Tools 0") && !text.contains("r:retry"),
        "{text}"
    );
}

#[test]
fn docker_unavailable_is_said_once_without_column_headers() {
    let mut a = empty_app();
    a.docker
        .configure(docker::Context::fixture(Path::new("fixtures/docker")).unwrap());
    a.docker.finish(Err(std::io::Error::other(
        "Docker CLI not found: put docker on PATH, then press r",
    )));
    a.navigate(Destination::Containers);
    let screen = draw(&mut a);
    said_once(&screen, "Docker CLI not found");
    assert!(!screen.join("\n").contains("Container "));
}

#[test]
fn aws_without_config_files_says_not_set_up() {
    let t = Empty::new("aws");
    let context = aws::Context::fixture(&t.0).unwrap();
    let inventory = aws::discover(&context, &AtomicBool::new(false)).unwrap();
    assert!(!inventory.found);
    let mut a = empty_app();
    a.aws.configure(context);
    a.aws.finish(Ok(inventory));
    a.navigate(Destination::Aws);
    let screen = draw(&mut a);
    said_once(&screen, "AWS not set up here");
    let text = screen.join("\n");
    // The panel keeps the coverage: which files were looked for.
    assert!(
        text.contains("Coverage") && !text.contains("0 profiles"),
        "{text}"
    );
}

#[test]
fn agents_without_codex_or_claude_homes_say_not_set_up() {
    let t = Empty::new("agents");
    let mut a = empty_app();
    let codex = codex::Context::fixture(&t.0, None).unwrap();
    a.codex.configure(codex.clone());
    a.codex.finish(
        codex::discover(&codex, &AtomicBool::new(false)),
        &mut a.skills,
    );
    let claude = claude::Context::fixture(&t.0).unwrap();
    let inventory = claude::discover(&claude, &AtomicBool::new(false)).unwrap();
    assert_eq!(inventory.missing_home.as_deref(), Some("~/.claude"));
    a.claude.configure(claude);
    a.claude.finish(Ok(inventory));
    a.codex.requested = false;
    a.claude.requested = false;
    a.navigate(Destination::Skills);
    let screen = draw(&mut a);
    said_once(&screen, "Codex and Claude Code are not set up here");
    let text = screen.join("\n");
    // The list wraps the message; both home folders are named.
    assert!(
        text.contains("no ~/.codex or") && text.contains("~/.claude."),
        "{text}"
    );
    assert!(!text.contains("0 skills"), "{text}");

    // With only Claude Code set up, the status line keeps its counts and notes Codex.
    std::fs::create_dir_all(t.0.join("home/.claude")).unwrap();
    let claude = claude::Context::fixture(&t.0).unwrap();
    let inventory = claude::discover(&claude, &AtomicBool::new(false)).unwrap();
    a.claude.finish(Ok(inventory));
    a.claude.requested = false;
    let screen = draw(&mut a);
    assert!(
        screen[1].contains("0 skills") && screen[1].contains("Codex not set up (no ~/.codex)"),
        "{}",
        screen[1]
    );
}
