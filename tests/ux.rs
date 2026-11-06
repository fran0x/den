use den::{
    app::{App, Module},
    codex, homebrew, ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

fn brew() -> App {
    App::new(
        homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap(),
        "Homebrew snapshot",
    )
}
fn skills() -> App {
    let mut a = brew();
    let context = codex::Context::fixture(Path::new("fixtures/codex"), None).unwrap();
    a.module = Module::Skills;
    a.codex.configure(context.clone());
    a.codex.finish(
        codex::discover(&context, &AtomicBool::new(false)),
        &mut a.skills,
    );
    a.codex.requested = false;
    a
}
fn draw(a: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| t.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
#[ignore = "explicit synthetic terminal capture regeneration"]
fn captures() {
    let phase = std::env::var("DEN_CAPTURE_PHASE").unwrap_or("after".into());
    assert!(phase == "before" || phase == "after");
    std::fs::create_dir_all("docs/ux2").unwrap();
    for (name, mut a) in [("homebrew", brew()), ("skills", skills())] {
        let text = draw(&mut a, 120, 32);
        let root = std::env::current_dir().unwrap();
        let text = text.replace(root.to_str().unwrap(), "<repo>");
        std::fs::write(format!("docs/ux2/{name}-{phase}.txt"), format!("{text}\n")).unwrap();
    }
}

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{app::Focus, codex::state::View};
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn fixture(path: &str) -> App {
    let mut a = brew();
    let context = codex::Context::fixture(Path::new(path), None).unwrap();
    a.module = Module::Skills;
    a.codex.configure(context.clone());
    a.codex.finish(
        codex::discover(&context, &AtomicBool::new(false)),
        &mut a.skills,
    );
    a.codex.requested = false;
    a
}
#[test]
fn homebrew_rows_are_single_line_and_have_room_for_all_fixture_packages() {
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let mut a = brew();
        let s = draw(&mut a, w, h);
        let y = s.lines().position(|l| l.contains("> ripgrep")).unwrap();
        for (offset, name) in [
            "ripgrep",
            "pcre2",
            "python@3.12",
            "widget",
            "legacy",
            "widget",
            "requested-app",
        ]
        .iter()
        .enumerate()
        {
            assert!(
                s.lines().nth(y + offset).unwrap().contains(name),
                "{w}: {name}"
            );
        }
        assert!(s.contains("[Cask]"));
        assert!(!s.contains("on_request:"));
        assert!(!s.contains("brew:formula:"));
        assert!(a.page_size >= 19);
    }
}
#[test]
fn skills_rows_show_scope_and_eligibility_without_technical_noise() {
    let mut a = skills();
    let s = draw(&mut a, 120, 32);
    let row = s.lines().find(|s| s.contains("> example")).unwrap();
    assert!(row.contains("User") && row.contains("Eligible"));
    assert!(!s.contains("config not evaluated"));
    assert!(!s.contains("Discovered: yes") && !s.contains("Resolved:"));
    assert!(s.contains("Synthetic skill for configuration"));
    key(&mut a, KeyCode::Char('v'));
    let s = draw(&mut a, 160, 45);
    assert!(s.contains("Resolved:") && s.contains("Configuration status: Eligible"));
}
#[test]
fn overview_and_evidence_share_inventory_and_never_collect() {
    let mut a = brew();
    let id = a.resource().unwrap().id.clone();
    key(&mut a, KeyCode::Char('v'));
    let text = draw(&mut a, 160, 45);
    assert!(text.contains("on_request: true"));
    assert!(text.contains("ID: brew:formula:"));
    assert_eq!(a.resource().unwrap().id, id);
    assert!(!a.refresh_requested && !a.codex.requested);
    key(&mut a, KeyCode::Esc);
    assert!(!draw(&mut a, 160, 45).contains("on_request:"));
}
#[test]
fn inspector_back_retains_query_selection_filters_and_list_position() {
    let mut a = brew();
    a.query = "widget".into();
    a.refresh();
    key(&mut a, KeyCode::Down);
    let selected = a.selected;
    key(&mut a, KeyCode::Enter);
    assert!(a.inspecting());
    let s = draw(&mut a, 120, 32);
    assert!(!s.contains(" Inventory "));
    assert!(s.contains("Esc:Back"));
    key(&mut a, KeyCode::Esc);
    assert!(!a.inspecting());
    assert_eq!(a.focus, Focus::Inventory);
    assert_eq!(a.query, "widget");
    assert_eq!(a.selected, selected);
}
#[test]
fn narrow_layout_is_switchable_not_squeezed_or_stacked() {
    let mut a = skills();
    let list = draw(&mut a, 80, 24);
    assert!(list.contains("Skills inventory"));
    assert!(!list.contains("supporting entries"));
    key(&mut a, KeyCode::Tab);
    let detail = draw(&mut a, 80, 24);
    assert!(!detail.contains("Skills inventory"));
    assert!(detail.contains("supporting entries"));
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.skills.focus, Focus::Inventory);
}
#[test]
fn inspector_scrolling_and_help_preserve_reading_context() {
    let mut a = skills();
    a.skills.inventory.skills[0].instructions = (0..160)
        .map(|n| format!("Synthetic instruction line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Char('2'));
    draw(&mut a, 80, 24);
    key(&mut a, KeyCode::PageDown);
    let scroll = a.skills.scroll;
    assert!(scroll > 0);
    key(&mut a, KeyCode::Char('?'));
    draw(&mut a, 80, 24);
    key(&mut a, KeyCode::End);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.skills.scroll, scroll);
    assert!(a.inspecting());
    key(&mut a, KeyCode::Esc);
    assert!(a.inspecting()); // Overview restored
    assert_eq!(a.skills.section, den::skills::state::Section::Overview);
    key(&mut a, KeyCode::Esc);
    assert!(!a.inspecting());
}
#[test]
fn search_is_local_to_inspector_sections_and_does_not_dispatch_actions() {
    let mut a = brew();
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Char('/'));
    assert!(a.inspecting());
    assert!(a.presentation.menu_searching);
    for c in "v?z".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    assert!(
        matches!(&a.presentation.page,den::presentation::Page::Inspector{query,..} if query=="v?z")
    );
    assert!(!a.evidence() && !a.help);
    key(&mut a, KeyCode::Esc);
    assert!(draw(&mut a, 80, 24).contains("No matching sections"));
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.visible.len(), 7);
}
#[test]
fn fullscreen_state_survives_module_and_codex_view_switching() {
    let mut a = skills();
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::F(2));
    assert!(!a.inspecting());
    key(&mut a, KeyCode::F(2));
    assert!(a.inspecting());
    key(&mut a, KeyCode::F(3));
    assert_eq!(a.codex.view, View::Configuration);
    for _ in 0..3 {
        key(&mut a, KeyCode::F(3));
    }
    assert_eq!(a.codex.view, View::Skills);
    assert!(a.inspecting());
}
#[test]
fn f3_cycles_homebrew_relationships_and_all_diagnostics() {
    let mut a = brew();
    for n in 0..8 {
        key(&mut a, KeyCode::F(3));
        if n == 3 {
            assert_eq!(a.diagnostics.tab, Some(den::diagnostics::Tab::Unresolved));
        }
    }
    assert!(a.diagnostics.tab.is_none());
    assert_eq!(a.explorer.section, den::explorer::Section::Overview);
}
#[test]
fn mcp_inventory_has_transport_status_and_inspector_source_return() {
    let mut a = fixture("fixtures/mcp");
    a.codex.view = View::Mcp;
    a.codex.query = "docs".into();
    let s = draw(&mut a, 120, 32);
    let row = s.lines().find(|s| s.contains("> docs")).unwrap();
    assert!(row.contains("Stdio"));
    assert!(row.contains("Locally eligible"));
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Char('e'));
    assert_eq!(a.codex.view, View::Configuration);
    key(&mut a, KeyCode::Backspace);
    assert_eq!(a.codex.view, View::Mcp);
    assert_eq!(a.codex.query, "docs");
    assert!(a.inspecting());
}
#[test]
fn configuration_source_back_preserves_scroll_and_selection() {
    let mut a = skills();
    a.codex.view = View::Configuration;
    a.codex.query = "model".into();
    key(&mut a, KeyCode::Enter);
    a.codex.scroll = 3;
    key(&mut a, KeyCode::Char('e'));
    assert!(a.codex.sources);
    key(&mut a, KeyCode::Backspace);
    assert!(!a.codex.sources);
    assert_eq!(a.codex.query, "model");
    assert_eq!(a.codex.scroll, 3);
}
#[test]
fn long_unicode_names_and_paths_are_truncated_without_controls_or_panics() {
    let mut a = brew();
    a.inventory.resources[0].name = "長い名前".repeat(50);
    a.inventory.resources[0].description = "é\u{1b}[31m\u{202e}".repeat(60);
    a.inventory.resources[0].path = Some("/synthetic/very-long/".repeat(150));
    for detailed in [false, true] {
        if detailed {
            a.open_detail(den::presentation::Detail::Evidence);
        } else {
            a.presentation.page = den::presentation::Page::Browse;
        }
        for (w, h) in [(80, 24), (120, 32), (160, 45)] {
            let text = draw(&mut a, w, h);
            if !detailed {
                assert!(text.contains('…'));
            }
            assert!(!text.contains('\u{1b}'));
            assert!(!text.contains('\u{202e}'));
        }
    }
}
#[test]
fn empty_and_failed_views_remain_explainable_in_both_modes() {
    let mut a = fixture("fixtures/mcp-empty");
    a.codex.view = View::Mcp;
    for detailed in [false, true] {
        if detailed {
            a.open_detail(den::presentation::Detail::Evidence);
        } else {
            a.presentation.page = den::presentation::Page::Browse;
        }
        assert!(draw(&mut a, 120, 32).contains("No matching entries"));
        a.codex.error = Some("Collection failed; previous inventory retained (stale)".into());
        assert!(draw(&mut a, 80, 24).contains("Collection failed"));
    }
}
#[test]
fn all_codex_modes_inspectors_sections_and_sources_keep_secrets_redacted() {
    for fixture_path in ["fixtures/mcp", "fixtures/codex"] {
        let mut a = fixture(fixture_path);
        for view in [
            View::Skills,
            View::Configuration,
            View::Instructions,
            View::Mcp,
        ] {
            a.codex.view = view;
            for detailed in [false, true] {
                if detailed {
                    a.open_detail(den::presentation::Detail::Evidence);
                } else {
                    a.presentation.page = den::presentation::Page::Browse;
                }
                for (w, h) in [(80, 24), (120, 32), (160, 45)] {
                    for section in 0..6 {
                        a.codex.mcp.section = section;
                        a.codex.preview = true;
                        for full in [false, true] {
                            if full {
                                a.open_detail(if detailed {
                                    den::presentation::Detail::Evidence
                                } else {
                                    den::presentation::Detail::Overview
                                });
                            } else {
                                a.presentation.page = den::presentation::Page::Browse;
                            }
                            let s = draw(&mut a, w, h);
                            for secret in [
                                "MCP-SECRET-CANARY",
                                "PRIVATE-CONFIG-SENTINEL",
                                "PRIVATE-INSTRUCTION-SENTINEL",
                            ] {
                                assert!(!s.contains(secret));
                            }
                            assert!(!s.contains('\u{1b}'));
                        }
                    }
                }
            }
        }
        a.codex.view = View::Configuration;
        a.codex.sources = true;
        for n in 0..a.codex.rows().len() {
            a.codex.selected[1] = n;
            assert!(!draw(&mut a, 160, 45).contains("SECRET-CANARY"));
        }
    }
}
#[test]
fn focus_is_visible_without_color_and_uses_terminal_default_background() {
    use ratatui::style::{Color, Modifier};
    let mut a = brew();
    let mut t = Terminal::new(TestBackend::new(120, 32)).unwrap();
    t.draw(|f| ui::render(f, &mut a)).unwrap();
    assert!(
        t.backend()
            .buffer()
            .content
            .iter()
            .any(|c| c.modifier.contains(Modifier::REVERSED))
    );
    assert!(
        t.backend()
            .buffer()
            .content
            .iter()
            .all(|c| c.bg == Color::Reset && c.fg != Color::Cyan)
    );
    key(&mut a, KeyCode::Tab);
    assert!(draw(&mut a, 120, 32).contains("> Details"));
}

