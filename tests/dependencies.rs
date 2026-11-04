use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, CollectionStatus},
    explorer::{MAX_ROWS, Section},
    graph::{Evidence, Graph, Installation, MAX_DEPTH, MAX_EDGES, Provenance, Resolution},
    homebrew::{self, CollectionError},
    inventory::{HomebrewMetadata, InstallationIntent, Inventory, ResourceKind},
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
fn app(name: &str) -> App {
    let inventory = inventory();
    let selected = node(&inventory, name);
    let mut app = App::new(inventory, "Homebrew snapshot");
    app.selected = Some(selected);
    key(&mut app, KeyCode::Char('2'));
    app
}
fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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
fn provenance_directness_and_build_test_scopes_stay_separate() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let root = graph.preferred(node(&inventory, "root-a"));
    let direct = graph.adjacent(&root, Evidence::Recorded, false, false);
    assert!(
        direct
            .iter()
            .all(|&e| graph.edges[e].provenance == Provenance::RecordedDirect)
    );
    let all = graph.adjacent(&root, Evidence::Recorded, false, true);
    assert!(
        all.iter()
            .any(|&e| graph.edges[e].provenance == Provenance::RecordedTransitive)
    );
    assert!(
        all.iter()
            .any(|&e| graph.edges[e].provenance == Provenance::RecordedUnknown)
    );
    assert!(
        !all.iter()
            .any(|&e| graph.edges[e].reference == "build-tool")
    );
    let declarations = graph.adjacent(&root, Evidence::Current, false, false);
    for provenance in [
        Provenance::Required,
        Provenance::Recommended,
        Provenance::Optional,
        Provenance::Build,
        Provenance::Test,
    ] {
        assert!(
            declarations
                .iter()
                .any(|&e| graph.edges[e].provenance == provenance)
        );
    }
    let current = graph.traverse(root, Evidence::Current, false);
    assert!(
        current
            .nodes
            .iter()
            .any(|s| inventory.resources[s.node].name == "declared-only")
    );
    assert!(!current.nodes.iter().any(|s| matches!(
        inventory.resources[s.node].name.as_str(),
        "opt" | "build-tool" | "test-tool"
    )));
}

#[test]
fn every_resolved_edge_has_matching_forward_and_reverse_adjacency() {
    let graph = Graph::build(&inventory());
    for (index, edge) in graph.edges.iter().enumerate() {
        assert!(graph.forward[edge.from].contains(&index));
        if let Resolution::Installed(target) = edge.target {
            assert!(graph.reverse[target].contains(&index));
        } else {
            assert!(!graph.reverse.iter().any(|edges| edges.contains(&index)));
        }
    }
}

#[test]
fn linked_keg_is_preferred_and_other_versions_remain_independent() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let mid = node(&inventory, "mid");
    assert_eq!(graph.preferred(mid).keg.as_deref(), Some("2"));
    for (version, target) in [("1", "old"), ("2", "leaf")] {
        let edges = graph.adjacent(
            &Installation {
                node: mid,
                keg: Some(version.into()),
            },
            Evidence::Recorded,
            false,
            false,
        );
        assert_eq!(edges.len(), 1);
        assert_eq!(graph.edges[edges[0]].reference, target);
    }
    assert!(
        graph
            .preferred(node(&inventory, "multi-unlinked"))
            .keg
            .is_none()
    );
    let mut app = app("mid");
    assert!(app.explorer.rows[0].label.contains("leaf"));
    key(&mut app, KeyCode::Char('V'));
    assert!(app.explorer.rows[0].label.contains("old"));
    assert!(app.explorer.rows[0].detail.contains("source keg 1"));
}

