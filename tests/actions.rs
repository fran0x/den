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
    key(a, KeyCode::Char('t'));
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
/// The selected action's place in the menu.
fn selected(a: &App) -> usize {
    match &a.actions.dialog {
        Some(Dialog::Menu { selected, .. }) => *selected,
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
fn the_menu_offers_what_fits_the_selection_package_first() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Enter);
    // Enter lists only the package's own actions; Homebrew's are under a.
    assert_eq!(
        menu(&a),
        [
            ("brew upgrade --formula ripgrep".into(), None),
            ("brew uninstall --formula ripgrep".into(), None),
        ]
    );
    assert_eq!(selected(&a), 0, "the upgrade is selected");
    key(&mut a, KeyCode::Esc);
    assert!(a.actions.dialog.is_none());

    // Something needs pcre2, so uninstalling it is offered but blocked, and there is no
    // upgrade.
    let mut a = app(true);
    select(&mut a, "pcre2");
    key(&mut a, KeyCode::Enter);
    let items = menu(&a);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, "brew uninstall --formula pcre2");
    assert!(items[0].1.as_deref().unwrap().contains("required by 1"));

    // Casks get --cask, and a tapped package its full name, so no other tap's package
    // of the same name can be meant. Another cask declaring it needs this one blocks the
    // uninstall too.
    let mut a = app(true);
    key(&mut a, KeyCode::Char('t'));
    key(&mut a, KeyCode::Char('/'));
    type_text(&mut a, "widget");
    key(&mut a, KeyCode::Enter);
    while a.resource().unwrap().kind != den::inventory::ResourceKind::Cask {
        key(&mut a, KeyCode::Down);
    }
    key(&mut a, KeyCode::Enter);
    assert_eq!(menu(&a)[0].0, "brew upgrade --cask example/tools/widget");
}

#[test]
fn den_has_no_actions_at_all() {
    let mut a = app(false);
    select(&mut a, "ripgrep");
    let screen = draw(&mut a, 120, 30);
    assert!(!screen.contains("Enter:actions"), "{screen}");
    // x opens nothing, so no command is shown.
    key(&mut a, KeyCode::Enter);
    assert!(a.actions.dialog.is_none());
    let screen = draw(&mut a, 120, 30);
    for command in [
        "brew upgrade",
        "brew uninstall",
        "brew autoremove",
        "brew cleanup",
    ] {
        assert!(!screen.contains(command), "{screen}");
    }
    key(&mut a, KeyCode::Char('W'));
    assert!(!a.actions.allowed());
    assert!(!draw(&mut a, 120, 30).contains(" WRITE "));
    assert_eq!(a.actions.request, None);
}

fn upgrade() -> Action {
    Action::BrewUpgrade(den::actions::Package::new("ripgrep", false).unwrap())
}
fn ready(a: &mut App, action: &Action, text: &str) {
    a.actions.finish_preview(
        action,
        Ok(CommandOutput {
            code: Some(0),
            stdout: text.as_bytes().to_vec(),
            stderr: vec![],
        }),
    );
}

#[test]
fn x_shows_the_actions_and_the_dry_run_then_enter_runs_it() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    // dew shows WRITE from the start.
    assert!(draw(&mut a, 120, 30).contains(" WRITE "));
    key(&mut a, KeyCode::Enter);
    // Opening starts the selected action's dry run.
    assert_eq!(a.actions.request.take(), Some(Request::Preview(upgrade())));
    let screen = draw(&mut a, 120, 36);
    // The title names the package; the dry run below says what will change.
    assert!(screen.contains("Actions · ripgrep"), "{screen}");
    assert!(screen.contains("Checking…"), "{screen}");
    ready(
        &mut a,
        &upgrade(),
        "==> Would upgrade 1 outdated package:\nripgrep 14.1.1 -> 99.0\n",
    );
    assert!(draw(&mut a, 120, 36).contains("ripgrep 14.1.1 -> 99.0"));
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.actions.request.take(), Some(Request::Run(upgrade())));

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
fn moving_the_selection_starts_that_dry_run() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('a'));
    // brew update has no dry run: nothing to wait for.
    assert_eq!(a.actions.request, None);
    key(&mut a, KeyCode::Down);
    assert_eq!(
        a.actions.request.take(),
        Some(Request::Preview(Action::BrewAutoremove))
    );
    // Cleanup's late dry run is not shown against autoremove.
    ready(&mut a, &Action::BrewCleanup, "Would remove: old-download\n");
    assert!(!draw(&mut a, 120, 36).contains("old-download"));
}

#[test]
fn a_blocked_uninstall_cannot_run() {
    let mut a = app(true);
    select(&mut a, "pcre2");
    key(&mut a, KeyCode::Enter);
    pick(&mut a, "brew uninstall --formula pcre2");
    assert!(matches!(
        a.actions.dialog,
        Some(Dialog::Menu {
            preview: Preview::None,
            ..
        })
    ));
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.actions.request, None);
    assert!(draw(&mut a, 120, 36).contains("required by 1"));
}

#[test]
fn ctrl_c_quits_from_menus_but_not_mid_action() {
    let mut a = app(true);
    key(&mut a, KeyCode::Enter);
    ctrl_c(&mut a);
    assert!(a.quit && a.actions.dialog.is_none());
}

