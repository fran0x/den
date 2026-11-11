//! Toolchains: what wins on PATH, Rust toolchains and crates, global packages. Files only.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    inventory::Inventory,
    presentation::Destination,
    toolchains::{self, Section},
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

const FIXTURE: &str = "fixtures/toolchains";

fn collect(context: &toolchains::Context) -> toolchains::Inventory {
    toolchains::discover(context, &AtomicBool::new(false)).unwrap()
}
fn fixture() -> toolchains::Context {
    toolchains::Context::fixture(Path::new(FIXTURE)).unwrap()
}
fn row<'a>(rows: &'a [toolchains::Row], name: &str) -> &'a toolchains::Row {
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("no row {name}"))
}
fn app() -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated toolchains fixture",
    );
    let inventory = collect(&fixture());
    a.toolchains.configure(fixture());
    a.toolchains.finish(Ok(inventory));
    a.toolchains.requested = false;
    a.navigate(Destination::OnPath);
    a
}
fn draw(a: &mut App) -> String {
    let mut t = Terminal::new(TestBackend::new(130, 36)).unwrap();
    t.draw(|f| ui::render(f, a)).unwrap();
    (0..36)
        .map(|y| {
            (0..130)
                .map(|x| t.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

#[test]
fn on_path_shows_the_winner_its_provider_version_and_problems() {
    let i = collect(&fixture());
    let expect = [
        (".", "PATH entry", "Problem · relative PATH entry"),
        ("~/go/bin", "Folder", "Problem · not on PATH"),
        ("node", "Homebrew", "Ready · 22.1.0"),
        // npm's script lives in node_modules/npm: its package.json has the version.
        ("npm", "Homebrew", "Ready · 10.9.0"),
        (
            "python3",
            "macOS system",
            "Problem · the system copy hides Homebrew's (PATH order)",
        ),
        ("ruby", "macOS system", "Ready · version unknown"),
        (
            "java",
            "macOS system",
            "Problem · macOS stub: no JDK is installed",
        ),
        ("rustc", "rustup", "Ready · stable"),
        ("cargo", "rustup", "Ready · stable"),
    ];
    let names: Vec<_> = i.path.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, expect.map(|e| e.0), "rows in order");
    for (name, from, status) in expect {
        let r = row(&i.path, name);
        assert_eq!(
            (r.kind.as_str(), r.status.as_str()),
            (from, status),
            "{name}"
        );
    }
    let python = &row(&i.path, "python3").detail;
    assert!(
        python.contains("Hidden by it (1):\n  /opt/homebrew/bin/python3 · Homebrew · 3.13.1"),
        "{python}"
    );
    assert!(
        row(&i.path, "node")
            .detail
            .contains("Links to: /opt/homebrew/Cellar/node/22.1.0/bin/node")
    );
    assert!(row(&i.path, "~/go/bin").detail.contains("gopls"));
}

#[test]
fn rust_lists_toolchains_default_first_then_crates() {
    let i = collect(&fixture());
    assert_eq!(i.default_toolchain.as_deref(), Some("stable"));
    let expect = [
        ("stable", "Toolchain", "Ready · default"),
        ("1.80.0", "Toolchain", "Ready · override for 1 folder"),
        ("nightly", "Toolchain", "Installed"),
        ("bacon", "cargo install", "Ready · 3.26.0"),
        ("internal-tool", "cargo install", "Ready · 0.4.0"),
        (
            "local-thing",
            "cargo install",
            "Problem · program missing: local-thing",
        ),
    ];
    let names: Vec<_> = i.rust.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, expect.map(|e| e.0));
    for (name, kind, status) in expect {
        let r = row(&i.rust, name);
        assert_eq!(
            (r.kind.as_str(), r.status.as_str()),
            (kind, status),
            "{name}"
        );
    }
    let stable = &row(&i.rust, "stable").detail;
    assert!(
        stable.contains("Targets: aarch64-apple-darwin, wasm32-unknown-unknown"),
        "{stable}"
    );
    assert!(
        stable.contains("Components: cargo, rustc, rust-std-wasm32-unknown-unknown"),
        "{stable}"
    );
    assert!(
        row(&i.rust, "1.80.0")
            .detail
            .contains("Folder overrides: ~/projects/legacy")
    );
    let bacon = &row(&i.rust, "bacon").detail;
    assert!(bacon.contains("Source: crates.io") && bacon.contains("Built with: rustc 1.97.1"));
    assert!(
        row(&i.rust, "internal-tool")
            .detail
            .contains("Source: git (URL withheld)")
    );
    assert!(
        row(&i.rust, "local-thing")
            .detail
            .contains("Source: a local folder (path withheld)")
    );
}

#[test]
fn packages_come_from_each_managers_metadata() {
    let i = collect(&fixture());
    let expect = [
        ("@acme/cli", "npm -g", "Ready · 1.2.3"),
        ("npm", "npm -g", "Ready · 10.9.0"),
        ("ruff", "uv tool", "Ready · 0.9.1"),
        // Minor-version links such as cpython-3.12-… are not separate installs.
        ("cpython 3.12.8", "uv python", "Ready · 3.12.8"),
        ("httpie", "pipx", "Ready · 3.2.4"),
        ("prettier", "bun", "Ready · 3.4.2"),
        ("gopls", "go install", "Ready · version unknown"),
    ];
    let names: Vec<_> = i.packages.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, expect.map(|e| e.0));
    for (name, manager, status) in expect {
        let r = row(&i.packages, name);
        assert_eq!(
            (r.kind.as_str(), r.status.as_str()),
            (manager, status),
            "{name}"
        );
    }
    assert!(row(&i.packages, "ruff").detail.contains("Commands: ruff"));
    assert!(
        row(&i.packages, "ruff")
            .detail
            .contains("Python: python3.12")
    );
}

#[test]
fn withheld_sources_never_reach_the_inventory_or_the_screen() {
    let i = collect(&fixture());
    let mut text: String = [&i.path, &i.rust, &i.packages]
        .iter()
        .flat_map(|rows| rows.iter().map(|r| format!("{r:?}")))
        .collect();
    let mut a = app();
    for _ in 0..3 {
        for _ in 0..10 {
            text.push_str(&draw(&mut a));
            key(&mut a, KeyCode::Down);
        }
        key(&mut a, KeyCode::BackTab);
    }
    assert!(!text.contains("PRIVATE"), "{text}");
}

#[test]
fn nothing_outside_the_fixture_is_read() {
    let mut c = fixture();
    // A real system folder added to PATH is outside the fixture: ignored.
    c.path.insert(0, "/bin".into());
    c.path.insert(0, "/usr/bin".into());
    let i = collect(&c);
    assert_eq!(row(&i.path, "ruby").kind, "macOS system");
    assert!(row(&i.path, "ruby").detail.contains("Runs: /usr/bin/ruby"));
    assert_eq!(i.path.len(), 9);
}

#[test]
fn toolchains_is_the_fifth_area_with_three_sections() {
    let mut a = app();
    assert_eq!(a.module, Module::Toolchains);
    let screen = draw(&mut a);
    assert!(screen.contains("Toolchains  › On PATH"), "{screen}");
    assert!(
        screen.contains("7 commands on PATH · 4 problems"),
        "{screen}"
    );
    key(&mut a, KeyCode::Down);
    key(&mut a, KeyCode::Down);
    let chosen = a.toolchains.selected_row().unwrap().key.clone();
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.toolchains.section, Section::Rust);
    assert!(draw(&mut a).contains("3 toolchains · default stable · 3 crates · 1 problem"));
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.toolchains.section, Section::Packages);
    assert!(draw(&mut a).contains(
        "7 packages · npm -g 2 · uv tool 1 · uv python 1 · pipx 1 · bun 1 · go install 1"
    ));
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.toolchains.section, Section::Path);
    assert_eq!(a.toolchains.selected_row().unwrap().key, chosen);
    // Tab visits all five areas and comes back to the same place.
    for module in [
        Module::Homebrew,
        Module::Agents,
        Module::Cloud,
        Module::Docker,
        Module::Toolchains,
    ] {
        key(&mut a, KeyCode::Tab);
        assert_eq!(a.module, module);
    }
    assert_eq!(a.toolchains.selected_row().unwrap().key, chosen);
}

