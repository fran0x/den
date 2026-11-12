// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::App,
    areas::homebrew::diagnostics::{Cause, Check, Health, Item, Report, Status, Tab, describe},
    areas::homebrew::graph::{Graph, Resolution},
    areas::homebrew::{self, CollectionError, CommandOutput},
    inventory::{InstallationIntent, Inventory},
    tui::ui,
};
use ratatui::{Terminal, backend::TestBackend};

fn inventory() -> Inventory {
    homebrew::parse(include_bytes!("../fixtures/brew-diagnostics.json")).unwrap()
}
fn fixture(name: &str) -> CommandOutput {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../fixtures/diagnostic-output.json")).unwrap();
    let v = &value[name];
    CommandOutput {
        code: v["code"].as_i64().map(|n| n as i32),
        stdout: v["stdout"].as_str().unwrap().as_bytes().to_vec(),
        stderr: v["stderr"].as_str().unwrap().as_bytes().to_vec(),
    }
}
fn report(check: Check, name: &str) -> Report {
    let i = inventory();
    Report::interpret(check, Ok(fixture(name)), &i, &Graph::build(&i), 1)
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::render(f, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
#[test]
fn every_resolution_cause_retains_exact_edge_and_provenance() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let health = Health::build(&graph);
    for cause in [
        Cause::AbsentRecorded,
        Cause::AbsentCurrent,
        Cause::AbsentOptional,
        Cause::AbsentBuild,
        Cause::AbsentTest,
        Cause::AmbiguousIdentity,
        Cause::IncompleteReference,
        Cause::TargetVersionAbsent,
        Cause::TargetVersionUnknown,
    ] {
        assert!(
            health.groups.keys().any(|(c, _)| *c == cause),
            "missing {cause:?}"
        );
    }
    assert_eq!(health.unresolved, 7);
    assert_eq!(health.keg_gaps, 2);
    assert_eq!(health.groups.values().map(Vec::len).sum::<usize>(), 9);
    for ((cause, evidence), edges) in &health.groups {
        for &index in edges {
            let edge = &graph.edges[index];
            assert_eq!(*evidence, edge.provenance.label());
            assert_eq!(inventory.resources[edge.from].name, "source");
            if cause.keg() {
                assert!(matches!(edge.target, Resolution::Installed(_)));
                assert!(graph.target_state(edge).is_none());
            }
        }
    }
}
#[test]
fn missing_historical_target_is_not_replaced_by_installed_version() {
    let i = inventory();
    let g = Graph::build(&i);
    let h = Health::build(&g);
    let &index = h
        .groups
        .iter()
        .find(|((c, _), _)| *c == Cause::TargetVersionAbsent)
        .unwrap()
        .1
        .first()
        .unwrap();
    assert_eq!(g.edges[index].target_version.as_deref(), Some("0"));
    assert!(g.target_state(&g.edges[index]).is_none());
    assert_eq!(
        g.versions(match g.edges[index].target {
            Resolution::Installed(n) => n,
            _ => panic!(),
        }),
        ["2"]
    );
}
#[test]
fn current_declaration_difference_is_explained_without_changing_graph_or_intent() {
    let mut app = App::new(inventory(), "Homebrew snapshot");
    key(&mut app, KeyCode::Char('5'));
    let e = app
        .graph
        .edges
        .iter()
        .position(|e| e.reference == "absent-current")
        .unwrap();
    let (_, text) = describe(
        Item::Edge(e),
        &app.diagnostics,
        &app.health,
        &app.inventory,
        &app.graph,
        &app.checks,
        app.inventory_epoch,
    );
    assert!(text.contains("not observed in the preferred keg's runtime records"));
    assert_eq!(
        app.inventory.resources[0]
            .homebrew
            .as_ref()
            .unwrap()
            .intent(),
        InstallationIntent::Unknown
    );
}
#[test]
fn observations_and_homebrew_leaves_are_separate_classifications() {
    let i = inventory();
    let g = Graph::build(&i);
    let h = Health::build(&g);
    let target = i.resources.iter().position(|r| r.name == "target").unwrap();
    assert!(!h.no_dependents.contains(&target)); // Includes unresolved target-keg edge at package level.
    let declared = i
        .resources
        .iter()
        .position(|r| r.name == "declared-only")
        .unwrap();
    assert!(h.no_recorded_dependents.contains(&declared));
    assert!(!h.no_current_dependents.contains(&declared));
    let r = report(Check::Leaves, "leaves");
    assert_eq!(r.count, Some(2));
    assert_eq!(r.status, Status::Informational);
    assert!(r.entries.iter().all(|e| e.package_id.is_some()));
}
#[test]
fn expected_missing_exit_one_produces_findings_and_unique_edge_correlation() {
    let r = report(Check::Missing, "missing");
    assert_eq!(r.status, Status::Confirmed);
    assert_eq!(r.count, Some(1));
    assert!(r.entries[0].package_id.is_some());
    assert!(r.entries[0].edge.is_some());
    assert_eq!(r.stdout, "source: absent-recorded\n");
    let r = report(Check::Missing, "missing_unmapped");
    assert_eq!(r.status, Status::Confirmed);
    assert!(r.entries[0].package_id.is_none());
}
#[test]
fn missing_failure_is_not_a_finding_or_a_successful_empty_check() {
    for name in ["failure", "missing_failure"] {
        let r = report(Check::Missing, name);
        assert_eq!(r.status, Status::Failed);
        assert_eq!(r.count, None);
        assert!(!r.stderr.is_empty());
    }
    let r = report(Check::Missing, "cleanup_empty");
    assert_eq!(r.count, Some(0));
}
#[test]
fn cleanup_candidates_require_exact_header_count_and_identity() {
    let r = report(Check::Cleanup, "cleanup_candidates");
    assert_eq!(r.status, Status::Confirmed);
    assert_eq!(r.count, Some(2));
    assert!(r.entries.iter().all(|e| e.package_id.is_some()));
    assert_eq!(
        r.entries[1].package_id.as_deref(),
        Some("brew:formula:alpha/tools/clash")
    );
    let i = inventory();
    let g = Graph::build(&i);
    let mut out = fixture("cleanup_candidates");
    out.stdout = b"Would autoremove 1 unneeded formula:\nclash\n".to_vec();
    let r = Report::interpret(Check::Cleanup, Ok(out), &i, &g, 0);
    assert_eq!(r.count, Some(1));
    assert!(r.entries[0].package_id.is_none());
}
#[test]
fn cleanup_empty_success_uses_required_wording_but_failure_never_does() {
    let r = report(Check::Cleanup, "cleanup_empty");
    assert_eq!(r.count, Some(0));
    assert_eq!(
        r.message,
        "Homebrew found no formulae eligible for automatic removal."
    );
    let r = report(Check::Cleanup, "failure");
    assert_eq!(r.status, Status::Failed);
    assert_eq!(r.count, None);
    assert!(r.message.contains("unavailable"));
}
#[test]
fn malformed_or_new_dry_run_format_retains_output_and_unknown_count() {
    let r = report(Check::Cleanup, "cleanup_unexpected");
    assert_eq!(r.status, Status::Review);
    assert_eq!(r.count, None);
    assert!(r.stdout.contains("New preview"));
    for text in [
        "Would autoremove 2 unneeded formulae:\nleaf\n",
        "Would autoremove 2 unneeded formulae:\nleaf\nleaf\n",
        "Would autoremove 1 unneeded formula:\nleaf\nextra\n",
    ] {
        let i = inventory();
        let r = Report::interpret(
            Check::Cleanup,
            Ok(CommandOutput {
                code: Some(0),
                stdout: text.as_bytes().to_vec(),
                stderr: vec![],
            }),
            &i,
            &Graph::build(&i),
            0,
        );
        assert_eq!(r.count, None);
    }
}
#[test]
fn execution_failures_limits_and_invalid_utf8_never_become_empty_results() {
    let i = inventory();
    let g = Graph::build(&i);
    for error in [
        CollectionError::Unavailable,
        CollectionError::Timeout,
        CollectionError::OutputLimit,
        CollectionError::Cancelled,
    ] {
        let r = Report::interpret(Check::Cleanup, Err(error), &i, &g, 0);
        assert_eq!(r.status, Status::Failed);
        assert_eq!(r.count, None);
    }
    let r = Report::interpret(
        Check::Cleanup,
        Ok(CommandOutput {
            code: Some(0),
            stdout: vec![0; 256 * 1024 + 1],
            stderr: vec![],
        }),
        &i,
        &g,
        0,
    );
    assert_eq!(r.status, Status::Failed);
    let r = Report::interpret(
        Check::Cleanup,
        Ok(CommandOutput {
            code: Some(0),
            stdout: vec![255],
            stderr: vec![],
        }),
        &i,
        &g,
        0,
    );
    assert_eq!(r.status, Status::Review);
    assert_eq!(r.count, None);
}
#[test]
fn snapshot_diagnostics_never_request_live_processes_and_unchecked_is_explicit() {
    let mut app = App::new(inventory(), "Homebrew snapshot");
    for tab in ['6', '7', '8'] {
        key(&mut app, KeyCode::Char(tab));
        key(&mut app, KeyCode::Char('c'));
        assert_eq!(app.check_requested, None);
        assert_eq!(app.begin_check(), None);
        assert!(draw(&mut app, 100, 30).contains("Not checked"));
    }
}
#[test]
fn diagnostic_group_jump_and_back_restore_position_even_with_no_search_matches() {
    let mut app = App::new(inventory(), "Homebrew snapshot");
    app.query = "no match".into();
    app.refresh();
    key(&mut app, KeyCode::Char('5'));
    key(&mut app, KeyCode::Enter);
    let saved = app.diagnostics.clone();
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.diagnostics.tab, None);
    assert_eq!(app.resource().unwrap().name, "source");
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.diagnostics.tab, Some(Tab::Unresolved));
    assert_eq!(app.diagnostics.group, saved.group);
    assert_eq!(app.query, "no match");
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.diagnostics.group, None);
}
#[test]
fn report_staleness_tracks_successful_refresh_and_failures_keep_matching_graph() {
    let mut app = App::new(inventory(), "Homebrew");
    app.live = true;
    key(&mut app, KeyCode::Char('8'));
    key(&mut app, KeyCode::Char('c'));
    assert_eq!(app.begin_check(), Some(Check::Cleanup));
    app.finish_check(Check::Cleanup, Ok(fixture("cleanup_empty")), 0);
    assert_eq!(app.inventory_epoch, 0);
    let graph = app.graph.clone();
    app.begin_collection();
    app.finish_collection(Err(CollectionError::Timeout));
    assert_eq!(app.graph, graph);
    assert_eq!(app.inventory_epoch, 0);
    assert!(draw(&mut app, 120, 35).contains("STALE"));
    app.begin_collection();
    app.finish_collection(Ok(inventory()));
    assert_eq!(app.inventory_epoch, 1);
    assert_eq!(app.checks[2].report.as_ref().unwrap().epoch, 0);
    assert!(draw(&mut app, 120, 35).contains("STALE"));
    app.finish_check(Check::Cleanup, Err(CollectionError::Timeout), 1);
    assert_eq!(app.checks[2].report.as_ref().unwrap().count, None);
}
#[test]
fn original_output_and_full_findings_are_sanitized_scrollable_and_narrow_safe() {
    let mut app = App::new(inventory(), "Homebrew");
    app.live = true;
    key(&mut app, KeyCode::Char('8'));
    let out = CommandOutput {
        code: Some(2),
        stdout: format!(
            "\x1b]52;c;secret\x07\u{202e}\n{}",
            "report line\n".repeat(100)
        )
        .into_bytes(),
        stderr: b"error\n".to_vec(),
    };
    app.finish_check(Check::Cleanup, Ok(out), 0);
    key(&mut app, KeyCode::Char('o'));
    for (w, h) in [(120, 35), (50, 20), (30, 10), (10, 5)] {
        let text = draw(&mut app, w, h);
        assert!(!text.contains('\x1b'));
        assert!(!text.contains('\u{202e}'));
    }
    draw(&mut app, 50, 20);
    key(&mut app, KeyCode::End);
    assert!(app.diagnostics.scroll > 0);
    key(&mut app, KeyCode::Backspace);
    assert!(!app.diagnostics.raw);
    key(&mut app, KeyCode::Char('5'));
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char(' '));
    draw(&mut app, 30, 10);
    key(&mut app, KeyCode::End);
    assert!(app.diagnostics.scroll > 0);
}
#[test]
fn pending_checks_do_not_block_keys_or_confuse_search_text() {
    let mut app = App::new(inventory(), "Homebrew");
    app.live = true;
    key(&mut app, KeyCode::Char('8'));
    key(&mut app, KeyCode::Char('c'));
    assert_eq!(app.begin_check(), Some(Check::Cleanup));
    key(&mut app, KeyCode::Char('6'));
    key(&mut app, KeyCode::Char('c'));
    assert_eq!(app.check_requested, None);
    key(&mut app, KeyCode::Char('/'));
    key(&mut app, KeyCode::Char('8'));
    key(&mut app, KeyCode::Char('c'));
    assert!(
        matches!(&app.presentation.page,den::tui::presentation::Page::Inspector{query,..} if query=="8c")
    );
    assert_eq!(app.check_requested, None);
}

