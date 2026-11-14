// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! What changed since Den started: marks on new and changed rows, `d` for the changed
//! rows only, removed rows listed after them, and what does not count as a change.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::App,
    areas::{agents::claude, docker, toolchains},
    inventory::Inventory,
    status::Status,
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

fn empty() -> App {
    App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "changes test",
    )
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(a: &mut App) -> String {
    let (w, h) = (140, 34);
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

fn toolchains() -> (App, toolchains::Inventory) {
    let mut a = empty();
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    let read = || toolchains::discover(&context, &AtomicBool::new(false)).unwrap();
    a.toolchains.configure(context.clone());
    a.toolchains.finish(Ok(read()));
    a.toolchains.requested = false;
    a.navigate(Destination::Rust);
    (a, read())
}

#[test]
fn nothing_is_marked_until_something_changes() {
    let (mut a, same) = toolchains();
    a.toolchains.finish(Ok(same));
    let screen = draw(&mut a);
    assert!(!screen.contains("since Den started"), "{screen}");
    assert!(!screen.contains("d:changes"), "{screen}");
    // d still works: it says there is nothing to show.
    key(&mut a, KeyCode::Char('d'));
    let screen = draw(&mut a);
    assert!(
        screen.contains("Changes 0 · since Den started just now"),
        "{screen}"
    );
    assert!(
        screen.contains("No changes since Den started just now."),
        "{screen}"
    );
    key(&mut a, KeyCode::Char('d'));
    assert!(!draw(&mut a).contains("Changes 0"));
}

#[test]
fn new_changed_and_removed_rows_show_after_a_refresh() {
    let (mut a, mut now) = toolchains();
    let gone = now.rust.pop().unwrap();
    now.rust[0].status = Status::ready("changed on purpose");
    let mut new = now.rust[0].clone();
    new.name = "newcrate".into();
    new.key = "crate:newcrate".into();
    now.rust.push(new);
    let changed = now.rust[0].name.clone();
    a.toolchains.finish(Ok(now));

    let screen = draw(&mut a);
    assert!(screen.contains("3 changes since Den started"), "{screen}");
    assert!(screen.contains("d:changes"), "{screen}");
    assert!(screen.contains(&format!("~ {changed}")), "{screen}");
    assert!(screen.contains("+ newcrate"), "{screen}");
    // Removed rows are only listed in the changes view.
    assert!(!screen.contains(&format!("− {}", gone.name)), "{screen}");

    key(&mut a, KeyCode::Char('d'));
    let screen = draw(&mut a);
    assert!(screen.contains("Changes 3 · since Den started"), "{screen}");
    assert!(screen.contains(&format!("− {}", gone.name)), "{screen}");
    assert_eq!(
        a.toolchains.rows().len(),
        2,
        "only changed rows are selectable"
    );
    // The panel says what it was before.
    a.toolchains.select_key(&a.toolchains.rows()[0].key.clone());
    let screen = draw(&mut a);
    assert!(screen.contains("What changed"), "{screen}");
    assert!(
        screen.contains("Before: Toolchain · Ready · default"),
        "{screen}"
    );
    // Moving down never lands on a removed row.
    key(&mut a, KeyCode::End);
    assert_eq!(a.toolchains.selected(), 1);
    assert_eq!(a.toolchains.selected_row().unwrap().name, "newcrate");
}

#[test]
fn search_applies_to_the_changes_view() {
    let (mut a, mut now) = toolchains();
    let gone = now.rust.pop().unwrap();
    a.toolchains.finish(Ok(now));
    key(&mut a, KeyCode::Char('d'));
    key(&mut a, KeyCode::Char('/'));
    for ch in "zzz".chars() {
        key(&mut a, KeyCode::Char(ch));
    }
    key(&mut a, KeyCode::Enter);
    let screen = draw(&mut a);
    assert!(!screen.contains(&format!("− {}", gone.name)), "{screen}");
    assert!(screen.contains("No matching changes."), "{screen}");
}

#[test]
fn the_baseline_is_the_first_successful_read() {
    let (mut a, mut now) = toolchains();
    // A failed refresh keeps the rows and changes nothing.
    a.toolchains
        .finish(Err(std::io::Error::other("read failed")));
    assert!(a.toolchains.changes.changes().is_empty());
    now.rust.pop();
    a.toolchains.finish(Ok(now));
    assert_eq!(a.toolchains.changes.changes().len(), 1);
}

fn docker_app() -> (App, docker::Inventory) {
    let mut a = empty();
    let context = docker::Context::fixture(Path::new("fixtures/docker")).unwrap();
    let read = || docker::discover(&context, &AtomicBool::new(false)).unwrap();
    a.docker.configure(context.clone());
    a.docker.finish(Ok(read()));
    a.docker.requested = false;
    a.navigate(Destination::Containers);
    (a, read())
}

#[test]
fn a_stopped_container_is_no_change_but_a_removed_one_is() {
    let (mut a, mut now) = docker_app();
    for c in &mut now.containers {
        c.state = "Exited".into();
    }
    let gone = now.containers[0].clone();
    a.docker.finish(Ok(now));
    assert!(a.docker.changes.changes().is_empty());
    let (_, mut now) = docker_app();
    now.containers.remove(0);
    a.docker.finish(Ok(now));
    key(&mut a, KeyCode::Char('d'));
    let screen = draw(&mut a);
    assert!(screen.contains(&format!("− {}", gone.name)), "{screen}");
    assert!(
        screen.contains("No changes") || screen.contains("Removed since"),
        "{screen}"
    );
}

fn brew(json: &str) -> Result<Inventory, den::areas::homebrew::CollectionError> {
    den::areas::homebrew::parse(json.as_bytes())
}
fn formula(name: &str, version: &str, request: bool) -> String {
    format!(
        r#"{{"name":"{name}","full_name":"{name}","tap":"homebrew/core","desc":"","versions":{{"stable":"{version}"}},"installed":[{{"version":"{version}","installed_on_request":{request},"installed_as_dependency":{dep},"runtime_dependencies":[]}}],"linked_keg":"{version}","dependencies":[]}}"#,
        dep = !request
    )
}
fn brew_json(formulae: &[String]) -> String {
    format!(r#"{{"formulae":[{}],"casks":[]}}"#, formulae.join(","))
}

#[test]
fn homebrew_shows_upgrades_and_removed_dependencies() {
    let mut a = App::live_homebrew();
    a.finish_collection(brew(&brew_json(&[
        formula("git", "2.47.0", true),
        formula("pcre2", "10.44", false),
    ])));
    assert!(!draw(&mut a).contains("since Den started"));
    a.finish_collection(brew(&brew_json(&[formula("git", "2.48.1", true)])));
    let screen = draw(&mut a);
    assert!(screen.contains("2 changes since Den started"), "{screen}");
    assert!(screen.contains("~ git"), "{screen}");
    assert!(screen.contains("d:changes"), "{screen}");
    // The changes view shows dependencies too, even from Tools.
    key(&mut a, KeyCode::Char('d'));
    let screen = draw(&mut a);
    assert!(screen.contains("Changes 2 · since Den started"), "{screen}");
    assert!(screen.contains("− pcre2"), "{screen}");
    assert!(screen.contains("Before: 2.47.0"), "{screen}");
    // a goes back to Tools / All.
    key(&mut a, KeyCode::Char('a'));
    assert!(!draw(&mut a).contains("Changes 2"));
}

#[test]
fn agents_compare_once_both_tools_have_been_read() {
    let root = Path::new("fixtures/claude");
    let context = claude::Context::fixture(root).unwrap();
    let read = || claude::discover(&context, &AtomicBool::new(false));
    let mut a = empty();
    a.claude.configure(context.clone());
    a.claude.requested = false;
    a.finish_claude(read());
    a.navigate(Destination::Mcp);
    // The same reading again: nothing changed.
    a.finish_claude(read());
    assert!(a.agents.changes.changes().is_empty());
    // Claude's MCP servers disappear.
    let mut gone = read().unwrap();
    gone.config.mcp.clear();
    let names: Vec<String> = read()
        .unwrap()
        .config
        .mcp
        .iter()
        .map(|m| m.name.clone())
        .collect();
    assert!(!names.is_empty(), "the fixture has MCP servers");
    a.finish_claude(Ok(gone));
    key(&mut a, KeyCode::Char('d'));
    let screen = draw(&mut a);
    for name in &names {
        assert!(screen.contains(&format!("− {name}")), "{screen}");
    }
}
