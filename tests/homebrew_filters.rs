use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, CollectionStatus, Focus, HomebrewCounts},
    graph::Evidence,
    homebrew::{self, CollectionError},
    inventory::{InstallationIntent as Intent, Inventory, ResourceKind},
    presentation::Detail,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};

fn inventory() -> Inventory {
    homebrew::parse(include_bytes!("../fixtures/brew.json")).unwrap()
}
fn app() -> App {
    App::new(inventory(), "Homebrew snapshot")
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn names(app: &App) -> Vec<&str> {
    app.visible
        .iter()
        .map(|&n| app.inventory.resources[n].name.as_str())
        .collect()
}
fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::render(f, app)).unwrap();
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
fn counts_use_search_and_type_scope_without_collapsing_unknown_intent() {
    let mut a = app();
    let all = HomebrewCounts {
        all: 7,
        requested: 2,
        dependency: 1,
        unknown: 4,
        used_by_others: 1,
    };
    assert_eq!(a.homebrew_counts(), all);
    key(&mut a, KeyCode::Char('u'));
    key(&mut a, KeyCode::Char('i'));
    assert!(a.visible.is_empty()); // Requested and recorded-used do not overlap in this fixture.
    assert_eq!(a.homebrew_counts(), all); // Facet counts do not disappear with the selected filters.
    a.filter = Some(ResourceKind::Formula);
    a.refresh();
    assert_eq!(
        a.homebrew_counts(),
        HomebrewCounts {
            all: 5,
            requested: 1,
            dependency: 1,
            unknown: 3,
            used_by_others: 1
        }
    );
    a.filter = Some(ResourceKind::Cask);
    a.refresh();
    assert_eq!(
        a.homebrew_counts(),
        HomebrewCounts {
            all: 2,
            requested: 1,
            dependency: 0,
            unknown: 1,
            used_by_others: 0
        }
    );
    a.filter = None;
    a.query = "widget".into();
    a.refresh();
    assert_eq!(
        a.homebrew_counts(),
        HomebrewCounts {
            all: 2,
            requested: 0,
            dependency: 0,
            unknown: 2,
            used_by_others: 0
        }
    );
}

fn evidence_fixture() -> Inventory {
    let mut raw = serde_json::json!({"formulae":[
        {"name":"manual","installed":[{"version":"1","installed_on_request":true,"runtime_dependencies":[]}]},
        {"name":"dep","installed":[{"version":"1","installed_as_dependency":true,"runtime_dependencies":[]}]},
        {"name":"unknown","installed":[{"version":"1","runtime_dependencies":[]}]},
        {"name":"false-only","installed":[{"version":"1","installed_on_request":false,"runtime_dependencies":[]}]},
        {"name":"conflict","installed":[{"version":"1","installed_on_request":true,"installed_as_dependency":true,"runtime_dependencies":[]}]},
        {"name":"mixed","installed":[{"version":"1","installed_on_request":true,"runtime_dependencies":[]},{"version":"2","installed_as_dependency":true,"runtime_dependencies":[]}]},
        {"name":"current-only","installed":[{"version":"1","runtime_dependencies":[]}]},
        {"name":"wrong-keg","installed":[{"version":"2","runtime_dependencies":[]}]},
        {"name":"unresolved","installed":[{"version":"1","runtime_dependencies":[]},{"version":"2","runtime_dependencies":[]}]},
        {"name":"self-only","installed":[{"version":"1","runtime_dependencies":[{"full_name":"self-only","pkg_version":"1","declared_directly":true}]}]},
        {"name":"consumer","installed":[{"version":"1","installed_on_request":true,"runtime_dependencies":[]}],"dependencies":["current-only"],"optional_dependencies":["conflict"],"build_dependencies":["conflict"]}
    ],"casks":[]});
    raw["formulae"][10]["installed"][0]["runtime_dependencies"] = serde_json::json!([
        {"full_name":"manual","pkg_version":"1","declared_directly":true},
        {"full_name":"manual","pkg_version":"1","declared_directly":false},
        {"full_name":"dep","pkg_version":"1","declared_directly":true},
        {"full_name":"unknown","pkg_version":"1","declared_directly":false},
        {"full_name":"false-only","pkg_version":"1"},
        {"full_name":"mixed","pkg_version":"1","declared_directly":true},
        {"full_name":"wrong-keg","pkg_version":"1","declared_directly":true},
        {"full_name":"unresolved","declared_directly":true},
        {"full_name":"not-installed","pkg_version":"1","declared_directly":true}
    ]);
    homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap()
}

