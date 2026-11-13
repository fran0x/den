// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The Problems overview: what it lists, in what order, and where Enter goes.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    areas::{cloud::aws, cloud::kube, homebrew, toolchains},
    problems::{self, Kind, Target},
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

/// Homebrew, AWS, Kubernetes and Toolchains from their fixtures, as if each was opened.
fn app() -> App {
    let inventory = homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap();
    let mut a = App::new(inventory, "Homebrew snapshot");
    let cancel = AtomicBool::new(false);
    let context = aws::Context::fixture(Path::new("fixtures/aws")).unwrap();
    a.aws.configure(context.clone());
    a.aws.finish(aws::discover(&context, &cancel));
    a.aws.requested = false;
    let context = kube::Context::fixture(Path::new("fixtures/kube")).unwrap();
    a.kube.configure(context.clone());
    a.kube.finish(kube::discover(&context, &cancel));
    a.kube.requested = false;
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    a.toolchains.configure(context.clone());
    a.toolchains.finish(toolchains::discover(&context, &cancel));
    a.toolchains.requested = false;
    a
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

#[test]
fn problems_from_every_area_come_before_updates() {
    let a = app();
    let entries = problems::collect(&a);
    let first_update = entries.iter().position(|e| e.kind == Kind::Update).unwrap();
    assert!(
        entries[..first_update]
            .iter()
            .all(|e| e.kind == Kind::Problem)
    );
    assert!(
        entries[first_update..]
            .iter()
            .all(|e| e.kind == Kind::Update)
    );
    let has = |place: &str, name: &str| entries.iter().any(|e| e.place == place && e.name == name);
    assert!(has("Cloud › AWS", "broken"));
    assert!(has("Cloud › AWS", "loop-a"));
    assert!(has("Toolchains › PATH", "java"));
    assert!(has("Toolchains › PATH", "."));
    assert!(entries.iter().any(|e| e.place == "Cloud › Kubernetes"));
    // Homebrew's outdated packages are updates, not problems.
    let ripgrep = entries.iter().find(|e| e.name == "ripgrep").unwrap();
    assert_eq!(ripgrep.kind, Kind::Update);
    assert!(ripgrep.detail.starts_with("↑ 99.0 available"));
    assert_eq!(
        problems::problems(&a),
        first_update,
        "the badge counts problems only"
    );
}

#[test]
fn bang_opens_the_overview_and_enter_goes_to_the_row() {
    let mut a = app();
    let count = problems::problems(&a);
    // The badge shows the count everywhere; ! opens the list.
    assert!(draw(&mut a, 140, 40).contains(&format!(" ! {count} ")));
    key(&mut a, KeyCode::Char('!'));
    assert!(a.problems.is_some());
    let screen = draw(&mut a, 140, 40);
    assert!(screen.contains("Problems"), "{screen}");
    assert!(screen.contains("Docker: open it to check") || a.docker.context.is_none());
    assert!(screen.contains("Problem · "), "{screen}");

    // Enter on an AWS problem opens Cloud › AWS with that profile selected.
    let entries = problems::collect(&a);
    let at = entries
        .iter()
        .position(|e| matches!(&e.target, Target::Aws(key) if key == "profile:broken"))
        .unwrap();
    for _ in 0..at {
        key(&mut a, KeyCode::Down);
    }
    key(&mut a, KeyCode::Enter);
    assert!(a.problems.is_none());
    assert_eq!(a.destination(), Destination::Aws);
    assert_eq!(a.aws.selected_row().unwrap().key, "profile:broken");

    // Enter on an update opens the package in Homebrew.
    key(&mut a, KeyCode::Char('!'));
    key(&mut a, KeyCode::End);
    let last = problems::collect(&a).pop().unwrap();
    assert_eq!(last.kind, Kind::Update);
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.module, Module::Homebrew);
    assert_eq!(
        a.resource().unwrap().display_name(),
        last.name,
        "the package is selected"
    );
}

#[test]
fn bang_is_text_while_searching_and_esc_closes() {
    let mut a = app();
    key(&mut a, KeyCode::Char('/'));
    key(&mut a, KeyCode::Char('!'));
    assert!(a.problems.is_none());
    assert_eq!(a.query, "!");
    key(&mut a, KeyCode::Esc);
    key(&mut a, KeyCode::Char('!'));
    assert!(a.problems.is_some());
    key(&mut a, KeyCode::Esc);
    assert!(a.problems.is_none());
}

#[test]
fn an_empty_overview_says_so_and_fits_small_screens() {
    let inventory = homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap();
    let mut a = App::new(inventory, "Homebrew snapshot");
    // Only an absent build-only dependency here: normal, so no problem and no badge;
    // its updates are listed but not counted.
    assert_eq!(problems::problems(&a), 0);
    assert!(!draw(&mut a, 120, 30).contains(" ! "));
    key(&mut a, KeyCode::Char('!'));
    for (w, h) in [(30, 10), (45, 20), (80, 24), (160, 45)] {
        draw(&mut a, w, h);
    }
    let mut a = App::new(
        den::inventory::Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "empty",
    );
    assert!(
        !draw(&mut a, 100, 24).contains(" ! "),
        "no badge without problems"
    );
    key(&mut a, KeyCode::Char('!'));
    assert!(draw(&mut a, 100, 24).contains("Nothing needs attention"));
}

#[test]
fn only_missing_runtime_dependencies_are_homebrew_problems() {
    // One absent dependency of every kind: only the recorded and the required runtime
    // ones count; build, test, optional, version and identity gaps are normal.
    let inventory = homebrew::load_snapshot(Path::new("fixtures/brew-diagnostics.json")).unwrap();
    let mut a = App::new(inventory, "Homebrew snapshot");
    let entries = problems::collect(&a);
    let missing: Vec<_> = entries
        .iter()
        .filter(|e| e.place == "Homebrew › Diagnostics")
        .collect();
    assert_eq!(missing.len(), 1);
    assert!(
        missing[0].detail.starts_with("2 dependencies"),
        "{}",
        missing[0].detail
    );
    assert!(draw(&mut a, 120, 30).contains(" ! 1 "));
    // Enter opens Diagnostics.
    key(&mut a, KeyCode::Char('!'));
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.destination(), Destination::Unresolved);
}
