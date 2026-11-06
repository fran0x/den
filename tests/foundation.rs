use std::io::Cursor;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use den::{
    app::{App, Focus},
    inventory::{Inventory, ResourceKind, safe_text},
    source, ui,
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn app() -> App {
    App::new(source::demo().unwrap(), "synthetic demo")
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}
fn screen(buffer: &Buffer) -> String {
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn navigation_clamps_and_page_moves_use_viewport() {
    let mut app = app();
    draw(&mut app, 100, 14);
    key(&mut app, KeyCode::Up);
    assert_eq!(app.selected, Some(0));
    key(&mut app, KeyCode::PageDown);
    assert_eq!(app.selected, Some(app.page_size));
    key(&mut app, KeyCode::End);
    assert_eq!(app.selected, Some(9));
    key(&mut app, KeyCode::Down);
    assert_eq!(app.selected, Some(9));
    draw(&mut app, 100, 14);
    assert!(app.list_offset > 0);
    key(&mut app, KeyCode::Home);
    assert_eq!(app.selected, Some(0));
}

#[test]
fn live_search_matches_all_words_across_fields_and_ignores_case() {
    let mut app = app();
    key(&mut app, KeyCode::Char('/'));
    for ch in "RUST toolchain".chars() {
        key(&mut app, KeyCode::Char(ch));
    }
    assert_eq!(app.visible.len(), 1);
    assert_eq!(app.resource().unwrap().name, "Rust");
    key(&mut app, KeyCode::Esc);
    assert!(!app.searching);
    assert_eq!(app.query, "RUST toolchain");
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.visible.len(), 10);
}

#[test]
fn search_is_literal_unicode_safe_and_does_not_dispatch_shortcuts() {
    let mut app = app();
    key(&mut app, KeyCode::Char('/'));
    for ch in "q?fé".chars() {
        key(&mut app, KeyCode::Char(ch));
    }
    assert!(!app.quit && !app.help);
    assert_eq!(app.filter, None);
    assert_eq!(app.query, "q?fé");
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.query, "q?f");
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert!(app.query.is_empty());
}

#[test]
fn filters_intersect_search_and_cycle_back_to_all() {
    let mut app = app();
    app.query = "git".into();
    app.refresh();
    key(&mut app, KeyCode::Char('f'));
    assert_eq!(app.filter, Some(ResourceKind::Package));
    assert_eq!(app.visible.len(), 1);
    key(&mut app, KeyCode::Char('f'));
    assert!(app.visible.is_empty());
    assert_eq!(app.selected, None);
    for _ in 0..6 {
        key(&mut app, KeyCode::Char('f'));
    }
    assert_eq!(app.filter, None);
    assert_eq!(app.resource().unwrap().name, "Git");
}

#[test]
fn refresh_preserves_selection_when_it_still_matches() {
    let mut app = app();
    key(&mut app, KeyCode::Down);
    app.query = "fd".into();
    app.refresh();
    assert_eq!(app.resource().unwrap().name, "fd");
    app.query.clear();
    app.refresh();
    assert_eq!(app.selected, Some(1));
}

#[test]
fn no_matches_is_safe_and_can_recover() {
    let mut app = app();
    app.query = "missing resource".into();
    app.refresh();
    for code in [KeyCode::Down, KeyCode::End, KeyCode::PageUp] {
        key(&mut app, code);
    }
    assert_eq!(app.selected, None);
    let rendered = screen(&draw(&mut app, 100, 24));
    assert!(rendered.contains("No matches"));
    assert!(rendered.contains("Select a resource"));
    key(&mut app, KeyCode::Char('/'));
    app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.selected, Some(0));
}

#[test]
fn empty_inventory_renders_and_navigates() {
    let mut app = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot (unverified)",
    );
    key(&mut app, KeyCode::End);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Down);
    assert!(screen(&draw(&mut app, 80, 24)).contains("0/0 resources"));
}

#[test]
fn focus_routes_scroll_and_resize_clamps_it() {
    let mut app = app();
    draw(&mut app, 40, 16);
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.focus, Focus::Details);
    draw(&mut app, 40, 16); // focused pane now occupies narrow terminals
    let selected = app.selected;
    key(&mut app, KeyCode::End);
    assert!(app.detail_scroll > 0);
    assert_eq!(app.selected, selected);
    draw(&mut app, 160, 60);
    assert_eq!(app.detail_scroll, 0);
    key(&mut app, KeyCode::BackTab);
    assert_eq!(app.focus, Focus::Inventory);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.focus, Focus::Details);
    key(&mut app, KeyCode::Left);
    assert_eq!(app.focus, Focus::Inventory);
}

