use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Focus},
    codex::{self, state::View},
    homebrew,
    presentation::{Destination, Detail, InspectorFocus, Page},
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};
fn app() -> App {
    let mut a = App::new(
        homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap(),
        "Homebrew snapshot",
    );
    let c = codex::Context::fixture(Path::new("fixtures/mcp"), None).unwrap();
    a.codex.configure(c.clone());
    a.codex
        .finish(codex::discover(&c, &AtomicBool::new(false)), &mut a.skills);
    a.codex.requested = false;
    a
}
fn key(a: &mut App, k: KeyCode) {
    a.handle_key(KeyEvent::new(k, KeyModifiers::NONE));
}
fn text(a: &mut App, s: &str) {
    for c in s.chars() {
        key(a, KeyCode::Char(c));
    }
}
fn switch(a: &mut App, d: Destination) {
    key(a, KeyCode::Char('g'));
    key(a, KeyCode::Char('/'));
    text(a, d.label());
    key(a, KeyCode::Enter);
    assert_eq!(a.destination(), d);
}
fn detail(a: &mut App, d: Detail) {
    if matches!(
        a.presentation.page,
        Page::Inspector {
            focus: InspectorFocus::Content,
            ..
        }
    ) {
        key(a, KeyCode::Tab);
    }
    key(a, KeyCode::Char('/'));
    a.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    text(a, d.label());
    key(a, KeyCode::Enter);
    key(a, KeyCode::Enter);
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
fn all_destinations_are_reachable_without_function_keys() {
    for d in Destination::ALL {
        let mut a = app();
        switch(&mut a, d);
        let s = draw(&mut a, 120, 32);
        assert!(s.contains("g:sections"));
        assert!(!s.contains("F2:"));
    }
}
#[test]
fn switcher_cancel_and_empty_search_preserve_resource_context() {
    let mut a = app();
    a.query = "widget".into();
    a.refresh();
    let selected = a.selected;
    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "no-such-section");
    assert!(draw(&mut a, 80, 24).contains("No matching sections"));
    key(&mut a, KeyCode::Enter);
    assert!(a.presentation.switcher.is_some());
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.query, "widget");
    assert_eq!(a.selected, selected);
}
#[test]
fn package_search_inspect_back_retains_query_and_selection() {
    let mut a = app();
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "ripgrep");
    key(&mut a, KeyCode::Enter);
    let id = a.resource().unwrap().id.clone();
    key(&mut a, KeyCode::Enter);
    assert!(a.inspecting());
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            detail: Detail::Overview,
            ..
        }
    ));
    key(&mut a, KeyCode::Esc);
    assert!(!a.inspecting());
    assert_eq!(a.query, "ripgrep");
    assert_eq!(a.resource().unwrap().id, id);
    assert_eq!(a.focus, Focus::Inventory);
}
#[test]
fn related_package_escape_restores_dependency_context_and_expansion() {
    let mut a = app();
    key(&mut a, KeyCode::Enter);
    detail(&mut a, Detail::Dependencies);
    let selected = a
        .explorer
        .rows
        .iter()
        .position(|r| r.target.is_some())
        .unwrap();
    a.explorer.selected = selected;
    a.explorer.tree = true;
    a.explorer.expanded.insert(vec![17]);
    let id = a.resource().unwrap().id.clone();
    let expanded = a.explorer.expanded.clone();
    key(&mut a, KeyCode::Enter);
    assert_ne!(a.resource().unwrap().id, id);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.resource().unwrap().id, id);
    assert_eq!(a.explorer.section, den::explorer::Section::Dependencies);
    assert_eq!(a.explorer.selected, selected);
    assert_eq!(a.explorer.expanded, expanded);
    assert!(a.explorer.tree);
}
#[test]
fn sections_restore_search_filters_and_inspector_position() {
    let mut a = app();
    switch(&mut a, Destination::Mcp);
    a.codex.query = "docs".into();
    a.codex.mcp.transport = 1;
    key(&mut a, KeyCode::Enter);
    detail(&mut a, Detail::Transport);
    a.codex.scroll = 7;
    switch(&mut a, Destination::Configuration);
    switch(&mut a, Destination::Mcp);
    assert_eq!(a.codex.query, "docs");
    assert_eq!(a.codex.mcp.transport, 1);
    assert_eq!(a.codex.scroll, 7);
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            detail: Detail::Transport,
            ..
        }
    ));
}
#[test]
fn evidence_escape_restores_original_overview_focus_and_scroll() {
    let mut a = app();
    key(&mut a, KeyCode::Enter);
    a.detail_scroll = 2;
    detail(&mut a, Detail::Evidence);
    assert!(a.evidence());
    assert!(draw(&mut a, 120, 32).contains("on_request:"));
    key(&mut a, KeyCode::Esc);
    assert!(!a.evidence());
    assert_eq!(a.detail_scroll, 2);
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            focus: InspectorFocus::Sections,
            ..
        }
    ));
    key(&mut a, KeyCode::Esc);
    assert!(!a.inspecting());
}
#[test]
fn source_jump_escape_restores_server_and_filters() {
    let mut a = app();
    switch(&mut a, Destination::Mcp);
    a.codex.query = "docs".into();
    key(&mut a, KeyCode::Enter);
    detail(&mut a, Detail::Sources);
    assert_eq!(a.codex.view, View::Configuration);
    assert!(a.codex.sources);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.codex.view, View::Mcp);
    assert_eq!(a.codex.query, "docs");
}
#[test]
fn inspector_focus_and_evidence_are_responsive_and_private() {
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let mut a = app();
        switch(&mut a, Destination::Mcp);
        a.codex.query = "docs".into();
        key(&mut a, KeyCode::Enter);
        let s = draw(&mut a, w, h);
        assert!(s.contains("Overview"));
        key(&mut a, KeyCode::Tab);
        assert!(matches!(
            a.presentation.page,
            Page::Inspector {
                focus: InspectorFocus::Sections,
                ..
            }
        ));
        for d in [
            Detail::Transport,
            Detail::Authentication,
            Detail::Tools,
            Detail::Evidence,
        ] {
            detail(&mut a, d);
            let s = draw(&mut a, w, h);
            assert!(!s.contains("MCP-SECRET-CANARY"));
            assert!(!s.contains("Compact") && !s.contains("Detailed"));
        }
    }
}
#[test]
fn navigation_never_collects_and_snapshot_checks_remain_disabled() {
    let mut a = app();
    for d in Destination::ALL {
        switch(&mut a, d);
        assert!(!a.refresh_requested && !a.codex.requested);
        assert!(a.check_requested.is_none());
    }
    switch(&mut a, Destination::Cleanup);
    assert!(!a.detail_choices().contains(&Detail::RunCheck));
}
#[test]
fn history_survives_refresh_without_restoring_an_old_graph() {
    let mut a = app();
    key(&mut a, KeyCode::Enter);
    detail(&mut a, Detail::Dependencies);
    let mut new = a.inventory.clone();
    new.resources.remove(0);
    a.finish_collection(Ok(new));
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.inventory.resources.len(), 6);
    assert!(
        a.selected_node()
            .is_none_or(|n| n < a.inventory.resources.len())
    );
    draw(&mut a, 80, 24);
}
#[test]
fn history_is_bounded_under_repeated_section_changes() {
    let mut a = app();
    for _ in 0..100 {
        switch(&mut a, Destination::Skills);
        switch(&mut a, Destination::Homebrew);
    }
    for _ in 0..100 {
        key(&mut a, KeyCode::Esc);
    }
    assert!(!a.inspecting());
    draw(&mut a, 80, 24);
}
#[test]
fn switcher_help_and_query_never_execute_navigation_keys_as_actions() {
    let mut a = app();
    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('?'));
    assert!(a.help);
    draw(&mut a, 80, 24);
    key(&mut a, KeyCode::Esc);
    assert!(a.presentation.switcher.is_some());
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "qrv?g");
    assert!(!a.quit && !a.help && !a.refresh_requested);
    assert_eq!(a.presentation.switcher.as_ref().unwrap().query, "qrv?g");
}