#[test]
fn escape_from_source_returns_to_inspected_mcp_without_losing_context() {
    let mut a = fixture("fixtures/mcp");
    a.codex.view = View::Mcp;
    a.codex.query = "docs".into();
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Char('e'));
    assert_eq!(a.codex.view, View::Configuration);
    assert!(a.inspecting());
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.codex.view, View::Mcp);
    assert_eq!(a.codex.query, "docs");
    assert!(a.inspecting());
    key(&mut a, KeyCode::Esc);
    assert!(!a.inspecting());
}

#[test]
fn searching_source_sections_does_not_jump_or_execute_commands() {
    let mut a = fixture("fixtures/mcp");
    a.codex.view = View::Mcp;
    a.codex.query = "docs".into();
    key(&mut a, KeyCode::Char('e'));
    assert!(a.inspecting());
    key(&mut a, KeyCode::Char('/'));
    assert!(a.presentation.menu_searching);
    for c in "Evidence".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.codex.view, View::Configuration);
    assert!(draw(&mut a, 80, 24).contains("Evidence"));
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.codex.view, View::Mcp);
    assert_eq!(a.codex.query, "docs");
}

#[test]
fn backspace_never_leaves_inventory_focused_behind_an_inspector() {
    let mut a = skills();
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Backspace);
    assert_eq!(a.active_focus(), Focus::Inventory);
    assert!(!a.inspecting());
    assert!(draw(&mut a, 80, 24).contains("Skills inventory"));
    key(&mut a, KeyCode::F(3));
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Backspace);
    assert_eq!(a.active_focus(), Focus::Inventory);
    assert!(!a.inspecting());
}
#[test]
fn file_back_first_returns_to_files_then_to_visible_inventory() {
    let mut a = skills();
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Char('3'));
    key(&mut a, KeyCode::Enter);
    assert!(a.skills.file_open);
    key(&mut a, KeyCode::Backspace);
    assert!(!a.skills.file_open);
    assert!(a.inspecting());
    key(&mut a, KeyCode::Backspace);
    assert!(a.inspecting());
    key(&mut a, KeyCode::Backspace);
    assert!(!a.inspecting());
    assert_eq!(a.skills.focus, Focus::Inventory);
}
#[test]
fn action_hints_follow_focus_and_help_remains_visible_at_small_sizes() {
    let mut a = brew();
    assert!(
        draw(&mut a, 80, 24)
            .lines()
            .last()
            .unwrap()
            .contains("Enter:inspect")
    );
    key(&mut a, KeyCode::Tab);
    let s = draw(&mut a, 80, 24);
    let footer = s.lines().last().unwrap();
    assert!(footer.contains("Enter:inspect"));
    key(&mut a, KeyCode::Char('2'));
    assert!(
        draw(&mut a, 80, 24)
            .lines()
            .last()
            .unwrap()
            .contains("Tab:sections")
    );
    for size in [(30, 10), (45, 20), (80, 24)] {
        let s = draw(&mut a, size.0, size.1);
        let footer = s.lines().last().unwrap();
        assert!(footer.contains("?:help") && footer.contains("q:quit"));
    }
    let mut a = skills();
    key(&mut a, KeyCode::Char('3'));
    assert!(
        draw(&mut a, 80, 24)
            .lines()
            .last()
            .unwrap()
            .contains("Tab:sections")
    );
}
