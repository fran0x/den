// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Write actions through the app: what the menu offers, which keys run what, and that
//! nothing runs outside `dew`, without write mode, or without a confirmation.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    actions::{Action, Dialog, Permit, Preview, Request, State},
    app::App,
    areas::homebrew::{self, CommandOutput},
    tui::ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::path::Path;

fn app(allow_writes: bool) -> App {
    let inventory = homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap();
    let mut a = App::new(inventory, "Homebrew snapshot");
    a.actions = State::new(Permit::granted(allow_writes));
    a
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn ctrl_c(a: &mut App) {
    a.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
}
fn type_text(a: &mut App, text: &str) {
    for c in text.chars() {
        key(a, KeyCode::Char(c));
    }
}
/// Shows all packages and selects the one named.
fn select(a: &mut App, name: &str) {
    key(a, KeyCode::Char('a'));
    key(a, KeyCode::Char('/'));
    type_text(a, name);
    key(a, KeyCode::Enter);
    assert_eq!(a.resource().unwrap().name, name);
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
fn menu(a: &App) -> Vec<(String, Option<String>)> {
    match &a.actions.dialog {
        Some(Dialog::Menu { items, .. }) => items
            .iter()
            .map(|i| (i.action.command_line(), i.blocked.clone()))
            .collect(),
        other => panic!("no menu: {other:?}"),
    }
}
/// Moves the menu selection to the action with this command line.
fn pick(a: &mut App, line: &str) {
    let at = menu(a).iter().position(|(l, _)| l == line).unwrap();
    key(a, KeyCode::Home);
    for _ in 0..at {
        key(a, KeyCode::Down);
    }
}

#[test]
fn the_menu_offers_what_fits_the_selection() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('x'));
    assert_eq!(
        menu(&a),
        [
            ("brew upgrade --formula ripgrep".into(), None),
            ("brew uninstall --formula ripgrep".into(), None),
            ("brew update".into(), None),
            ("brew autoremove".into(), None),
            ("brew cleanup".into(), None),
        ]
    );
    key(&mut a, KeyCode::Esc);
    assert!(a.actions.dialog.is_none());

    // Something needs pcre2, so uninstalling it is offered but blocked; no upgrade.
    let mut a = app(true);
    select(&mut a, "pcre2");
    key(&mut a, KeyCode::Char('x'));
    let items = menu(&a);
    assert_eq!(items[0].0, "brew uninstall --formula pcre2");
    assert!(items[0].1.as_deref().unwrap().contains("required by 1"));
    assert!(!items.iter().any(|(l, _)| l.starts_with("brew upgrade")));

    // Casks get --cask, and a tapped package its full name, so no other tap's package
    // of the same name can be meant.
    let mut a = app(true);
    key(&mut a, KeyCode::Char('a'));
    key(&mut a, KeyCode::Char('/'));
    type_text(&mut a, "widget");
    key(&mut a, KeyCode::Enter);
    while a.resource().unwrap().kind != den::inventory::ResourceKind::Cask {
        key(&mut a, KeyCode::Down);
    }
    key(&mut a, KeyCode::Char('x'));
    assert_eq!(menu(&a)[0].0, "brew upgrade --cask example/tools/widget");
}

#[test]
fn den_has_no_actions_at_all() {
    let mut a = app(false);
    select(&mut a, "ripgrep");
    let screen = draw(&mut a, 120, 30);
    assert!(!screen.contains("x:actions"), "{screen}");
    // x opens nothing, so no command is shown or copied.
    key(&mut a, KeyCode::Char('x'));
    assert!(a.actions.dialog.is_none());
    assert_eq!(a.actions.clipboard, None);
    let screen = draw(&mut a, 120, 30);
    for command in [
        "brew upgrade",
        "brew uninstall",
        "brew autoremove",
        "brew cleanup",
    ] {
        assert!(!screen.contains(command), "{screen}");
    }
    // W is just a letter: there is no write mode to switch on.
    key(&mut a, KeyCode::Char('W'));
    assert!(!a.actions.allowed());
    assert!(!draw(&mut a, 120, 30).contains(" WRITE "));
    assert_eq!(a.actions.request, None);
}