#[test]
fn usage_is_positive_recorded_evidence_and_overlaps_all_three_intents() {
    let mut a = App::new(evidence_fixture(), "Homebrew snapshot");
    let before: Vec<_> = a.inventory.resources.iter().map(|r| r.intent()).collect();
    let graph = a.graph.clone();
    assert_eq!(
        a.homebrew_counts(),
        HomebrewCounts {
            all: 11,
            requested: 2,
            dependency: 1,
            unknown: 8,
            used_by_others: 5
        }
    );
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(
        names(&a),
        ["manual", "dep", "unknown", "false-only", "mixed"]
    );
    key(&mut a, KeyCode::Char('i'));
    assert_eq!(names(&a), ["manual"]); // Explicitly installed can also be used by others.
    key(&mut a, KeyCode::Char('i'));
    assert_eq!(names(&a), ["dep"]);
    key(&mut a, KeyCode::Char('i'));
    assert_eq!(names(&a), ["unknown", "false-only", "mixed"]);
    a.explorer.evidence = Evidence::Current;
    a.refresh();
    assert_eq!(names(&a), ["unknown", "false-only", "mixed"]); // Inspector evidence does not redefine the filter.
    a.graph.truncated = true;
    assert_eq!(a.homebrew_counts().used_by_others, 5);
    assert!(draw(&mut a, 120, 32).contains("partial"));
    a.graph.truncated = false;
    assert_eq!(a.graph, graph);
    assert_eq!(
        a.inventory
            .resources
            .iter()
            .map(|r| r.intent())
            .collect::<Vec<_>>(),
        before
    );
    let ambiguous = homebrew::parse(include_bytes!("../fixtures/brew-dependencies.json")).unwrap();
    let a = App::new(ambiguous, "Homebrew snapshot");
    let candidates: Vec<_> = a
        .inventory
        .resources
        .iter()
        .enumerate()
        .filter(|(_, r)| r.name == "clash")
        .collect();
    assert_eq!(candidates.len(), 2);
    assert!(
        candidates
            .iter()
            .all(|(node, _)| !a.graph.has_recorded_users(*node))
    );
}

#[test]
fn type_intent_usage_and_search_filters_intersect_and_u_stays_text_during_search() {
    let mut a = app();
    key(&mut a, KeyCode::Char('f'));
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(names(&a), ["pcre2"]);
    key(&mut a, KeyCode::Char('i'));
    assert!(a.visible.is_empty());
    key(&mut a, KeyCode::Char('i'));
    assert_eq!(names(&a), ["pcre2"]);
    key(&mut a, KeyCode::Char('/'));
    for c in "pcre2u".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    assert_eq!(a.query, "pcre2u");
    assert!(a.used_by_others && a.visible.is_empty());
    a.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert!(a.query.is_empty() && a.used_by_others);
    key(&mut a, KeyCode::Enter);
    assert_eq!(names(&a), ["pcre2"]);
    key(&mut a, KeyCode::Char('u'));
    assert!(!a.used_by_others);
    assert_eq!(names(&a), ["pcre2"]); // Intent still restricts the inventory.
}

