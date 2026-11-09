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
        key(a, KeyCode::Char('['));
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
fn tab_switches_areas_and_restores_the_last_codex_subview_and_inventory_context() {
    let mut a = app();
    a.query = "widget".into();
    a.filter = Some(den::inventory::ResourceKind::Formula);
    a.refresh();
    let package = a.resource().unwrap().id.clone();
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Skills);
    a.navigate(Destination::Mcp);
    a.codex.query = "docs".into();
    a.codex.clamp();
    key(&mut a, KeyCode::Char('f'));
    let controls = a.codex.mcp.label();
    let server = a.codex.rows()[a.codex.selected[3]].clone();
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Homebrew);
    assert_eq!(a.query, "widget");
    assert_eq!(a.filter, Some(den::inventory::ResourceKind::Formula));
    assert_eq!(a.resource().unwrap().id, package);
    assert_eq!(a.active_focus(), Focus::Inventory);
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.destination(), Destination::Mcp);
    assert_eq!(a.codex.query, "docs");
    assert_eq!(a.codex.rows()[a.codex.selected[3]], server);
    assert_eq!(a.codex.mcp.label(), controls);
    assert_eq!(a.active_focus(), Focus::Inventory);
    assert!(!a.refresh_requested && !a.codex.requested && !a.skills.requested);
}

#[test]
fn tab_preserves_explicit_inspector_context_and_search_queries() {
    let mut a = app();
    a.finish_collection(Ok(homebrew::load_snapshot(Path::new(
        "fixtures/brew-dependencies.json",
    ))
    .unwrap()));
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "root-a");
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Skills);
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.query, "root-a");
    assert!(!a.searching);
    key(&mut a, KeyCode::Enter);
    a.open_detail(Detail::Dependencies);
    a.explorer.selected = a
        .explorer
        .rows
        .iter()
        .position(|r| r.label.contains("mid"))
        .unwrap();
    key(&mut a, KeyCode::Right);
    let expanded = a.explorer.expanded.clone();
    assert!(!expanded.is_empty());
    let page = a.presentation.page.clone();
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Skills);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.presentation.page, page);
    assert_eq!(a.explorer.expanded, expanded);
    assert_eq!(a.query, "root-a");
    key(&mut a, KeyCode::Char('['));
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            focus: InspectorFocus::Sections,
            ..
        }
    ));
    key(&mut a, KeyCode::Char(']'));
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            focus: InspectorFocus::Content,
            ..
        }
    ));
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.destination(), Destination::Skills);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.destination(), Destination::Dependencies);
    assert_eq!(a.explorer.expanded, expanded);
}

