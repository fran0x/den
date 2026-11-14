// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Toolchain actions in dew: what each Rust row offers, what is refused and why, that
//! rustup and cargo run as the copy that wins on PATH, and that Toolchains is read again.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    actions::{Dialog, Permit, Request, State},
    app::App,
    areas::{homebrew::CommandOutput, toolchains},
    inventory::Inventory,
    tui::{presentation::Destination, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const FIXTURE: &str = "fixtures/toolchains";

fn app(dew: bool) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated toolchains fixture",
    );
    let context = toolchains::Context::fixture(Path::new(FIXTURE)).unwrap();
    a.toolchains.configure(context.clone());
    a.toolchains
        .finish(toolchains::discover(&context, &AtomicBool::new(false)));
    a.toolchains.requested = false;
    a.actions = State::new(Permit::granted(dew));
    a.navigate(Destination::Rust);
    a
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn draw(a: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(140, 34)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    (0..34)
        .map(|y| {
            (0..140)
                .map(|x| t.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
/// The fixture's cargo, which wins on PATH (`~/.cargo/bin`).
fn cargo() -> PathBuf {
    std::fs::canonicalize(FIXTURE)
        .unwrap()
        .join("home/.cargo/bin/cargo")
}
/// The fixture has no rustup on PATH; give it one, as if found there.
fn with_rustup(a: &mut App) -> PathBuf {
    let rustup = std::fs::canonicalize(FIXTURE)
        .unwrap()
        .join("home/.cargo/bin/rustup");
    let inventory = a.toolchains.inventory.as_mut().unwrap();
    inventory.winners.insert("rustup", rustup.clone());
    rustup
}
/// x on the row with this key: each action's command line and why it is blocked.
fn menu(a: &mut App, row: &str) -> Vec<(String, Option<String>)> {
    a.toolchains.select_key(row);
    key(a, KeyCode::Char('x'));
    let items = match &a.actions.dialog {
        Some(Dialog::Menu { items, .. }) => items
            .iter()
            .map(|i| (i.action.command_line(), i.blocked.clone()))
            .collect(),
        other => panic!("no menu: {other:?}"),
    };
    key(a, KeyCode::Esc);
    items
}

#[test]
fn toolchains_offer_update_and_uninstall_except_the_default_or_an_override() {
    let mut a = app(true);
    let rustup = with_rustup(&mut a);
    let r = rustup.display();
    // The default can be updated but not uninstalled.
    assert_eq!(
        menu(&mut a, "toolchain:stable-aarch64-apple-darwin"),
        [
            (
                format!("{r} update --no-self-update stable-aarch64-apple-darwin"),
                None
            ),
            (
                format!("{r} toolchain uninstall stable-aarch64-apple-darwin"),
                Some("it is rustup's default toolchain".into())
            ),
        ]
    );
    // A folder override keeps 1.80.0.
    let items = menu(&mut a, "toolchain:1.80.0-aarch64-apple-darwin");
    assert_eq!(
        items[1].1.as_deref(),
        Some("a folder override uses it (1 folder)")
    );
    // Nothing selects nightly: it can go.
    assert_eq!(
        menu(&mut a, "toolchain:nightly-aarch64-apple-darwin")[1],
        (
            format!("{r} toolchain uninstall nightly-aarch64-apple-darwin"),
            None
        )
    );
}

#[test]
fn without_rustup_on_path_toolchain_actions_are_refused() {
    let mut a = app(true);
    for (line, blocked) in menu(&mut a, "toolchain:nightly-aarch64-apple-darwin") {
        assert!(line.starts_with("rustup "), "{line}");
        assert_eq!(blocked.as_deref(), Some("rustup is not on PATH"));
    }
}

#[test]
fn crates_from_crates_io_can_be_upgraded_others_only_uninstalled() {
    let mut a = app(true);
    let c = cargo();
    let c = c.display();
    assert_eq!(
        menu(&mut a, "crate:bacon"),
        [
            (format!("{c} install --locked bacon"), None),
            (format!("{c} uninstall bacon"), None),
        ]
    );
    // From git, with chosen features: Den withholds the URL, so it cannot rebuild it.
    assert_eq!(
        menu(&mut a, "crate:internal-tool"),
        [(format!("{c} uninstall internal-tool"), None)]
    );
    assert_eq!(
        menu(&mut a, "crate:local-thing"),
        [(format!("{c} uninstall local-thing"), None)]
    );
    // From crates.io but with chosen features: a plain cargo install would drop them.
    let inventory = a.toolchains.inventory.as_mut().unwrap();
    let bacon = inventory
        .rust
        .iter_mut()
        .find(|r| r.key == "crate:bacon")
        .unwrap();
    if let Some(toolchains::Manage::Crate { features, .. }) = &mut bacon.manage {
        *features = true;
    }
    let items = menu(&mut a, "crate:bacon");
    assert!(
        items[0].1.as_deref().unwrap().contains("chosen features"),
        "{items:?}"
    );
    assert_eq!(items[1].1, None, "uninstall stays possible");
}

#[test]
fn x_then_enter_runs_the_first_action_and_toolchains_is_read_again() {
    let mut a = app(true);
    a.toolchains.select_key("crate:bacon");
    assert!(draw(&mut a).contains("x:actions"));
    key(&mut a, KeyCode::Char('x'));
    let screen = draw(&mut a);
    assert!(screen.contains("runs the crate's"), "{screen}");
    key(&mut a, KeyCode::Enter);
    let Some(Request::Run(action)) = a.actions.request.take() else {
        panic!("{:?}", a.actions.request);
    };
    assert_eq!(
        action.command_line(),
        format!("{} install --locked bacon", cargo().display())
    );
    let log = std::env::temp_dir().join(format!("den-toolchain-actions-{}", std::process::id()));
    a.finish_action(
        Ok(CommandOutput {
            code: Some(0),
            stdout: vec![],
            stderr: b"package `bacon v3.26.0` is already installed\n".to_vec(),
        }),
        Some(&log.join("actions.log")),
    );
    assert!(a.toolchains.requested, "Toolchains is read again");
    assert!(!a.refresh_requested, "Homebrew is not");
    assert!(draw(&mut a).contains("already installed"));
    let _ = std::fs::remove_dir_all(&log);
}

#[test]
fn den_offers_no_toolchain_actions() {
    let mut a = app(false);
    a.toolchains.select_key("crate:bacon");
    assert!(!draw(&mut a).contains("x:actions"));
    key(&mut a, KeyCode::Char('x'));
    assert!(a.actions.dialog.is_none());
}

fn fixture_path(rest: &str) -> PathBuf {
    std::fs::canonicalize(FIXTURE).unwrap().join(rest)
}

#[test]
fn global_packages_offer_upgrade_and_uninstall_with_their_own_manager() {
    let mut a = app(true);
    a.navigate(Destination::Packages);
    let npm = fixture_path("opt/homebrew/bin/npm");
    let n = npm.display();
    assert_eq!(
        menu(&mut a, "npm -g:@acme/cli"),
        [
            (format!("{n} update -g @acme/cli"), None),
            (format!("{n} uninstall -g @acme/cli"), None),
        ]
    );
    // npm itself comes with Node.
    for (_, blocked) in menu(&mut a, "npm -g:npm") {
        assert_eq!(
            blocked.as_deref(),
            Some("npm comes with Node; upgrade Node instead")
        );
    }
    // uv and pipx are not on the fixture's PATH: refused, by name.
    assert_eq!(
        menu(&mut a, "uv tool:ruff"),
        [
            (
                "uv tool upgrade ruff".to_owned(),
                Some("uv is not on PATH".to_owned())
            ),
            (
                "uv tool uninstall ruff".to_owned(),
                Some("uv is not on PATH".to_owned())
            ),
        ]
    );
    // Found on PATH, they run by their full path.
    let inventory = a.toolchains.inventory.as_mut().unwrap();
    let pipx = fixture_path("home/.local/bin/pipx");
    inventory.winners.insert("pipx", pipx.clone());
    let p = pipx.display();
    assert_eq!(
        menu(&mut a, "pipx:httpie"),
        [
            (format!("{p} upgrade httpie"), None),
            (format!("{p} uninstall httpie"), None),
        ]
    );
    // No actions for uv Pythons, bun or go programs.
    for key in [
        "uv python:cpython-3.12.8-macos-aarch64-none",
        "bun:prettier",
        "go install:gopls",
    ] {
        assert!(menu(&mut a, key).is_empty(), "{key}");
    }
}

#[test]
fn npm_actions_show_npms_own_dry_run() {
    let mut a = app(true);
    a.navigate(Destination::Packages);
    a.toolchains.select_key("npm -g:@acme/cli");
    key(&mut a, KeyCode::Char('x'));
    let Some(Request::Preview(action)) = a.actions.request.clone() else {
        panic!("{:?}", a.actions.request);
    };
    assert_eq!(
        action.preview_line().unwrap(),
        format!(
            "{} update -g --dry-run @acme/cli",
            fixture_path("opt/homebrew/bin/npm").display()
        )
    );
}