#[test]
fn usage_filter_survives_area_changes_and_back_from_a_hidden_related_package() {
    let mut a = app();
    key(&mut a, KeyCode::Char('u'));
    key(&mut a, KeyCode::Char('i'));
    key(&mut a, KeyCode::Char('i'));
    a.query = "pcre2".into();
    a.refresh();
    key(&mut a, KeyCode::Tab);
    key(&mut a, KeyCode::Tab);
    assert!(a.used_by_others);
    assert_eq!(a.intent_filter, Some(Intent::Dependency));
    assert_eq!(a.query, "pcre2");
    key(&mut a, KeyCode::Enter);
    a.explorer.evidence = Evidence::Recorded;
    a.open_detail(Detail::RequiredBy);
    let ripgrep = a
        .explorer
        .rows
        .iter()
        .position(|r| r.label.contains("ripgrep"))
        .unwrap();
    a.explorer.selected = ripgrep;
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.resource().unwrap().name, "ripgrep");
    assert!(!a.used_by_others && a.intent_filter.is_none() && a.query.is_empty());
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.resource().unwrap().name, "pcre2");
    assert!(a.used_by_others);
    assert_eq!(a.intent_filter, Some(Intent::Dependency));
    assert_eq!(a.query, "pcre2");
    assert_eq!(a.explorer.selected, ripgrep);
}

#[test]
fn refresh_recomputes_usage_and_failure_retains_matching_counts_and_selection() {
    let mut a = App::live_homebrew();
    a.begin_collection();
    a.finish_collection(Ok(inventory()));
    key(&mut a, KeyCode::Char('u'));
    let selected = a.resource().unwrap().id.clone();
    let counts = a.homebrew_counts();
    a.begin_collection();
    a.finish_collection(Err(CollectionError::Timeout));
    assert_eq!(a.resource().unwrap().id, selected);
    assert_eq!(a.homebrew_counts(), counts);
    assert!(matches!(
        a.collection_status,
        CollectionStatus::Failed { stale: true, .. }
    ));
    let mut reordered = inventory();
    reordered.resources.reverse();
    a.finish_collection(Ok(reordered));
    assert_eq!(a.resource().unwrap().id, selected);
    let mut raw: serde_json::Value =
        serde_json::from_slice(include_bytes!("../fixtures/brew.json")).unwrap();
    raw["formulae"][0]["installed"][0]["runtime_dependencies"] = serde_json::json!([]);
    a.finish_collection(Ok(
        homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap()
    ));
    assert!(a.used_by_others && a.visible.is_empty());
    assert_eq!(
        a.homebrew_counts(),
        HomebrewCounts {
            used_by_others: 0,
            ..counts
        }
    );
    let text = draw(&mut a, 120, 32);
    assert!(text.contains("No matches"));
    assert!(text.contains("Used by others 0 [ON]"));
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(a.visible.len(), 7);
}

#[test]
fn counts_active_filters_and_per_row_intent_are_visible_at_supported_sizes() {
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let mut a = app();
        let screen = draw(&mut a, w, h);
        let inventory = screen
            .lines()
            .map(|line| {
                if w >= 100 {
                    line.chars().take(w as usize * 45 / 100).collect::<String>()
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let normalized = inventory.split_whitespace().collect::<Vec<_>>().join(" ");
        for label in [
            "[All 7]",
            "Explicitly requested 2",
            "Dependencies 1",
            "Unknown 4",
            "Used by others 1 [off]",
            "recorded runtime",
        ] {
            assert!(normalized.contains(label), "{w}: missing {label}: {screen}");
        }
        for (name, intent) in [
            ("ripgrep", "Requested"),
            ("pcre2", "Dependency"),
            ("python@3.12", "Unknown"),
        ] {
            assert!(
                inventory
                    .lines()
                    .any(|line| line.contains(name) && line.contains(intent)),
                "{w}: {name} intent missing"
            );
        }
        key(&mut a, KeyCode::Char('u'));
        key(&mut a, KeyCode::Char('i'));
        key(&mut a, KeyCode::Char('i'));
        let screen = draw(&mut a, w, h);
        assert!(screen.contains("[Dependencies 1]"));
        assert!(screen.contains("Used by others 1 [ON]"));
        assert_eq!(a.active_focus(), Focus::Inventory);
        assert!(!a.inspecting() && !a.refresh_requested && a.check_requested.is_none());
    }
    let mut a = app();
    for (w, h) in [(0, 0), (30, 10), (45, 20)] {
        draw(&mut a, w, h);
    }
}