#[test]
fn help_is_modal_scrollable_and_restores_search_mode() {
    let mut app = app();
    key(&mut app, KeyCode::Char('/'));
    key(&mut app, KeyCode::F(1));
    assert!(screen(&draw(&mut app, 40, 16)).contains("Help"));
    key(&mut app, KeyCode::End);
    assert!(app.help_scroll > 0);
    assert_eq!(app.selected, Some(0));
    key(&mut app, KeyCode::Esc);
    assert!(!app.help && app.searching);
}

#[test]
fn ctrl_c_quits_in_every_mode_and_releases_are_ignored() {
    for mode in 0..3 {
        let mut app = app();
        app.searching = mode == 1;
        app.help = mode == 2;
        app.handle_key(KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ));
        assert!(!app.quit);
        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit);
    }
}

#[test]
fn wide_and_narrow_layouts_show_panes_details_and_status() {
    for (width, height) in [(120, 30), (45, 30), (30, 10)] {
        let mut app = app();
        let rendered = draw(&mut app, width, height);
        let text = screen(&rendered);
        assert!(text.contains("Inventory"));
        assert!(text.contains("ripgrep"));
        if width >= 100 {
            assert!(text.contains("Details"));
            let row = |needle: &str| {
                (0..height)
                    .find(|&y| {
                        (0..width)
                            .map(|x| rendered[(x, y)].symbol())
                            .collect::<String>()
                            .contains(needle)
                    })
                    .unwrap()
            };
            let title_line = row("Inventory");
            let detail_line = row("Details");
            assert_eq!(title_line, detail_line);
        } else {
            assert!(!text.contains("Details"));
            key(&mut app, KeyCode::Tab);
            let detail = screen(&draw(&mut app, width, height));
            assert!(detail.contains("Details"));
            assert!(!detail.contains("Inventory"));
        }
    }
}

#[test]
fn rendering_is_safe_at_tiny_sizes() {
    for width in [0, 1, 10, 29, 30, 80] {
        for height in [0, 1, 5, 9, 10] {
            draw(&mut app(), width, height);
        }
    }
    assert!(screen(&draw(&mut app(), 29, 5)).contains("resize"));
}

#[test]
fn hostile_fields_never_reach_terminal_as_control_sequences() {
    let hostile = "evil\u{1b}]52;c;clipboard\u{7}\u{9b}31m\r\t\u{202e}text";
    let cleaned = safe_text(hostile);
    assert!(!cleaned.chars().any(char::is_control));
    assert!(!cleaned.contains('\u{202e}'));
    assert!(cleaned.contains('\u{fffd}'));
    let mut app = app();
    let r = &mut app.inventory.resources[0];
    r.id = hostile.into();
    r.name = hostile.into();
    r.path = Some(hostile.into());
    r.version = Some(hostile.into());
    r.description = hostile.into();
    r.reason = hostile.into();
    r.origin = hostile.into();
    app.query = "\u{1b}".into();
    let rendered = screen(&draw(&mut app, 160, 60));
    assert!(!rendered.chars().any(char::is_control));
    assert!(!rendered.contains('\u{202e}'));
    assert!(rendered.contains("Unknown - not assessed"));
}

#[test]
fn fixture_round_trips_and_local_snapshot_matches_embedded_demo() {
    let inventory = source::demo().unwrap();
    let bytes = serde_json::to_vec(&inventory).unwrap();
    assert_eq!(
        source::read_snapshot(Cursor::new(bytes)).unwrap(),
        inventory
    );
    assert_eq!(
        source::load_snapshot(std::path::Path::new("fixtures/demo.json")).unwrap(),
        inventory
    );
}

#[test]
fn invalid_snapshot_schema_and_duplicate_ids_are_rejected() {
    for data in [
        "{",
        r#"{"schema_version":2,"resources":[]}"#,
        r#"{"schema_version":1,"resources":[],"extra":true}"#,
    ] {
        assert!(source::read_snapshot(Cursor::new(data)).is_err());
    }
    let mut inventory = source::demo().unwrap();
    inventory.resources.push(inventory.resources[0].clone());
    assert!(source::read_snapshot(Cursor::new(serde_json::to_vec(&inventory).unwrap())).is_err());
    inventory.resources.pop();
    inventory.resources[0].name = " ".into();
    assert!(source::read_snapshot(Cursor::new(serde_json::to_vec(&inventory).unwrap())).is_err());
}

#[test]
fn snapshot_limits_are_enforced() {
    assert!(
        source::read_snapshot(Cursor::new(vec![
            b' ';
            source::MAX_SNAPSHOT_BYTES as usize + 1
        ]))
        .is_err()
    );
    let mut inventory = source::demo().unwrap();
    inventory.resources[0].description = "a".repeat(8193);
    assert!(source::read_snapshot(Cursor::new(serde_json::to_vec(&inventory).unwrap())).is_err());
    assert!(source::load_snapshot(std::path::Path::new("fixtures")).is_err());
    assert!(source::load_snapshot(std::path::Path::new("fixtures/does-not-exist.json")).is_err());
}
