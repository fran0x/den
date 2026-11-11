// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Focus},
    codex, homebrew,
    presentation::{Destination, Detail},
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};
fn app() -> App {
    let mut a = App::new(
        homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap(),
        "Homebrew snapshot",
    );
    // These tests cover navigation over the full fixture; Tools is covered in homebrew_filters.
    a.show_all = true;
    a.refresh();
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
/// Reach a destination with real keys: Tab for the area, Shift+Tab for the section,
/// then 6-8 for a specific Diagnostics report.
fn switch(a: &mut App, d: Destination) {
    let area = d.label().split(" / ").next().unwrap();
    for _ in 0..4 {
        if a.destination().label().starts_with(area) {
            break;
        }
        key(a, KeyCode::Tab);
    }
    for _ in 0..5 {
        if a.destination().section() == d.section() {
            break;
        }
        key(a, KeyCode::BackTab);
    }
    match d {
        Destination::Missing => key(a, KeyCode::Char('6')),
        Destination::Leaves => key(a, KeyCode::Char('7')),
        Destination::Cleanup => key(a, KeyCode::Char('8')),
        _ => {}
    }
    assert_eq!(a.destination(), d);
}
const DESTINATIONS: [Destination; 9] = [
    Destination::Homebrew,
    Destination::Unresolved,
    Destination::Missing,
    Destination::Leaves,
    Destination::Cleanup,
    Destination::Skills,
    Destination::Settings,
    Destination::Instructions,
    Destination::Mcp,
];
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
fn tab_switches_areas_and_restores_the_last_codex_subview_and_inventory_context() {
    let mut a = app();
    a.query = "widget".into();
    a.filter = Some(den::inventory::ResourceKind::Formula);
    a.refresh();
    let package = a.resource().unwrap().id.clone();
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Skills);
    a.navigate(Destination::Mcp);
    a.agents.query = "docs".into();
    a.agents_clamp();
    key(&mut a, KeyCode::Down);
    let server = a.agent_row();
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Aws);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Containers);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::OnPath);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Homebrew);
    assert_eq!(a.query, "widget");
    assert_eq!(a.filter, Some(den::inventory::ResourceKind::Formula));
    assert_eq!(a.resource().unwrap().id, package);
    assert_eq!(a.active_focus(), Focus::Inventory);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Mcp);
    assert_eq!(a.agents.query, "docs");
    assert_eq!(a.agent_row(), server);
    assert_eq!(a.active_focus(), Focus::Inventory);
    assert!(!a.refresh_requested && !a.codex.requested && !a.skills.requested);
}

#[test]
fn tab_preserves_panel_scroll_and_search_queries() {
    let mut a = app();
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "rip");
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Skills);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Aws);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Containers);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::OnPath);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.query, "rip");
    assert!(!a.searching);
    key(&mut a, KeyCode::Tab);
    switch(&mut a, Destination::Mcp);
    draw(&mut a, 80, 24);
    key(&mut a, KeyCode::PageDown);
    let offset = a.agents.panel.offset;
    assert!(offset > 0);
    for _ in 0..4 {
        key(&mut a, KeyCode::Tab); // Cloud, Docker, Toolchains, Homebrew
    }
    assert_eq!(a.destination(), Destination::Homebrew);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Mcp);
    assert_eq!(a.agents.panel.offset, offset);
    // Area and section switches never add a Back step.
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.destination(), Destination::Mcp);
    assert!(!a.inspecting());
}

