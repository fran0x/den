use den::{app::App, codex, homebrew, ui};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

fn brew() -> App {
    let mut a = App::new(
        homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap(),
        "Homebrew snapshot",
    );
    // These tests cover navigation over the full fixture; Tools is covered in homebrew_filters.
    a.show_all = true;
    a.refresh();
    a
}
fn skills() -> App {
    let mut a = brew();
    let context = codex::Context::fixture(Path::new("fixtures/codex"), None).unwrap();
    a.codex.configure(context.clone());
    a.codex.finish(
        codex::discover(&context, &AtomicBool::new(false)),
        &mut a.skills,
    );
    a.codex.requested = false;
    a.navigate(Destination::Skills);
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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{app::Focus, presentation::Destination};
/// Supporting file count of the selected skill.
fn files(a: &App) -> usize {
    let sources = a.agent_sources();
    let row = a.agents.selected_row(&sources).unwrap();
    den::agents::skill(&sources, &row).unwrap().files.len()
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn fixture(path: &str) -> App {
    let mut a = brew();
    let context = codex::Context::fixture(Path::new(path), None).unwrap();
    a.codex.configure(context.clone());
    a.codex.finish(
        codex::discover(&context, &AtomicBool::new(false)),
        &mut a.skills,
    );
    a.codex.requested = false;
    a.navigate(Destination::Skills);
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
        assert!(s.contains("[requested-app]") && !s.contains("[Cask]"));
        assert!(!s.contains("on_request:"));
        assert!(!s.contains("brew:formula:"));
        assert!(a.page_size >= if w < 100 { 7 } else { 19 });
    }
}
#[test]
fn skills_rows_show_each_tools_status_without_technical_noise() {
    let mut a = skills();
    let s = draw(&mut a, 120, 32);
    let row = s.lines().find(|s| s.contains("> example")).unwrap();
    // Tools, then one status (Claude Code is not configured in this fixture).
    assert!(row.contains("Codex only") && row.contains("Ready"));
    assert!(!s.contains("config not evaluated"));
    assert!(s.contains("Synthetic skill for configuration"));
    // The panel says, per tool, the status and where the skill lives.
    assert!(s.contains("Codex: Ready · Eligible: Valid skill in a documented location"));
    assert!(s.contains("Where: User · Your skills folder"));
    // Evidence is further down the same panel; there is no inspector.
    let s = draw(&mut a, 160, 45);
    assert!(s.contains("Evidence") && s.contains("Resolved:"));
}
#[test]
fn enter_and_backspace_open_nothing_and_the_inventory_stays_visible() {
    let mut a = skills();
    for code in [
        KeyCode::Enter,
        KeyCode::Char('v'),
        KeyCode::Char('z'),
        KeyCode::Backspace,
    ] {
        key(&mut a, code);
        assert!(!a.inspecting());
        assert_eq!(a.active_focus(), Focus::Inventory);
        let s = draw(&mut a, 80, 24);
        assert!(s.contains("> Skills ") && s.contains("Information"));
    }
    key(&mut a, KeyCode::F(3));
    key(&mut a, KeyCode::Enter);
    assert!(!a.inspecting());
}
#[test]
fn panel_scrolls_with_page_keys_and_keeps_its_place_across_help_and_switches() {
    let mut a = skills();
    a.skills.inventory.skills[0].instructions = (0..160)
        .map(|n| format!("Synthetic instruction line {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(draw(&mut a, 80, 24).contains("PgDn for more"));
    key(&mut a, KeyCode::PageDown);
    draw(&mut a, 80, 24);
    key(&mut a, KeyCode::PageDown);
    let offset = a.agents.panel.offset;
    assert!(offset > 0);
    assert!(draw(&mut a, 80, 24).contains("PgUp/PgDn to scroll"));
    key(&mut a, KeyCode::Char('?'));
    draw(&mut a, 80, 24);
    key(&mut a, KeyCode::End);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.agents.panel.offset, offset);
    for _ in 0..5 {
        key(&mut a, KeyCode::F(2)); // Cloud, Docker, Toolchains, Homebrew, back to Agents
    }
    for _ in 0..6 {
        key(&mut a, KeyCode::F(3)); // Every Agents section, back to Skills.
    }
    assert_eq!(a.destination(), Destination::Skills);
    assert_eq!(a.agents.panel.offset, offset);
    key(&mut a, KeyCode::PageUp);
    key(&mut a, KeyCode::PageUp);
    assert_eq!(a.agents.panel.offset, 0);
}
#[test]
fn long_unicode_names_and_paths_are_truncated_without_controls_or_panics() {
    let mut a = brew();
    a.inventory.resources[0].name = "長い名前".repeat(50);
    a.inventory.resources[0].description = "é\u{1b}[31m\u{202e}".repeat(60);
    a.inventory.resources[0].path = Some("/synthetic/very-long/".repeat(150));
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let text = draw(&mut a, w, h);
        assert!(text.contains('…'));
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\u{202e}'));
    }
}
#[test]
fn empty_and_failed_views_remain_explainable() {
    let mut a = fixture("fixtures/mcp-empty");
    a.navigate(Destination::Mcp);
    assert!(draw(&mut a, 120, 32).contains("No MCP servers found."));
    a.codex.error = Some("Codex collection failed; previous inventory retained (stale)".into());
    assert!(draw(&mut a, 80, 24).contains("Codex collection failed"));
}
#[test]
fn every_agents_section_panel_page_and_source_keeps_secrets_redacted() {
    for fixture_path in ["fixtures/mcp", "fixtures/codex"] {
        let mut a = fixture(fixture_path);
        for destination in [
            Destination::Skills,
            Destination::Settings,
            Destination::Sources,
            Destination::Instructions,
            Destination::Mcp,
        ] {
            a.navigate(destination);
            for (w, h) in [(80, 24), (120, 32), (160, 45)] {
                let rows = a.agents.rows(&a.agent_sources()).len();
                for _ in 0..rows {
                    // Every page of the panel, for every row and both instruction scenarios.
                    for conditional in [false, true] {
                        a.codex.conditional = conditional;
                        a.agents.panel.offset = 0;
                        loop {
                            let s = draw(&mut a, w, h);
                            for secret in [
                                "MCP-SECRET-CANARY",
                                "PRIVATE-CONFIG-SENTINEL",
                                "PRIVATE-INSTRUCTION-SENTINEL",
                            ] {
                                assert!(!s.contains(secret), "{destination:?} {w}: {secret}");
                            }
                            assert!(!s.contains('\u{1b}'));
                            if a.agents.panel.offset >= a.agents.panel.max {
                                break;
                            }
                            key(&mut a, KeyCode::PageDown);
                        }
                    }
                    key(&mut a, KeyCode::Char('V'));
                    key(&mut a, KeyCode::Down);
                }
                key(&mut a, KeyCode::Home);
            }
        }
    }
}
#[test]
fn only_the_selected_row_leaves_the_terminal_default_background() {
    // Reverse video without colour is covered by the theme's unit tests.
    use ratatui::style::{Color, Modifier};
    let mut a = brew();
    let mut t = Terminal::new(TestBackend::new(120, 32)).unwrap();
    t.draw(|f| ui::render(f, &mut a)).unwrap();
    let buffer = t.backend().buffer();
    let width = buffer.area.width as usize;
    let selected: Vec<_> = buffer
        .content
        .iter()
        .enumerate()
        .filter(|(_, c)| c.bg != Color::Reset)
        .map(|(i, _)| i / width)
        // The top bar's active area is the one other highlighted span.
        .filter(|&y| y > 0)
        .collect();
    let row = selected.first().copied().expect("a selected row");
    assert!(selected.iter().all(|&y| y == row));
    let line: String = (0..width)
        .map(|x| buffer.content[row * width + x].symbol())
        .collect();
    assert!(line.contains("> "), "{line}");
    assert!(
        buffer.content[row * width + 2]
            .modifier
            .contains(Modifier::BOLD)
    );
    // The inventory is the one focused pane, marked with ">".
    let mut a = skills();
    assert!(draw(&mut a, 120, 32).contains("> Skills "));
}
#[test]
fn supporting_files_are_listed_in_the_panel() {
    let mut a = skills();
    assert!(files(&a) > 0);
    let screen = draw(&mut a, 160, 45);
    assert!(screen.contains("Supporting files ("));
    key(&mut a, KeyCode::Char('o')); // Not a key any more.
    assert_eq!(draw(&mut a, 160, 45), screen);
}
#[test]
fn action_hints_follow_focus_and_help_remains_visible_at_small_sizes() {
    let mut a = brew();
    // Packages have no inspector, so the footer lists their keys instead of Enter.
    let footer = draw(&mut a, 80, 24).lines().last().unwrap().to_owned();
    assert!(footer.contains("a:tools/all") && !footer.contains("f:type"));
    assert!(!footer.contains("Enter:inspect"));
    key(&mut a, KeyCode::Right);
    assert_eq!(draw(&mut a, 80, 24).lines().last().unwrap(), footer);
    for size in [(30, 10), (45, 20), (80, 24)] {
        let s = draw(&mut a, size.0, size.1);
        let footer = s.lines().last().unwrap();
        assert!(footer.contains("?:help") && footer.contains("q:quit"));
    }
    let mut a = skills();
    let footer = draw(&mut a, 80, 24).lines().last().unwrap().to_owned();
    assert!(!footer.contains("e:source") && !footer.contains("Enter") && !footer.contains("f/i"));
}
