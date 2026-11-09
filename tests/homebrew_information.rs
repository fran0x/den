use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Focus},
    explorer::Explorer,
    graph::{Evidence, Graph},
    homebrew,
    inventory::Inventory,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};

fn inventory() -> Inventory {
    homebrew::parse(include_bytes!("../fixtures/brew-dependencies.json")).unwrap()
}
fn node(inventory: &Inventory, name: &str) -> usize {
    inventory
        .resources
        .iter()
        .position(|r| r.name == name)
        .unwrap()
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn browse(name: &str) -> App {
    let mut app = App::new(inventory(), "Homebrew snapshot");
    app.query = name.into();
    app.refresh();
    assert_eq!(app.resource().unwrap().name, name);
    app
}
fn information(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(frame, app)).unwrap();
    let screen: Vec<_> = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    if width < 100 {
        screen
            .into_iter()
            .skip_while(|line| !line.contains(" Information "))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        screen
            .iter()
            .map(|line| {
                line.chars()
                    .skip(width as usize * 45 / 100)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[test]
fn previews_open_two_levels_in_both_directions_with_exact_kegs() {
    let i = inventory();
    let g = Graph::build(&i);
    let mid = g.preferred(node(&i, "mid"));
    assert_eq!(mid.keg.as_deref(), Some("2"));
    let forward = Explorer::preview(&g, &i, mid.clone(), Evidence::Recorded, false, 32);
    assert!(
        forward
            .rows
            .iter()
            .any(|r| r.label.contains("leaf") && r.label.contains("@1"))
    );
    assert!(!forward.rows.iter().any(|r| r.label.contains("old")));
    let reverse = Explorer::preview(&g, &i, mid, Evidence::Recorded, true, 32);
    let wrong_keg = reverse
        .rows
        .iter()
        .find(|r| r.label.contains("root-old"))
        .unwrap();
    assert!(wrong_keg.label.contains("[other target keg]"));
    assert!(!wrong_keg.expandable);
    assert!(
        reverse
            .rows
            .iter()
            .any(|r| r.label.contains("root-a") && r.label.contains("@1"))
    );
    let reverse = Explorer::preview(
        &g,
        &i,
        g.preferred(node(&i, "leaf")),
        Evidence::Recorded,
        true,
        32,
    );
    let mid = reverse
        .rows
        .iter()
        .find(|r| r.label.contains("mid"))
        .unwrap();
    assert!(mid.label.contains("@2"));
    assert!(reverse.rows.iter().any(|r| r.label.contains("root-a")
        && r.branch.len() == 2
        && r.branch.starts_with(&mid.branch)));
    assert!(
        reverse
            .rows
            .iter()
            .all(|r| !r.label.contains("[+]") && !r.label.contains("[-]"))
    );
}

#[test]
fn uncertain_records_and_nonruntime_declarations_do_not_become_parent_links() {
    let i = inventory();
    let g = Graph::build(&i);
    let root = g.preferred(node(&i, "root-a"));
    for (evidence, labels) in [
        (
            Evidence::Recorded,
            vec![
                "[transitive record]",
                "[directness unknown]",
                "[keg unresolved]",
            ],
        ),
        (
            Evidence::Current,
            vec!["[optional]", "[build only]", "[test only]"],
        ),
    ] {
        let tree = Explorer::preview(&g, &i, root.clone(), evidence, false, 32);
        for label in labels {
            let leaf = tree.rows.iter().find(|r| r.label.contains(label)).unwrap();
            assert_eq!(leaf.branch.len(), 1);
            assert!(!leaf.expandable);
            assert!(
                !tree
                    .rows
                    .iter()
                    .any(|r| r.branch.len() > 1 && r.branch.starts_with(&leaf.branch))
            );
        }
        if evidence == Evidence::Recorded {
            assert!(!tree.rows.iter().any(|r| r.label.contains("declared-only")));
        } else {
            assert!(tree.rows.iter().any(|r| r.label.contains("declared-only")));
            assert!(
                !tree
                    .rows
                    .iter()
                    .any(|r| r.label.contains("[transitive record]"))
            );
        }
    }
    let unresolved = Explorer::preview(
        &g,
        &i,
        g.preferred(node(&i, "consumer")),
        Evidence::Recorded,
        false,
        32,
    );
    for label in ["[not installed]", "[ambiguous:"] {
        let row = unresolved
            .rows
            .iter()
            .find(|row| row.label.contains(label))
            .unwrap();
        assert!(!row.expandable && row.target.is_none());
    }
    let casks = Explorer::preview(
        &g,
        &i,
        g.preferred(node(&i, "leaf")),
        Evidence::Current,
        true,
        32,
    );
    assert!(
        casks
            .rows
            .iter()
            .any(|row| row.label.contains("studio [Cask]"))
    );
}

#[test]
fn previews_distinguish_cycles_from_shared_branches_and_report_limits() {
    let i = inventory();
    let g = Graph::build(&i);
    let root = g.preferred(node(&i, "root-a"));
    let shared = Explorer::preview(&g, &i, root.clone(), Evidence::Recorded, false, 32);
    assert_eq!(
        shared
            .rows
            .iter()
            .filter(|r| r.label.contains("leaf") && r.branch.len() == 2)
            .count(),
        2
    );
    assert!(!shared.rows.iter().any(|r| r.cycle));
    let limited = Explorer::preview(&g, &i, root, Evidence::Recorded, false, 2);
    assert_eq!(limited.rows.len(), 2);
    assert!(limited.limited);
    let mut incomplete = g.clone();
    incomplete.truncated = true;
    assert!(
        Explorer::preview(
            &incomplete,
            &i,
            incomplete.preferred(node(&i, "leaf")),
            Evidence::Recorded,
            false,
            32
        )
        .limited
    );
    for reverse in [false, true] {
        let cycle = Explorer::preview(
            &g,
            &i,
            g.preferred(node(&i, "cycle-a")),
            Evidence::Recorded,
            reverse,
            32,
        );
        assert_eq!(cycle.rows.len(), 2);
        assert!(cycle.rows[1].cycle);
        assert!(cycle.rows[1].label.contains("[cycle]"));
        assert!(!cycle.limited);
    }
    let mut raw = serde_json::json!({"formulae":[], "casks":[]});
    for n in 0..4 {
        raw["formulae"].as_array_mut().unwrap().push(serde_json::json!({"name":format!("n{n}"), "installed":[{"version":"1", "runtime_dependencies":[{"full_name":format!("n{}",n+1), "pkg_version":"1", "declared_directly":true}]}]}));
    }
    let i = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let g = Graph::build(&i);
    let depth = Explorer::preview(&g, &i, g.preferred(0), Evidence::Recorded, false, 32);
    assert_eq!(depth.rows.len(), 2);
    assert!(depth.rows[1].label.contains('…'));
    assert!(depth.limited);
}

#[test]
fn passive_panel_shows_both_trees_without_mutating_inspector_or_evidence() {
    let mut app = browse("mid");
    app.explorer.tree = false; // The panel must not depend on the inspector's direct-list preference.
    let before = app.explorer.clone();
    let graph = app.graph.clone();
    let inventory = app.inventory.clone();
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let text = information(&mut app, w, h);
        for label in [
            "Information",
            "Recorded runtime · keg 2",
            "Depends on",
            "Required by",
            "leaf",
            "root-a",
        ] {
            assert!(text.contains(label), "missing {label} at {w}: {text}");
        }
        assert!(!text.contains("> Information") && !text.contains("Space:toggle"));
        assert!(!text.contains("[+]") && !text.contains("[-]"));
        key(&mut app, KeyCode::Right);
        key(&mut app, KeyCode::Char(' '));
        assert_eq!(app.active_focus(), Focus::Inventory);
        assert!(!app.inspecting());
    }
    assert_eq!(app.explorer.installation, before.installation);
    assert_eq!(app.explorer.evidence, before.evidence);
    assert_eq!(app.explorer.expanded, before.expanded);
    assert_eq!(app.explorer.selected, before.selected);
    assert!(!app.explorer.tree);
    assert_eq!(app.graph, graph);
    assert_eq!(app.inventory, inventory);
    assert!(!app.refresh_requested && app.check_requested.is_none());
}

#[test]
fn selection_refresh_and_evidence_changes_update_passive_trees() {
    let mut app = browse("mid");
    assert!(information(&mut app, 120, 32).contains("root-a"));
    app.query.clear();
    app.refresh();
    app.selected = Some(node(&app.inventory, "root-a"));
    app.refresh();
    let recorded = information(&mut app, 160, 45);
    assert!(recorded.contains("[transitive record]"));
    assert!(!recorded.contains("declared-only"));
    app.explorer.evidence = Evidence::Current;
    let current = information(&mut app, 160, 45);
    assert!(current.contains("Current declarations · not historical"));
    for label in ["declared-only", "[optional]", "[build only]", "[test only]"] {
        assert!(current.contains(label));
    }
    assert!(!current.contains("[transitive record]"));
    let mut changed = app.inventory.clone();
    changed.resources.reverse();
    app.finish_collection(Ok(changed));
    assert_eq!(app.resource().unwrap().name, "root-a");
    assert!(information(&mut app, 160, 45).contains("declared-only"));
}

#[test]
fn unavailable_and_empty_evidence_are_distinct_and_hostile_labels_are_sanitized() {
    let i = homebrew::parse(include_bytes!("../fixtures/brew.json")).unwrap();
    let g = Graph::build(&i);
    let missing = Explorer::preview(
        &g,
        &i,
        g.preferred(node(&i, "ripgrep")),
        Evidence::Recorded,
        false,
        32,
    );
    assert!(missing.rows[0].label.contains("pcre2"));
    assert!(missing.rows[0].label.contains("[metadata unavailable]"));
    let mut cask = browse("studio");
    cask.explorer.evidence = Evidence::Recorded;
    let cask_text = information(&mut cask, 160, 45);
    assert!(cask_text.contains("Recorded runtime · metadata unavailable"));
    assert!(!cask_text.contains("linked keg unknown"));
    let mut app = browse("multi-unlinked");
    let unknown = information(&mut app, 160, 45);
    assert!(unknown.contains("linked keg unknown"));
    assert!(unknown.contains("linkage unavailable"));
    let mut app = browse("leaf");
    let empty = information(&mut app, 160, 45);
    assert!(empty.contains("No matching recorded relationships"));
    assert!(empty.contains("Required by") && empty.contains("mid"));
    let mut app = browse("root-current");
    app.explorer.evidence = Evidence::Recorded;
    assert!(information(&mut app, 160, 45).contains("Dependency metadata unavailable"));
    let mut i = inventory();
    i.resources[node(&inventory(), "leaf")].name = "bad\u{1b}]52;clipboard\u{7}\u{202e}name".into();
    let mut app = App::new(i, "Homebrew snapshot");
    app.query = "root-a".into();
    app.refresh();
    let text = information(&mut app, 160, 45);
    assert!(!text.contains('\u{1b}') && !text.contains('\u{7}') && !text.contains('\u{202e}'));
    for (w, h) in [(0, 0), (30, 10), (45, 20)] {
        information(&mut app, w, h);
    }
}
