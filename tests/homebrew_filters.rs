use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, CollectionStatus, Focus, HomebrewCounts},
    graph::Evidence,
    homebrew::{self, CollectionError},
    inventory::{Inventory, ResourceKind},
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
fn homebrew_starts_with_tools_and_counts_follow_search_and_type() {
    let mut a = app();
    assert!(!a.show_all);
    assert_eq!(names(&a), ["ripgrep", "requested-app"]);
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 7, tools: 2 });
    key(&mut a, KeyCode::Char('a'));
    assert!(a.show_all);
    assert_eq!(a.visible.len(), 7);
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 7, tools: 2 }); // Counts ignore the view.
    a.filter = Some(ResourceKind::Formula);
    a.refresh();
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 5, tools: 1 });
    a.filter = Some(ResourceKind::Cask);
    a.refresh();
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 2, tools: 1 });
    a.filter = None;
    a.query = "widget".into();
    a.refresh();
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 2, tools: 0 });
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
fn tools_use_request_records_only_and_required_by_is_positive_recorded_evidence() {
    let mut a = App::new(evidence_fixture(), "Homebrew snapshot");
    let before: Vec<_> = a.inventory.resources.iter().map(|r| r.intent()).collect();
    // Conflicting and mixed records stay Unknown, so they are not tools.
    assert_eq!(names(&a), ["manual", "consumer"]);
    a.explorer.preferred = Evidence::Current;
    a.refresh();
    assert_eq!(names(&a), ["manual", "consumer"]); // Inspector evidence does not redefine Tools.
    key(&mut a, KeyCode::Char('a'));
    let required: Vec<_> = a
        .visible
        .iter()
        .filter(|&&n| a.graph.recorded_users(n) > 0)
        .map(|&n| {
            (
                a.inventory.resources[n].name.as_str(),
                a.graph.recorded_users(n),
            )
        })
        .collect();
    // Two records from consumer to manual count as one package. Current declarations,
    // self references and unresolved or missing versions do not count.
    assert_eq!(
        required,
        [
            ("manual", 1),
            ("dep", 1),
            ("unknown", 1),
            ("false-only", 1),
            ("mixed", 1)
        ]
    );
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
    assert!(
        a.inventory
            .resources
            .iter()
            .enumerate()
            .filter(|(_, r)| r.name == "clash")
            .all(|(node, _)| a.graph.recorded_users(node) == 0)
    );
}

#[test]
fn type_view_and_search_intersect_and_a_stays_text_during_search() {
    let mut a = app();
    key(&mut a, KeyCode::Char('f'));
    assert_eq!(names(&a), ["ripgrep"]);
    key(&mut a, KeyCode::Char('a'));
    assert_eq!(
        names(&a),
        ["ripgrep", "pcre2", "python@3.12", "widget", "legacy"]
    );
    key(&mut a, KeyCode::Char('/'));
    for c in "pcrea".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    assert_eq!(a.query, "pcrea");
    assert!(a.show_all && a.visible.is_empty());
    a.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    for c in "pcre".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    key(&mut a, KeyCode::Enter);
    assert_eq!(names(&a), ["pcre2"]);
    key(&mut a, KeyCode::Char('a'));
    assert!(!a.show_all && a.visible.is_empty()); // pcre2 is a dependency, not a tool.
}

#[test]
fn view_survives_area_changes_and_back_from_a_hidden_related_package() {
    let mut a = app();
    key(&mut a, KeyCode::Tab);
    key(&mut a, KeyCode::Tab);
    assert!(!a.show_all);
    assert_eq!(a.resource().unwrap().name, "ripgrep");
    key(&mut a, KeyCode::Enter);
    a.open_detail(Detail::Dependencies);
    let pcre2 = a
        .explorer
        .rows
        .iter()
        .position(|r| r.label.contains("pcre2"))
        .unwrap();
    a.explorer.selected = pcre2;
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.resource().unwrap().name, "pcre2");
    assert!(a.show_all); // The dependency is not a tool; the jump shows all packages.
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.resource().unwrap().name, "ripgrep");
    assert!(!a.show_all);
    assert_eq!(a.explorer.selected, pcre2);
}

#[test]
fn refresh_keeps_the_view_and_failure_retains_counts_and_selection() {
    let mut a = App::live_homebrew();
    a.begin_collection();
    a.finish_collection(Ok(inventory()));
    assert_eq!(names(&a), ["ripgrep", "requested-app"]);
    key(&mut a, KeyCode::Char('a'));
    key(&mut a, KeyCode::Char('j'));
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
    assert!(a.show_all);
    assert_eq!(a.resource().unwrap().id, selected);
}

#[test]
fn view_counts_and_row_evidence_are_visible_at_supported_sizes() {
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        let mut a = app();
        let screen = draw(&mut a, w, h);
        assert!(screen.contains("View:Tools"), "{w}: {screen}");
        assert!(
            screen.contains("[Tools 2] · All 7 · a: show all · f: type"),
            "{w}: {screen}"
        );
        assert_eq!(names(&a), ["ripgrep", "requested-app"]);
        key(&mut a, KeyCode::Char('a'));
        let screen = draw(&mut a, w, h);
        assert!(screen.contains("View:All"));
        assert!(screen.contains("Tools 2 · [All 7] · a: tools only"));
        // Narrow side-by-side rows keep "required by" rather than the intent word.
        let pcre2 = if w == 120 {
            "required by 1"
        } else {
            "Dependency · required by 1"
        };
        for (name, label) in [
            ("ripgrep", "Requested"),
            ("pcre2", pcre2),
            ("pcre2", "required by 1"),
            ("python@3.12", "Unknown"),
        ] {
            assert!(
                screen
                    .lines()
                    .any(|line| line.contains(name) && line.contains(label)),
                "{w}: {name} missing {label}: {screen}"
            );
        }
        assert_eq!(a.active_focus(), Focus::Inventory);
        assert!(!a.inspecting() && !a.refresh_requested && a.check_requested.is_none());
    }
    let mut a = app();
    for (w, h) in [(0, 0), (30, 10), (45, 20)] {
        draw(&mut a, w, h);
    }
}

#[test]
fn empty_tools_view_offers_all_packages() {
    let mut raw: serde_json::Value =
        serde_json::from_slice(include_bytes!("../fixtures/brew.json")).unwrap();
    raw["formulae"][0]["installed"][0]["installed_on_request"] = false.into();
    raw["casks"][1]["installed_on_request"] = false.into();
    let mut a = App::new(
        homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap(),
        "Homebrew snapshot",
    );
    assert!(a.visible.is_empty());
    let screen = draw(&mut a, 80, 24);
    assert!(screen.contains("No packages recorded as requested."));
    assert!(screen.contains("a: show all packages"));
    a.query = "nothing-matches".into();
    a.refresh();
    assert!(draw(&mut a, 80, 24).contains("No matching tools."));
    key(&mut a, KeyCode::Char('a'));
    assert!(draw(&mut a, 80, 24).contains("No matches."));
}