#[test]
fn area_return_after_refresh_revalidates_identity_and_discards_old_tree_indices() {
    let mut a = app();
    a.finish_collection(Ok(homebrew::load_snapshot(Path::new(
        "fixtures/brew-dependencies.json",
    ))
    .unwrap()));
    a.query = "root-a".into();
    a.refresh();
    key(&mut a, KeyCode::Enter);
    a.open_detail(Detail::Dependencies);
    a.explorer.selected = a
        .explorer
        .rows
        .iter()
        .position(|r| r.label.contains("mid"))
        .unwrap();
    key(&mut a, KeyCode::Right);
    assert!(!a.explorer.expanded.is_empty());
    key(&mut a, KeyCode::Tab);
    let mut changed = a.inventory.clone();
    changed.resources.reverse();
    a.finish_collection(Ok(changed));
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.resource().unwrap().name, "root-a");
    assert_eq!(a.destination(), Destination::Dependencies);
    assert!(a.explorer.expanded.is_empty());
    assert!(a.explorer.tree);
    assert!(a.explorer.rows.iter().all(|row| row.branch.len() == 1));
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.destination(), Destination::Skills);
    key(&mut a, KeyCode::F(2));
    assert_eq!(a.destination(), Destination::Dependencies);
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
        Destination::Configuration,
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
            assert!(screen.lines().next().unwrap().contains("Tab:area"));
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
            assert!(information.contains("Enter:inspect"));
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
fn browse_and_inspector_hints_match_enter_and_escape_routing() {
    for destination in [
        Destination::Homebrew,
        Destination::Skills,
        Destination::Configuration,
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
                    footer.contains("Enter:inspect") && footer.contains("Esc:Back"),
                    "{destination:?}, {arrow:?}, {width}: {footer}"
                );
                assert!(!footer.contains("Enter:source") && !footer.contains("Enter:preview"));
                key(&mut a, KeyCode::Enter);
                assert!(matches!(
                    a.presentation.page,
                    Page::Inspector {
                        detail: Detail::Overview,
                        focus: InspectorFocus::Content,
                        ..
                    }
                ));
                let overview = draw(&mut a, width, height);
                assert!(!overview.contains("Enter: inspect"));
                assert!(overview.lines().last().unwrap().contains("[:sections"));
                key(&mut a, KeyCode::Char('['));
                assert!(
                    draw(&mut a, width, height)
                        .lines()
                        .last()
                        .unwrap()
                        .contains("Enter:open")
                );
                key(&mut a, KeyCode::Enter);
                assert!(matches!(
                    a.presentation.page,
                    Page::Inspector {
                        focus: InspectorFocus::Content,
                        ..
                    }
                ));
                key(&mut a, KeyCode::Esc);
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
        key(&mut a, KeyCode::Char('i'));
        let intent = a.intent_filter;
        let skill_filter = a.skills.filter;
        assert!(if destination == Destination::Homebrew {
            intent.is_some()
        } else {
            skill_filter.is_some()
        });
        key(&mut a, KeyCode::Char('/'));
        text(&mut a, "no-such-resource");
        key(&mut a, KeyCode::Enter);
        for (width, height) in [(80, 24), (120, 32)] {
            let inventory = draw(&mut a, width, height);
            assert!(inventory.contains("No matches") || inventory.contains("No matching skills"));
            assert!(inventory.contains("Esc:Back"));
            assert!(!inventory.contains("Esc resets") && !inventory.contains("Esc:reset"));
            key(&mut a, KeyCode::Enter);
            let overview = draw(&mut a, width, height);
            assert!(overview.contains("Esc:Back"));
            assert!(!overview.contains("Esc resets") && !overview.contains("Esc:reset"));
            key(&mut a, KeyCode::Esc);
        }
        key(&mut a, KeyCode::Esc);
        assert_eq!(a.intent_filter, intent);
        assert_eq!(a.skills.filter, skill_filter);
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
    key(&mut a, KeyCode::Char('/'));
    let search = draw(&mut a, 80, 24);
    assert!(search.lines().last().unwrap().contains("Enter:done"));
    text(&mut a, "Dependencies");
    key(&mut a, KeyCode::Enter);
    assert!(!a.presentation.menu_searching);
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            detail: Detail::Overview,
            focus: InspectorFocus::Sections,
            ..
        }
    ));
    assert!(
        draw(&mut a, 80, 24)
            .lines()
            .last()
            .unwrap()
            .contains("Enter:open")
    );
    key(&mut a, KeyCode::Enter);
    assert!(matches!(
        a.presentation.page,
        Page::Inspector {
            detail: Detail::Dependencies,
            ..
        }
    ));

    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('/'));
    text(&mut a, "mcp");
    let search = draw(&mut a, 80, 24);
    let footer = search.lines().last().unwrap();
    assert!(footer.contains("Enter:open") && footer.contains("Esc:cancel"));
    assert!(footer.contains("Ctrl-C:quit") && !footer.contains("q:quit"));
    key(&mut a, KeyCode::Enter);
    assert!(a.presentation.switcher.is_none());
    assert_eq!(a.destination(), Destination::Mcp);
}

#[test]
fn switcher_q_quits_while_browsing_and_remains_text_during_search() {
    for inspect in [false, true] {
        let mut a = app();
        if inspect {
            key(&mut a, KeyCode::Enter);
        }
        key(&mut a, KeyCode::Char('g'));
        assert!(
            draw(&mut a, 80, 24)
                .lines()
                .last()
                .unwrap()
                .contains("q:quit")
        );
        key(&mut a, KeyCode::Char('q'));
        assert!(a.quit);
        assert_eq!(a.inspecting(), inspect);
    }

    let mut a = app();
    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('/'));
    key(&mut a, KeyCode::Char('q'));
    assert_eq!(a.presentation.switcher.as_ref().unwrap().query, "q");
    assert!(!a.quit);
    text(&mut a, "-no-such-section");
    let search = draw(&mut a, 80, 24);
    assert!(search.contains("No matching sections"));
    assert!(!search.lines().last().unwrap().contains("q:quit"));
    key(&mut a, KeyCode::Esc);
    assert!(a.presentation.switcher.is_none() && !a.quit);
    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('q'));
    assert!(a.quit);

    let mut a = app();
    key(&mut a, KeyCode::Char('g'));
    key(&mut a, KeyCode::Char('/'));
    a.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(a.quit);
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
        key(&mut a, KeyCode::Char('['));
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