#[test]
fn paths_follow_exact_kegs_and_do_not_mix_recorded_and_current_evidence() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let paths = graph.paths_to_requested(
        graph.preferred(node(&inventory, "leaf")),
        Evidence::Recorded,
    );
    let mut roots: Vec<_> = paths
        .paths
        .iter()
        .map(|p| inventory.resources[graph.edges[p[0]].from].name.as_str())
        .collect();
    roots.sort();
    assert_eq!(roots, ["root-a", "root-b"]);
    assert!(paths.paths.iter().all(|p| p.len() == 2));
    let old_paths =
        graph.paths_to_requested(graph.preferred(node(&inventory, "old")), Evidence::Recorded);
    assert_eq!(old_paths.paths.len(), 1);
    assert_eq!(
        inventory.resources[graph.edges[old_paths.paths[0][0]].from].name,
        "root-old"
    );
    let current =
        graph.paths_to_requested(graph.preferred(node(&inventory, "leaf")), Evidence::Current);
    assert!(
        current
            .paths
            .iter()
            .any(|p| inventory.resources[graph.edges[p[0]].from].name == "root-current")
    );
    assert!(
        current
            .paths
            .iter()
            .any(|p| inventory.resources[graph.edges[p[0]].from].name == "studio")
    );
}

#[test]
fn mismatched_target_versions_stop_paths_but_keep_the_relationship_visible() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let edge = graph
        .edges
        .iter()
        .find(|e| e.reference == "version-gap")
        .unwrap();
    assert!(matches!(edge.target, Resolution::Installed(_)));
    assert!(graph.target_state(edge).is_none());
    assert!(
        graph
            .paths_to_requested(
                graph.preferred(node(&inventory, "version-gap")),
                Evidence::Recorded
            )
            .paths
            .is_empty()
    );
    let app = app("root-a");
    assert!(
        app.explorer
            .rows
            .iter()
            .any(|r| r.detail.contains("target keg unresolved/not installed"))
    );
}

#[test]
fn aliases_taps_missing_and_ambiguous_references_resolve_conservatively() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let consumer = node(&inventory, "consumer");
    let edges: Vec<_> = graph.forward[consumer]
        .iter()
        .map(|&e| &graph.edges[e])
        .collect();
    assert_eq!(
        edges
            .iter()
            .filter(
                |e| e.target == Resolution::Installed(node(&inventory, "tool@2"))
                    && e.provenance == Provenance::RecordedDirect
            )
            .count(),
        1
    );
    // Check each alias independently, because equivalent resolved edges deduplicate.
    for alias in ["example/tools/tool", "example/tools/old-tool"] {
        let mut changed = inventory.clone();
        if let Some(HomebrewMetadata::Formula { installed, .. }) =
            &mut changed.resources[consumer].homebrew
        {
            installed[0].runtime_dependencies = Some(vec![
                serde_json::json!({"full_name":alias,"declared_directly":true}),
            ]);
        }
        let graph = Graph::build(&changed);
        assert_eq!(
            graph.edges[graph.forward[consumer][0]].target,
            Resolution::Installed(node(&inventory, "tool@2"))
        );
    }
    assert_eq!(
        edges
            .iter()
            .find(|e| e.reference == "absent")
            .unwrap()
            .target,
        Resolution::Missing
    );
    assert!(
        matches!(&edges.iter().find(|e| e.reference == "clash").unwrap().target, Resolution::Ambiguous(nodes) if nodes.len() == 2)
    );
    assert_eq!(graph.ids.len(), inventory.resources.len());
    let malformed = node(&inventory, "malformed");
    assert!(
        graph.forward[malformed]
            .iter()
            .all(|&e| graph.edges[e].target == Resolution::Invalid)
    );
}

#[test]
fn cask_edges_keep_types_and_ignore_platform_constraints() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let studio = graph.preferred(node(&inventory, "studio"));
    assert!(
        graph
            .adjacent(&studio, Evidence::Recorded, false, true)
            .is_empty()
    );
    let edges = graph.adjacent(&studio, Evidence::Current, false, false);
    assert_eq!(edges.len(), 3);
    for index in edges {
        let edge = &graph.edges[index];
        let Resolution::Installed(target) = edge.target else {
            panic!()
        };
        assert_eq!(inventory.resources[target].kind, edge.kind);
        if edge.reference == "same" {
            assert_eq!(edge.kind, ResourceKind::Formula);
        }
        if edge.reference == "helper" {
            assert_eq!(edge.kind, ResourceKind::Cask);
        }
    }
    assert!(graph.forward[node(&inventory, "platform-only")].is_empty());
}