#[test]
fn dialogs_fit_small_and_large_screens() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('a'));
    for (w, h) in [(30, 10), (45, 20), (80, 24), (160, 45)] {
        draw(&mut a, w, h);
    }
    pick(&mut a, "brew cleanup");
    ready(
        &mut a,
        &Action::BrewCleanup,
        &"Would remove: \u{1b}[31mold\n".repeat(500),
    );
    for (w, h) in [(30, 10), (45, 20), (80, 24), (160, 45)] {
        assert!(!draw(&mut a, w, h).contains('\u{1b}'));
    }
}

#[test]
fn the_same_key_again_runs_a_removal_or_an_upgrade() {
    // x opens the uninstall alone; x again runs it, like Enter.
    let mut a = app(true);
    select(&mut a, "ripgrep");
    assert!(draw(&mut a, 140, 30).contains("x:remove u:upgrade Enter:actions a:Homebrew actions"));
    key(&mut a, KeyCode::Char('x'));
    assert_eq!(
        menu(&a),
        [("brew uninstall --formula ripgrep".into(), None)]
    );
    assert!(draw(&mut a, 120, 30).contains("x or Enter: run it"));
    key(&mut a, KeyCode::Char('x'));
    assert_eq!(
        a.actions.request.take(),
        Some(Request::Run(Action::BrewUninstall(
            den::actions::Package::new("ripgrep", false).unwrap()
        )))
    );

    // u opens the upgrade and its dry run; u again after the dry run runs it.
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(a.actions.request.take(), Some(Request::Preview(upgrade())));
    ready(&mut a, &upgrade(), "ripgrep 14.1.1 -> 99.0\n");
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(a.actions.request.take(), Some(Request::Run(upgrade())));

    // u u quickly, before the dry run finishes: it waits, says so, and runs as soon as
    // the dry run succeeds.
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(a.actions.request.take(), Some(Request::Preview(upgrade())));
    key(&mut a, KeyCode::Char('u'));
    assert_eq!(a.actions.request, None, "the dry run is still running");
    let screen = draw(&mut a, 120, 30);
    assert!(
        screen.contains("runs as soon as the dry run succeeds"),
        "{screen}"
    );
    assert!(!screen.contains("Wait for"), "{screen}");
    ready(&mut a, &upgrade(), "ripgrep 14.1.1 -> 99.0\n");
    assert_eq!(a.actions.request.take(), Some(Request::Run(upgrade())));

    // A failed dry run never runs it.
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('u'));
    a.actions.request.take();
    key(&mut a, KeyCode::Char('u'));
    a.actions.finish_preview(
        &upgrade(),
        Ok(CommandOutput {
            code: Some(1),
            stdout: vec![],
            stderr: b"Error: no network".to_vec(),
        }),
    );
    assert_eq!(a.actions.request, None);
    assert_eq!(
        a.actions.notice.as_deref(),
        Some("Not run: its dry run failed")
    );

    // Esc takes it back before the dry run finishes.
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('u'));
    a.actions.request.take();
    key(&mut a, KeyCode::Char('u'));
    key(&mut a, KeyCode::Esc);
    ready(&mut a, &upgrade(), "ripgrep 14.1.1 -> 99.0\n");
    assert_eq!(a.actions.request, None);

    // Any other key closes it without running anything.
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('x'));
    key(&mut a, KeyCode::Char('j'));
    assert!(a.actions.dialog.is_none());
    assert_eq!(a.actions.request, None);

    // A blocked removal does not run either.
    let mut a = app(true);
    select(&mut a, "pcre2");
    key(&mut a, KeyCode::Char('x'));
    key(&mut a, KeyCode::Char('x'));
    assert_eq!(a.actions.request, None);
    assert!(
        a.actions
            .notice
            .as_deref()
            .unwrap()
            .contains("required by 1")
    );

    // No newer version known: u says so instead of opening anything.
    let mut a = app(true);
    select(&mut a, "pcre2");
    key(&mut a, KeyCode::Char('u'));
    assert!(a.actions.dialog.is_none());
    assert!(
        a.actions
            .notice
            .as_deref()
            .unwrap()
            .starts_with("Nothing to upgrade here")
    );
}

#[test]
fn a_opens_homebrews_own_actions() {
    let mut a = app(true);
    select(&mut a, "ripgrep");
    key(&mut a, KeyCode::Char('a'));
    assert_eq!(
        menu(&a),
        [
            ("brew update".into(), None),
            ("brew autoremove".into(), None),
            ("brew cleanup".into(), None),
        ]
    );
    assert!(draw(&mut a, 120, 30).contains("Actions · Homebrew"));
}

#[test]
fn in_den_x_u_and_a_change_nothing() {
    let mut a = app(false);
    select(&mut a, "ripgrep");
    for c in ['x', 'a'] {
        key(&mut a, KeyCode::Char(c));
        assert!(a.actions.dialog.is_none(), "{c}");
    }
    key(&mut a, KeyCode::Char('u'));
    assert!(a.actions.dialog.is_none());
    assert_eq!(
        a.actions.notice.as_deref(),
        Some("U checks online; u upgrades in dew")
    );
}