#[test]
fn an_empty_machine_says_what_is_missing_once_per_section() {
    let root = std::env::temp_dir().join(format!("den-toolchains-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::write(root.join("PATH"), "/usr/bin\n").unwrap();
    let context = toolchains::Context::fixture(&root).unwrap();
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated toolchains fixture",
    );
    let inventory = collect(&context);
    a.toolchains.configure(context);
    a.toolchains.finish(Ok(inventory));
    a.toolchains.requested = false;
    a.navigate(Destination::OnPath);
    for message in [
        "None of the usual language commands is on PATH.",
        "Rust not set up here: no rustup or cargo installs.",
        "No global packages: npm, uv, pipx, bun or go.",
    ] {
        let screen = draw(&mut a);
        assert_eq!(screen.matches(message).count(), 1, "{screen}");
        assert!(screen.lines().nth(1).unwrap().trim().is_empty(), "{screen}");
        key(&mut a, KeyCode::BackTab);
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unreadable_or_invalid_files_are_problems_not_silence() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("den-toolchains-broken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".rustup/toolchains/stable-aarch64-apple-darwin")).unwrap();
    std::fs::create_dir_all(home.join(".cargo")).unwrap();
    let tools = home.join(".local/share/uv/tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(root.join("PATH"), "/usr/bin\n").unwrap();
    std::fs::write(
        home.join(".rustup/settings.toml"),
        "default_toolchain = [unclosed\n",
    )
    .unwrap();
    std::fs::write(home.join(".cargo/.crates2.json"), "{\"installs\": ").unwrap();
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o000)).unwrap();
    // Root can list any folder, so only check the unlistable folder when it really fails.
    let unlistable = std::fs::read_dir(&tools).is_err();

    let i = collect(&toolchains::Context::fixture(&root).unwrap());
    std::fs::set_permissions(&tools, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _ = std::fs::remove_dir_all(&root);

    let status = |rows: &[toolchains::Row], name: &str| row(rows, name).status.clone();
    assert_eq!(
        status(&i.rust, "~/.rustup/settings.toml"),
        "Problem · not valid TOML"
    );
    assert_eq!(
        status(&i.rust, "~/.cargo/.crates2.json"),
        "Problem · not valid JSON"
    );
    // The toolchain is still listed, and the section does not claim Rust is missing.
    assert_eq!(status(&i.rust, "stable"), "Installed");
    if unlistable {
        assert_eq!(
            status(&i.packages, "~/.local/share/uv/tools"),
            "Problem · the folder cannot be listed"
        );
    }
}