#[test]
fn absent_runtime_metadata_defaults_to_labeled_current_declarations() {
    for name in ["studio", "root-current"] {
        let mut app = app(name);
        assert_eq!(app.explorer.evidence, Evidence::Current);
        assert!(app.explorer.note.contains("not historical"));
        assert!(!app.explorer.rows.is_empty());
        key(&mut app, KeyCode::Char('s'));
        assert_eq!(app.explorer.evidence, Evidence::Recorded);
        assert!(app.explorer.rows.is_empty());
        assert!(app.explorer.note.contains("unavailable"));
    }
}

#[test]
fn duplicates_are_deduplicated_without_merging_sources_or_kegs() {
    let mut inventory = inventory();
    if let Some(HomebrewMetadata::Formula { installed, .. }) = &mut inventory.resources[0].homebrew
    {
        let deps = installed[0].runtime_dependencies.as_mut().unwrap();
        deps.push(deps[0].clone());
    }
    let graph = Graph::build(&inventory);
    let matches: Vec<_> = graph
        .edges
        .iter()
        .filter(|e| e.from == 0 && e.reference == "mid")
        .collect();
    assert_eq!(matches.len(), 2); // recorded + current, not one indistinguishable edge
}

#[test]
fn cycles_terminate_and_shared_dependencies_are_not_cycles() {
    let inventory = inventory();
    let graph = Graph::build(&inventory);
    let cycle = graph.traverse(
        graph.preferred(node(&inventory, "cycle-a")),
        Evidence::Recorded,
        false,
    );
    assert!(cycle.cycle);
    assert_eq!(cycle.nodes.len(), 2);
    let shared = graph.traverse(
        graph.preferred(node(&inventory, "root-a")),
        Evidence::Recorded,
        false,
    );
    assert!(!shared.cycle);
    assert_eq!(
        shared
            .nodes
            .iter()
            .filter(|s| inventory.resources[s.node].name == "leaf")
            .count(),
        1
    );
    let mut app = app("cycle-a");
    key(&mut app, KeyCode::Char('t'));
    key(&mut app, KeyCode::Char(' '));
    assert!(app.explorer.rows.iter().any(|r| r.cycle && !r.expandable));
}

#[test]
fn tree_navigation_jump_and_back_restore_filters_and_selected_row() {
    let mut app = app("root-a");
    app.query = "root-a".into();
    app.refresh();
    key(&mut app, KeyCode::Char('t'));
    let index = app
        .explorer
        .rows
        .iter()
        .position(|r| r.label.contains("mid"))
        .unwrap();
    app.explorer.selected = index;
    key(&mut app, KeyCode::Char(' '));
    assert!(
        app.explorer
            .rows
            .iter()
            .any(|r| r.branch.len() == 2 && r.label.contains("leaf"))
    );
    key(&mut app, KeyCode::Char(' '));
    assert!(!app.explorer.rows.iter().any(|r| r.branch.len() == 2));
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.resource().unwrap().name, "mid");
    assert!(app.query.is_empty());
    assert_eq!(app.explorer.section, Section::Overview);
    key(&mut app, KeyCode::Backspace);
    assert_eq!(app.resource().unwrap().name, "root-a");
    assert_eq!(app.query, "root-a");
    assert_eq!(app.explorer.selected, index);
    assert!(app.explorer.tree);
}

