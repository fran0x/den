use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, CollectionStatus, Focus},
    homebrew::{self, CollectionError},
    inventory::{
        HomebrewMetadata, InstallationIntent as Intent, ResourceKind, installation_intent,
    },
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::io::Cursor;

const FIXTURE: &[u8] = include_bytes!("../fixtures/brew.json");
fn inventory() -> den::inventory::Inventory {
    homebrew::parse(FIXTURE).unwrap()
}
fn app() -> App {
    App::new(inventory(), "Homebrew snapshot")
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn normalizes_formulae_and_casks_using_installed_versions_only() {
    let inventory = inventory();
    assert_eq!(inventory.resources.len(), 7);
    assert_eq!(inventory.resources[0].version.as_deref(), Some("14.1.1"));
    assert_eq!(
        inventory.resources[2].version.as_deref(),
        Some("3.12.7, 3.12.8")
    );
    let cask = &inventory.resources[5];
    assert_eq!(cask.kind, ResourceKind::Cask);
    assert_eq!(cask.version.as_deref(), Some("8.1"));
    assert!(
        matches!(cask.homebrew.as_ref().unwrap(), HomebrewMetadata::Cask { display_names, .. } if display_names == &["Widget Studio"])
    );
}

#[test]
fn identities_distinguish_type_tap_and_versioned_names() {
    let resources = inventory().resources;
    assert_eq!(resources[0].id, "brew:formula:homebrew/core/ripgrep");
    assert_eq!(resources[2].id, "brew:formula:homebrew/core/python@3.12");
    assert_eq!(resources[3].id, "brew:formula:example/tools/widget");
    assert_eq!(resources[5].id, "brew:cask:example/tools/widget");
    let mut raw: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    raw["formulae"][3]
        .as_object_mut()
        .unwrap()
        .remove("full_name");
    let parsed = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(parsed.resources[3].id, resources[3].id);
}

#[test]
fn installation_intent_is_conservative_for_every_flag_combination() {
    for requested in [None, Some(false), Some(true)] {
        for dependency in [None, Some(false), Some(true)] {
            let expected = match (requested, dependency) {
                (Some(true), Some(true)) => Intent::Unknown,
                (Some(true), _) => Intent::Requested,
                (_, Some(true)) => Intent::Dependency,
                _ => Intent::Unknown,
            };
            assert_eq!(installation_intent(requested, dependency), expected);
        }
    }
    let intents: Vec<_> = inventory()
        .resources
        .iter()
        .map(|r| r.homebrew.as_ref().unwrap().intent())
        .collect();
    assert_eq!(
        intents,
        [
            Intent::Requested,
            Intent::Dependency,
            Intent::Unknown,
            Intent::Unknown,
            Intent::Unknown,
            Intent::Unknown,
            Intent::Requested
        ]
    );
}

#[test]
fn all_kegs_must_agree_and_missing_keg_flags_make_intent_unknown() {
    let mut raw: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    let second = raw["formulae"][2]["installed"][0].clone();
    raw["formulae"][2]["installed"][1] = second;
    let parsed = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(
        parsed.resources[2].homebrew.as_ref().unwrap().intent(),
        Intent::Requested
    );
    raw["formulae"][2]["installed"][1]
        .as_object_mut()
        .unwrap()
        .remove("installed_on_request");
    let parsed = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(
        parsed.resources[2].homebrew.as_ref().unwrap().intent(),
        Intent::Unknown
    );
}

#[test]
fn source_metadata_is_retained_without_dependency_traversal() {
    let resources = inventory().resources;
    let HomebrewMetadata::Formula {
        linked_keg,
        dependencies,
        installed,
        ..
    } = resources[0].homebrew.as_ref().unwrap()
    else {
        panic!("formula expected")
    };
    assert_eq!(linked_keg.as_deref(), Some("14.1.1"));
    assert_eq!(
        dependencies["build_dependencies"],
        serde_json::json!(["rust"])
    );
    assert!(!dependencies.contains_key("future_field"));
    assert_eq!(
        installed[0].runtime_dependencies.as_ref().unwrap()[0]["version"],
        "10.44"
    );
    assert!(resources[2].description.is_empty());
    assert!(resources[4].description.is_empty());
}

#[test]
fn malformed_incomplete_and_duplicate_inventory_never_become_empty_success() {
    for bytes in [
        include_bytes!("../fixtures/brew-incomplete.json").as_slice(),
        include_bytes!("../fixtures/brew-invalid.json").as_slice(),
        br#"{}"#,
        br#"{"formulae":[]}"#,
        br#"{"formulae":null,"casks":[]}"#,
        br#"{"formulae":[{"name":"x","installed":[]}],"casks":[]}"#,
        br#"{"formulae":[],"casks":[{"token":"x","version":"9","installed":null}]}"#,
        br#"{"formulae":[],"casks":[{"token":"x","installed":9}]}"#,
    ] {
        assert!(homebrew::parse(bytes).is_err());
    }
    let mut raw: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    let duplicate = raw["formulae"][0].clone();
    raw["formulae"].as_array_mut().unwrap().push(duplicate);
    assert!(homebrew::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
}

#[test]
fn brew_snapshots_use_the_same_parser_and_enforce_limits() {
    assert_eq!(
        homebrew::read_snapshot(Cursor::new(FIXTURE)).unwrap(),
        inventory()
    );
    assert_eq!(
        homebrew::load_snapshot(std::path::Path::new("fixtures/brew.json")).unwrap(),
        inventory()
    );
    assert!(
        homebrew::parse(include_bytes!("../fixtures/brew-empty.json"))
            .unwrap()
            .resources
            .is_empty()
    );
    assert!(homebrew::load_snapshot(std::path::Path::new("fixtures")).is_err());
    assert!(homebrew::load_snapshot(std::path::Path::new("fixtures/missing.json")).is_err());
    assert_eq!(
        homebrew::read_snapshot(Cursor::new(vec![b' '; homebrew::MAX_INPUT_BYTES + 1])),
        Err(CollectionError::OutputLimit)
    );
    let mut raw: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    raw["formulae"][0]["desc"] = "a".repeat(8193).into();
    assert!(homebrew::parse(&serde_json::to_vec(&raw).unwrap()).is_err());
}

#[test]
fn search_type_and_intent_filters_compose_without_changing_existing_controls() {
    let mut app = app();
    app.query = "editor widget".into();
    app.refresh();
    assert_eq!(app.resource().unwrap().id, "brew:cask:example/tools/widget");
    key(&mut app, KeyCode::Char('f')); // Formula
    assert!(app.visible.is_empty());
    key(&mut app, KeyCode::Char('f')); // Cask
    assert_eq!(app.visible.len(), 1);
    key(&mut app, KeyCode::Char('i')); // Requested
    assert!(app.visible.is_empty());
    key(&mut app, KeyCode::Char('i')); // Dependency
    key(&mut app, KeyCode::Char('i')); // Unknown
    assert_eq!(app.visible.len(), 1);
    key(&mut app, KeyCode::Esc);
    assert_eq!(app.visible.len(), 7);
    app.query = "Widget Studio".into();
    app.refresh();
    assert_eq!(app.visible.len(), 1); // cask display name
    key(&mut app, KeyCode::Char('/'));
    key(&mut app, KeyCode::Char('r'));
    assert!(!app.refresh_requested); // r stays text while searching
}

#[test]
fn refresh_preserves_selection_filters_focus_and_valid_data_after_failure() {
    let mut app = App::live_homebrew();
    app.begin_collection();
    assert_eq!(
        app.collection_status,
        CollectionStatus::Loading { refreshing: false }
    );
    app.finish_collection(Ok(inventory()));
    app.filter = Some(ResourceKind::Formula);
    app.intent_filter = Some(Intent::Unknown);
    app.query = "widget".into();
    app.refresh();
    app.focus = Focus::Details;
    let selected = app.resource().unwrap().id.clone();
    key(&mut app, KeyCode::Char('r'));
    assert!(app.refresh_requested);
    app.begin_collection();
    assert_eq!(
        app.collection_status,
        CollectionStatus::Loading { refreshing: true }
    );
    key(&mut app, KeyCode::Char('r'));
    assert!(!app.refresh_requested); // no concurrent collection
    let mut refreshed = inventory();
    refreshed.resources.reverse();
    app.finish_collection(Ok(refreshed));
    assert_eq!(app.resource().unwrap().id, selected);
    assert_eq!(app.focus, Focus::Details);
    assert_eq!(app.filter, Some(ResourceKind::Formula));
    assert_eq!(app.intent_filter, Some(Intent::Unknown));
    app.begin_collection();
    let before = app.inventory.clone();
    app.finish_collection(Err(CollectionError::Timeout));
    assert_eq!(app.inventory, before);
    assert!(app.collection_message().contains("STALE"));
    assert_eq!(app.resource().unwrap().id, selected);
}

#[test]
fn initial_failure_empty_success_and_removed_selection_are_distinct() {
    let mut app = App::live_homebrew();
    app.begin_collection();
    app.finish_collection(Err(CollectionError::Unavailable));
    assert!(matches!(
        app.collection_status,
        CollectionStatus::Failed { stale: false, .. }
    ));
    assert!(draw(&mut app, 100, 30).contains("Homebrew unavailable"));
    app.begin_collection();
    app.finish_collection(Ok(inventory()));
    key(&mut app, KeyCode::End);
    let mut fewer = inventory();
    fewer.resources.pop();
    app.finish_collection(Ok(fewer));
    assert_eq!(app.selected, Some(0));
    app.finish_collection(Ok(homebrew::parse(include_bytes!(
        "../fixtures/brew-empty.json"
    ))
    .unwrap()));
    assert_eq!(app.collection_status, CollectionStatus::Loaded);
    assert!(draw(&mut app, 100, 30).contains("No installed Homebrew packages"));
    app.begin_collection();
    app.finish_collection(Err(CollectionError::Timeout));
    assert!(matches!(
        app.collection_status,
        CollectionStatus::Failed { stale: true, .. }
    ));
}

#[test]
fn snapshots_cannot_request_live_refresh() {
    let mut app = app();
    key(&mut app, KeyCode::Char('r'));
    assert!(!app.refresh_requested);
}

#[test]
fn rendering_shows_installed_versions_intent_details_and_sanitizes_metadata() {
    let mut app = app();
    let text = draw(&mut app, 140, 40);
    for expected in [
        "ripgrep",
        "Installation intent: Requested",
        "Installed: 14.1.1",
        "Search source trees",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    app.presentation.detailed = true;
    let evidence = draw(&mut app, 140, 40);
    assert!(evidence.contains("Linked keg: 14.1.1"));
    assert!(evidence.contains("on_request: true"));
    app.selected = Some(5);
    let text = draw(&mut app, 140, 40);
    assert!(text.contains("Homebrew-recorded version: 8.1"));
    assert!(text.contains("Casks may self-update"));
    let mut raw: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    raw["formulae"][0]["homepage"] = "evil\u{1b}]52;c;secret\u{7}\u{202e}".into();
    app.inventory = homebrew::parse(&serde_json::to_vec(&raw).unwrap()).unwrap();
    app.selected = Some(0);
    let text = draw(&mut app, 160, 60);
    assert!(!text.chars().any(char::is_control));
    assert!(!text.contains('\u{202e}'));
    for (width, height) in [(45, 30), (30, 10), (10, 5)] {
        draw(&mut app, width, height);
    }
}