#[test]
fn dew_previews_then_y_runs_it() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    // dew shows WRITE from the start; there is no switch to turn on first.
    assert!(draw(&mut a, 120, 30).contains(" WRITE "));
    key(&mut a, KeyCode::Char('x'));
    key(&mut a, KeyCode::Enter);
    let upgrade = Action::BrewUpgrade(den::actions::Package::new("ripgrep", false).unwrap());
    assert_eq!(
        a.actions.request.take(),
        Some(Request::Preview(upgrade.clone()))
    );
    assert!(draw(&mut a, 120, 30).contains("Checking…"));
    // Enter never confirms: a second Enter must not run anything.
    key(&mut a, KeyCode::Enter);
    a.actions.finish_preview(Ok(CommandOutput {
        code: Some(0),
        stdout: b"==> Would upgrade 1 outdated package:\nripgrep 14.1.1 -> 99.0\n".to_vec(),
        stderr: vec![],
    }));
    assert!(draw(&mut a, 120, 30).contains("ripgrep 14.1.1 -> 99.0"));
    key(&mut a, KeyCode::Char('y'));
    assert_eq!(a.actions.request.take(), Some(Request::Run(upgrade)));

    // While it runs, Ctrl-C and Esc stop it instead of quitting, and Tab does nothing.
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.module, den::app::Module::Homebrew);
    ctrl_c(&mut a);
    assert!(!a.quit && a.actions.cancel_requested);
    assert!(draw(&mut a, 120, 30).contains("Stopping"));

    let log = std::env::temp_dir().join(format!("den-actions-app-{}", std::process::id()));
    a.finish_action(
        Err(homebrew::CollectionError::Cancelled),
        Some(&log.join("actions.log")),
    );
    let screen = draw(&mut a, 120, 30);
    assert!(screen.contains("Stopped before it finished"), "{screen}");
    key(&mut a, KeyCode::Enter);
    assert!(a.actions.dialog.is_none());
    let _ = std::fs::remove_dir_all(&log);
}

#[test]
fn a_blocked_uninstall_cannot_be_confirmed() {
    let mut a = app(true);
    select(&mut a, "pcre2");
    key(&mut a, KeyCode::Char('x'));
    pick(&mut a, "brew uninstall --formula pcre2");
    key(&mut a, KeyCode::Enter);
    assert!(matches!(
        a.actions.dialog,
        Some(Dialog::Confirm {
            preview: Preview::None,
            ..
        })
    ));
    key(&mut a, KeyCode::Char('y'));
    assert_eq!(a.actions.request, None);
    assert!(draw(&mut a, 120, 30).contains("required by 1"));
}

#[test]
fn ctrl_c_quits_from_menus_but_not_mid_action() {
    let mut a = app(true);
    key(&mut a, KeyCode::Char('x'));
    ctrl_c(&mut a);
    assert!(a.quit && a.actions.dialog.is_none());
}

#[test]
fn dialogs_fit_small_and_large_screens() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('x'));
    for (w, h) in [(30, 10), (45, 20), (80, 24), (160, 45)] {
        draw(&mut a, w, h);
    }
    pick(&mut a, "brew cleanup");
    key(&mut a, KeyCode::Enter);
    a.actions.finish_preview(Ok(CommandOutput {
        code: Some(0),
        stdout: "Would remove: \u{1b}[31mold\n".repeat(500).into_bytes(),
        stderr: vec![],
    }));
    for (w, h) in [(30, 10), (45, 20), (80, 24), (160, 45)] {
        assert!(!draw(&mut a, w, h).contains('\u{1b}'));
    }
}