#[test]
fn switcher_arrows_select_filtered_matches_while_searching() {
    let mut a = app();
    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "Codex");
    for _ in 0..4 {
        key(&mut a, KeyCode::Down);
    }
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.destination(), Destination::Mcp);
}
#[test]
fn inspector_actions_reuse_read_only_commands_without_new_collection() {
    let mut a = app();
    a.live = true;
    a.navigate(Destination::Cleanup);
    assert!(a.detail_choices().contains(&Detail::RunCheck));
    a.open_detail(Detail::RunCheck);
    assert_eq!(a.check_requested, Some(den::diagnostics::Check::Cleanup));
    assert!(!a.refresh_requested && !a.codex.requested);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.destination(), Destination::Cleanup);
}

#[test]
#[ignore = "explicit synthetic navigation capture regeneration"]
fn navigation_captures() {
    std::fs::create_dir_all("docs/ux2").unwrap();
    let mut a = app();
    key(&mut a, KeyCode::Char('g'));
    std::fs::write("docs/ux2/sections-80x24.txt", draw(&mut a, 80, 24)).unwrap();
    key(&mut a, KeyCode::Esc);
    key(&mut a, KeyCode::Enter);
    std::fs::write("docs/ux2/inspector-120x32.txt", draw(&mut a, 120, 32)).unwrap();
    detail(&mut a, Detail::Evidence);
    std::fs::write("docs/ux2/evidence-80x24.txt", draw(&mut a, 80, 24)).unwrap();
}