#[test]
fn failed_refresh_preserves_matching_graph_and_success_rebuilds_it() {
    let mut app = app("leaf");
    let graph = app.graph.clone();
    let inventory = app.inventory.clone();
    app.finish_collection(Err(CollectionError::Timeout));
    assert_eq!(app.graph, graph);
    assert_eq!(app.inventory, inventory);
    assert!(matches!(
        app.collection_status,
        CollectionStatus::Failed { stale: true, .. }
    ));
    let mut changed = inventory;
    changed.resources.reverse();
    app.finish_collection(Ok(changed));
    assert_eq!(
        app.graph.ids,
        app.inventory
            .resources
            .iter()
            .map(|r| r.id.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(app.resource().unwrap().name, "leaf");
    assert_eq!(app.graph, Graph::build(&app.inventory));
}

#[test]
fn graph_does_not_modify_intent_and_no_path_does_not_mean_unused() {
    let mut app = app("cycle-a");
    let before = app.inventory.clone();
    key(&mut app, KeyCode::Char('4'));
    assert!(
        app.explorer
            .note
            .contains("does not imply unused or safe to remove")
    );
    assert_eq!(app.inventory, before);
    assert_eq!(
        app.resource().unwrap().homebrew.as_ref().unwrap().intent(),
        InstallationIntent::Unknown
    );
}

#[test]
fn dependency_rendering_is_safe_and_handles_narrow_and_empty_panes() {
    let mut app = app("mid");
    assert!(draw(&mut app, 120, 30).contains("recorded direct runtime"));
    key(&mut app, KeyCode::Char('3'));
    assert!(app.explorer.rows.iter().any(|r| r.label.contains("root-a")));
    key(&mut app, KeyCode::Char('s'));
    assert!(draw(&mut app, 120, 30).contains("Current declarations"));
    app.explorer.note = "\u{1b}]52;c;test\u{7}\u{202e}".into();
    if let Some(row) = app.explorer.rows.first_mut() {
        row.label = app.explorer.note.clone();
        row.detail = app.explorer.note.clone();
    }
    let rendered = draw(&mut app, 120, 30);
    assert!(!rendered.chars().any(char::is_control));
    assert!(!rendered.contains('\u{202e}'));
    for (w, h) in [(45, 30), (30, 10), (1, 1), (0, 0)] {
        draw(&mut app, w, h);
    }
    let mut empty = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "Homebrew snapshot",
    );
    for section in ['2', '3', '4'] {
        key(&mut empty, KeyCode::Char(section));
        draw(&mut empty, 45, 20);
    }
}

#[test]
fn graph_and_projection_limits_are_explicit() {
    let mut inventory = inventory();
    inventory.resources.truncate(1);
    let template = inventory.resources[0].clone();
    inventory.resources.clear();
    for index in 0..(MAX_EDGES / 2048 + 1) {
        let mut resource = template.clone();
        resource.id = format!("brew:formula:node-{index}");
        resource.name = format!("node-{index}");
        if let Some(HomebrewMetadata::Formula {
            installed,
            dependencies,
            aliases,
            ..
        }) = &mut resource.homebrew
        {
            installed[0].runtime_dependencies = Some((0..2048).map(|i| serde_json::json!({"full_name":format!("missing-{i}"),"declared_directly":true})).collect());
            dependencies.clear();
            aliases.clear();
        }
        inventory.resources.push(resource);
    }
    let graph = Graph::build(&inventory);
    assert!(graph.truncated);
    assert_eq!(graph.edges.len(), MAX_EDGES);
    let mut app = App::new(inventory, "Homebrew snapshot");
    key(&mut app, KeyCode::Char('2'));
    assert!(app.explorer.limited);
    assert_eq!(app.explorer.rows.len(), MAX_ROWS);
    assert!(draw(&mut app, 100, 30).contains("PARTIAL RESULTS"));
}

#[test]
fn traversal_depth_is_bounded() {
    let mut raw = serde_json::json!({"formulae":[],"casks":[]});
    for i in 0..MAX_DEPTH + 3 {
        raw["formulae"].as_array_mut().unwrap().push(serde_json::json!({"name":format!("node-{i}"),"installed":[{"version":"1","installed_on_request":i==0,"runtime_dependencies":[{"full_name":format!("node-{}",i+1),"pkg_version":"1","declared_directly":true}]}]}));
    }
    let inventory = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    let graph = Graph::build(&inventory);
    let traversal = graph.traverse(graph.preferred(0), Evidence::Recorded, false);
    assert!(traversal.truncated);
    assert_eq!(traversal.nodes.len(), MAX_DEPTH + 1);
    assert!(
        graph
            .paths_to_requested(graph.preferred(MAX_DEPTH + 2), Evidence::Recorded)
            .truncated
    );
}