#[test]
fn all_findings_remain_accessible_beyond_a_single_page() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(include_bytes!("../fixtures/brew-diagnostics.json")).unwrap();
    raw["formulae"][0]["build_dependencies"] = serde_json::json!(
        (0..1000)
            .map(|n| format!("missing-tool-{n}"))
            .collect::<Vec<_>>()
    );
    let mut app = App::new(
        homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap(),
        "Homebrew snapshot",
    );
    key(&mut app, KeyCode::Char('5'));
    app.diagnostics.selected = app
        .health
        .groups
        .keys()
        .position(|(c, _)| *c == Cause::AbsentBuild)
        .unwrap();
    key(&mut app, KeyCode::Enter);
    draw(&mut app, 60, 20);
    key(&mut app, KeyCode::End);
    assert_eq!(app.diagnostics.selected, 999);
    let screen = draw(&mut app, 60, 20);
    assert!(screen.contains("missing-tool-999"));
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.diagnostics.selected, 999);
}

#[test]
fn missing_package_mapping_does_not_choose_between_formula_and_cask() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(include_bytes!("../fixtures/brew-diagnostics.json")).unwrap();
    raw["casks"][0]["token"] = serde_json::json!("source");
    raw["casks"][0]["full_token"] = serde_json::json!("source");
    let i = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let r = Report::interpret(
        Check::Missing,
        Ok(fixture("missing")),
        &i,
        &Graph::build(&i),
        0,
    );
    assert_eq!(r.count, Some(1));
    assert!(r.entries[0].package_id.is_none());
    assert!(r.entries[0].edge.is_none());
}