fn information_text(screen: &str, width: u16) -> String {
    if width < 100 {
        screen
            .lines()
            .skip_while(|line| !line.contains(" Information "))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        screen
            .lines()
            .map(|line| {
                line.chars()
                    .skip((width as usize * 45) / 100)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[test]
fn browsing_selection_updates_passive_information_in_every_resource_view() {
    for destination in [
        Destination::Homebrew,
        Destination::Skills,
        Destination::Settings,
        Destination::Sources,
        Destination::Instructions,
        Destination::Mcp,
    ] {
        for (width, height) in [(80, 24), (120, 32), (160, 45)] {
            let mut a = app();
            let c = codex::Context::fixture(Path::new("fixtures/codex"), None).unwrap();
            if matches!(destination, Destination::Skills | Destination::Instructions) {
                a.codex.configure(c.clone());
                a.codex
                    .finish(codex::discover(&c, &AtomicBool::new(false)), &mut a.skills);
                a.codex.requested = false;
            }
            if destination == Destination::Skills {
                let c = den::skills::Config::fixture(Path::new("fixtures/skills")).unwrap();
                a.skills
                    .finish(den::skills::discover(&c, &AtomicBool::new(false)));
            }
            a.navigate(destination);
            let before = information_text(&draw(&mut a, width, height), width);
            assert!(before.contains("Information"));
            key(&mut a, KeyCode::Right);
            key(&mut a, KeyCode::Down);
            let after = information_text(&draw(&mut a, width, height), width);
            assert_ne!(before, after, "{destination:?} at {width}");
            assert_eq!(a.active_focus(), Focus::Inventory);
            assert!(!a.inspecting());
            assert!(!after.contains("> Information"));
            assert!(
                !after.contains('\u{fffd}'),
                "layout newlines must remain newlines"
            );
            assert!(after.contains('\n'));
            let screen = draw(&mut a, width, height);
            assert!(screen.lines().last().unwrap().contains("Tab:area"));
            assert!(!screen.lines().next().unwrap().contains("Tab:area"));
            assert!(!screen.contains("Tab:pane") && !screen.contains("Tab:list"));
        }
    }
}

#[test]
fn passive_information_handles_empty_and_long_content_without_taking_inventory_input() {
    let mut a = app();
    a.inventory.resources[0].description = "Private prose stays data. ".repeat(200);
    for (width, height) in [(30, 10), (80, 24), (120, 32)] {
        let screen = draw(&mut a, width, height);
        let information = information_text(&screen, width);
        if width == 30 {
            assert!(information.contains("Depends on / Required by"));
        } else {
            assert!(information.contains("enlarge the terminal"));
        }
        key(&mut a, KeyCode::Home);
        key(&mut a, KeyCode::Right);
        key(&mut a, KeyCode::End);
        assert_eq!(a.selected, Some(a.visible.len() - 1));
        assert_eq!(a.detail_scroll, 0);
        key(&mut a, KeyCode::Home);
    }
    a.query = "no-such-package".into();
    a.refresh();
    let screen = draw(&mut a, 80, 24);
    assert!(screen.contains("No matches"));
    assert!(information_text(&screen, 80).contains("Select a resource"));
    assert_eq!(a.active_focus(), Focus::Inventory);
}
#[test]
fn shift_tab_cycles_sections_and_the_top_bar_names_the_active_one() {
    let mut a = app();
    let mut seen = vec![];
    for _ in 0..2 {
        seen.push(a.destination());
        key(&mut a, KeyCode::BackTab);
    }
    key(&mut a, KeyCode::Tab);
    for _ in 0..7 {
        seen.push(a.destination());
        let screen = draw(&mut a, 120, 32);
        let top = screen.lines().next().unwrap().to_owned();
        let section = a.destination().label().split_once(" / ").unwrap().1;
        assert!(top.contains(&format!("Agents  › {section}")), "{top}");
        assert!(!top.contains("Tab:area"));
        let footer = screen.lines().last().unwrap();
        assert!(footer.contains("Shift+Tab:section") && !footer.contains("g:"));
        key(&mut a, KeyCode::BackTab);
    }
    use Destination::*;
    assert_eq!(
        seen,
        [
            Homebrew,
            Unresolved,
            Skills,
            Instructions,
            Settings,
            Mcp,
            Hooks,
            Plugins,
            Skills
        ]
    );
    // g no longer opens anything.
    key(&mut a, KeyCode::Char('g'));
    assert!(!a.inspecting() && !a.help);
    assert_eq!(a.destination(), Instructions);
}

#[test]
fn agents_footers_name_section_keys_and_enter_opens_nothing() {
    for destination in [
        Destination::Skills,
        Destination::Settings,
        Destination::Sources,
        Destination::Instructions,
        Destination::Mcp,
    ] {
        for (width, height) in [(80, 24), (120, 32), (160, 45)] {
            for arrow in [KeyCode::Left, KeyCode::Right] {
                let mut a = app();
                if destination == Destination::Skills {
                    let config =
                        den::skills::Config::fixture(Path::new("fixtures/skills")).unwrap();
                    a.skills
                        .finish(den::skills::discover(&config, &AtomicBool::new(false)));
                }
                a.navigate(destination);
                key(&mut a, arrow);
                assert_eq!(a.active_focus(), Focus::Inventory);
                let browse = draw(&mut a, width, height);
                let footer = browse.lines().last().unwrap();
                assert!(
                    footer.contains("Shift+Tab:section") && !footer.contains("Enter"),
                    "{destination:?}, {arrow:?}, {width}: {footer}"
                );
                key(&mut a, KeyCode::Enter);
                assert!(!a.inspecting());
                assert_eq!(a.destination(), destination);
                assert_eq!(a.active_focus(), Focus::Inventory);
            }
        }
    }
}

#[test]
fn empty_inventory_hints_offer_back_without_resetting_filters() {
    for destination in [Destination::Homebrew, Destination::Skills] {
        let mut a = app();
        a.navigate(destination);
        let initial = a.show_all;
        if destination == Destination::Homebrew {
            key(&mut a, KeyCode::Char('a'));
        }
        let show_all = a.show_all;
        assert_eq!(show_all != initial, destination == Destination::Homebrew);
        key(&mut a, KeyCode::Char('/'));
        text(&mut a, "no-such-resource");
        key(&mut a, KeyCode::Enter);
        for (width, height) in [(80, 24), (120, 32)] {
            let inventory = draw(&mut a, width, height);
            assert!(inventory.contains("No match"));
            assert!(!inventory.contains("Esc resets") && !inventory.contains("Esc:reset"));
            key(&mut a, KeyCode::Enter);
            assert!(!a.inspecting()); // Neither packages nor skills have an inspector.
        }
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.show_all, show_all);
    }
}

#[test]
fn search_hints_distinguish_finishing_edits_from_opening_destinations() {
    let mut a = app();
    key(&mut a, KeyCode::Char('/'));
    let search = draw(&mut a, 80, 24);
    let footer = search.lines().last().unwrap();
    assert!(footer.contains("Enter:done") && footer.contains("Esc:done"));
    assert!(!footer.contains("q:quit"));
    text(&mut a, "ripgrep");
    key(&mut a, KeyCode::Enter);
    assert!(!a.searching && !a.inspecting());
    key(&mut a, KeyCode::Enter);
    assert!(!a.inspecting()); // Packages have no inspector.

    switch(&mut a, Destination::Mcp);
    key(&mut a, KeyCode::Char('/'));
    let search = draw(&mut a, 80, 24);
    assert!(search.lines().last().unwrap().contains("Enter:done"));
    text(&mut a, "docs");
    key(&mut a, KeyCode::Enter);
    assert!(!a.agents.searching && !a.inspecting());
    assert_eq!(a.agents.query, "docs");
    key(&mut a, KeyCode::Enter);
    assert!(!a.inspecting());
}

#[test]
fn sections_restore_search_filters_and_panel_position() {
    let mut a = app();
    switch(&mut a, Destination::Mcp);
    a.agents.query = "docs".into();
    a.agents.panel.offset = 7;
    switch(&mut a, Destination::Settings);
    switch(&mut a, Destination::Mcp);
    assert_eq!(a.agents.query, "docs");
    assert_eq!(a.agents.panel.offset, 7);
    assert!(!a.inspecting());
}
#[test]
fn mcp_panel_stacks_sections_at_every_size_and_stays_private() {
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let mut a = app();
        switch(&mut a, Destination::Mcp);
        a.agents.query = "docs".into();
        let mut seen = String::new();
        loop {
            let s = draw(&mut a, w, h);
            assert!(!s.contains("MCP-SECRET-CANARY"));
            seen.push_str(&s);
            if a.agents.panel.offset >= a.agents.panel.max {
                break;
            }
            key(&mut a, KeyCode::PageDown);
        }
        for section in [
            "Transport",
            "Authentication",
            "Tools and approvals",
            "Limits",
        ] {
            assert!(seen.contains(section), "{w}: {section}");
        }
    }
}
#[test]
fn navigation_never_collects_and_snapshot_checks_remain_disabled() {
    let mut a = app();
    for d in DESTINATIONS {
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
    a.jump_to(a.graph.preferred(1));
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
