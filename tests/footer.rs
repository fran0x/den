// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The bottom line lists the keys that work where you are, in three groups: this view's
//! own │ Den's │ moving around. Keys that do nothing here are left out.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    actions::{Permit, State},
    app::App,
    inventory::Inventory,
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

fn footer(a: &mut App, width: u16) -> String {
    let height = 30;
    let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    let line = |y| {
        (0..width)
            .map(|x| t.backend().buffer()[(x, y)].symbol())
            .collect::<String>()
    };
    assert!(
        line(height - 2).starts_with("────"),
        "a rule above the keys"
    );
    line(height - 1).trim_end().to_owned()
}
fn docker(dew: bool) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "footer test",
    );
    let context = den::areas::docker::Context::fixture(Path::new("fixtures/docker")).unwrap();
    a.docker.configure(context.clone());
    a.docker.finish(den::areas::docker::discover(
        &context,
        &AtomicBool::new(false),
    ));
    a.docker.requested = false;
    a.actions = State::new(Permit::granted(dew));
    a.navigate(Destination::Images);
    a
}

#[test]
fn keys_are_grouped_by_scope() {
    let mut a = docker(true);
    assert_eq!(
        footer(&mut a, 140),
        " x:actions d:changes r:refresh │ Tab:area Shift+Tab:section /:search PgUp/PgDn:panel ?:help q:quit"
    );
    // den has no actions, so the view's group is empty.
    let mut a = docker(false);
    assert!(footer(&mut a, 140).starts_with(" d:changes r:refresh │ Tab:area"));
    // With live data, the keys for all areas get their own group.
}

#[test]
fn snapshot_and_online_keys_show_with_live_data_only() {
    let mut a = docker(true);
    let dir = std::env::temp_dir().join(format!("den-footer-{}", std::process::id()));
    a.online.use_file(dir.join("online.json"));
    a.snapshot_path = Some(dir.join("snapshot.json"));
    let line = footer(&mut a, 160);
    assert!(
        line.starts_with(" x:actions d:changes r:refresh │ S:snapshot u:check online │ Tab:area"),
        "{line}"
    );
    // The status line no longer carries u.
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn narrow_terminals_keep_this_views_keys_and_help() {
    let mut a = docker(true);
    for width in [60, 80, 100] {
        let line = footer(&mut a, width);
        assert!(
            line.starts_with(" x:actions d:changes r:refresh │"),
            "{width}: {line}"
        );
        assert!(line.ends_with("?:help q:quit"), "{width}: {line}");
        assert!(line.chars().count() <= width as usize, "{width}: {line}");
    }
}

#[test]
fn help_lists_only_its_own_keys() {
    let mut a = docker(true);
    a.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    let line = footer(&mut a, 140);
    assert!(line.starts_with(" Esc:back ↑↓:scroll"), "{line}");
    assert!(
        line.ends_with("│ q:quit") && !line.contains("Tab:area"),
        "{line}"
    );
}