#[test]
fn error_shaped_stdout_is_not_misclassified_as_missing_dependencies() {
    let i = inventory();
    let g = Graph::build(&i);
    for message in [
        "Error: cannot load formula\n",
        "Warning: incompatible metadata\n",
        "fatal: malformed record\n",
    ] {
        let r = Report::interpret(
            Check::Missing,
            Ok(CommandOutput {
                code: Some(1),
                stdout: message.as_bytes().to_vec(),
                stderr: vec![],
            }),
            &i,
            &g,
            0,
        );
        assert_eq!(r.status, Status::Failed);
        assert_eq!(r.count, None);
        assert_eq!(r.stdout, message);
    }
}
#[test]
fn empty_recorded_version_is_incomplete_metadata_not_a_historical_version() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(include_bytes!("../fixtures/brew-diagnostics.json")).unwrap();
    raw["formulae"][0]["installed"][0]["runtime_dependencies"][1]["version"] =
        serde_json::json!("");
    let i = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let h = Health::build(&Graph::build(&i));
    assert!(
        !h.groups
            .keys()
            .any(|(c, _)| *c == Cause::TargetVersionAbsent)
    );
    assert_eq!(
        h.groups
            .iter()
            .filter(|((c, _), _)| *c == Cause::TargetVersionUnknown)
            .map(|(_, v)| v.len())
            .sum::<usize>(),
        2
    );
}
