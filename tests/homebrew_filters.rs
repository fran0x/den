// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, CollectionStatus, Focus, HomebrewCounts},
    areas::homebrew::graph::Evidence,
    areas::homebrew::{self, CollectionError},
    inventory::Inventory,
    tui::ui,
};
use ratatui::{Terminal, backend::TestBackend, style::Modifier};

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
fn homebrew_starts_with_tools_and_counts_follow_search() {
    let mut a = app();
    assert!(!a.show_all);
    assert_eq!(names(&a), ["ripgrep", "requested-app"]);
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 7, tools: 2 });
    key(&mut a, KeyCode::Char('a'));
    assert!(a.show_all);
    assert_eq!(a.visible.len(), 7);
    assert_eq!(a.homebrew_counts(), HomebrewCounts { all: 7, tools: 2 }); // Counts ignore the view.
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
    a.explorer.evidence = Evidence::Current;
    a.refresh();
    assert_eq!(names(&a), ["manual", "consumer"]); // Panel evidence does not redefine Tools.
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
fn f_does_not_filter_types_and_a_stays_text_during_search() {
    let mut a = app();
    key(&mut a, KeyCode::Char('f'));
    assert_eq!(a.filter, None);
    assert_eq!(names(&a), ["ripgrep", "requested-app"]);
    key(&mut a, KeyCode::Char('a'));
    assert_eq!(a.visible.len(), 7);
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
    for _ in 0..5 {
        key(&mut a, KeyCode::Tab); // Agents, Cloud, Docker, Toolchains, back to Homebrew.
    }
    assert!(!a.show_all);
    assert_eq!(a.resource().unwrap().name, "ripgrep");
    key(&mut a, KeyCode::Enter); // Packages have no inspector.
    assert!(!a.inspecting());
    // A diagnostic jump to a dependency selects it in Packages, showing all packages.
    let pcre2 = a
        .inventory
        .resources
        .iter()
        .position(|r| r.name == "pcre2")
        .unwrap();
    a.jump_to(a.graph.preferred(pcre2));
    assert!(!a.inspecting());
    assert_eq!(a.resource().unwrap().name, "pcre2");
    assert!(a.show_all);
    key(&mut a, KeyCode::Esc);
    assert_eq!(a.resource().unwrap().name, "ripgrep");
    assert!(!a.show_all);
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
        assert!(screen.contains("[Tools 2] · All 7"), "{w}: {screen}");
        assert!(screen.lines().last().unwrap().contains("a:tools/all"));
        assert_eq!(names(&a), ["ripgrep", "requested-app"]);
        key(&mut a, KeyCode::Char('a'));
        let screen = draw(&mut a, w, h);
        assert!(screen.contains("Tools 2 · [All 7]"));
        assert!(!screen.contains("Type:") && !screen.contains("f:type"));
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
fn tools_are_bold_and_casks_are_bracketed() {
    let mut a = app();
    key(&mut a, KeyCode::Char('a'));
    let mut t = Terminal::new(TestBackend::new(120, 32)).unwrap();
    t.draw(|f| ui::render(f, &mut a)).unwrap();
    let buffer = t.backend().buffer();
    let row = |name: &str| {
        (0..32)
            .find(|&y| {
                let line: String = (0..60).map(|x| buffer[(x, y)].symbol()).collect();
                line.contains(&format!(" {name} "))
            })
            .unwrap_or_else(|| panic!("{name} row"))
    };
    let bold = |y: u16| buffer[(4, y)].modifier.contains(Modifier::BOLD);
    // ripgrep and the requested-app cask are tools; pcre2 and the widget cask are not.
    assert!(bold(row("ripgrep")) && bold(row("[requested-app]")));
    assert!(!bold(row("pcre2")) && !bold(row("[widget]")));
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

#[test]
fn outdated_packages_show_the_newer_version() {
    let resources = inventory().resources;
    let update = |name: &str, cask: bool| {
        let r = resources
            .iter()
            .find(|r| r.name == name && (r.kind == den::inventory::ResourceKind::Cask) == cask)
            .unwrap();
        r.homebrew.as_ref().unwrap().update().map(str::to_owned)
    };
    assert_eq!(update("ripgrep", false).as_deref(), Some("99.0"));
    assert_eq!(update("widget", true).as_deref(), Some("9.0"));
    // python@3.12 has a newer stable version, but Homebrew does not flag it outdated.
    assert_eq!(update("python@3.12", false), None);
    assert_eq!(update("pcre2", false), None);

    let mut a = app();
    let screen = draw(&mut a, 140, 32);
    assert!(
        screen.contains("Homebrew snapshot · 2 outdated"),
        "{screen}"
    );
    let row = screen.lines().find(|l| l.contains("> ripgrep")).unwrap();
    assert!(row.contains("↑ 99.0 · Requested"), "{row}");
    assert!(screen.contains("↑ 99.0 available · as of last brew update"));
}

#[test]
fn a_revision_bump_is_part_of_the_latest_version() {
    let raw = br#"{"formulae":[{"name":"lib","versions":{"stable":"1.1.2"},"revision":1,
        "outdated":true,"pinned":true,"installed":[{"version":"1.1.2"}]}],"casks":[]}"#;
    let inventory = homebrew::parse(raw).unwrap();
    let m = inventory.resources[0].homebrew.as_ref().unwrap();
    assert_eq!(m.update(), Some("1.1.2_1"));
    assert!(m.pinned());
    let mut a = App::new(inventory, "Homebrew snapshot");
    key(&mut a, KeyCode::Char('a'));
    assert!(draw(&mut a, 140, 32).contains("as of last brew update · pinned"));
}
