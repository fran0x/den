// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    aws::{self, Kind},
    inventory::Inventory,
    presentation::Destination,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

const FIXTURE: &str = "fixtures/aws";

fn collect(root: &Path) -> aws::Inventory {
    let context = aws::Context::fixture(root).unwrap();
    aws::discover(&context, &AtomicBool::new(false)).unwrap()
}
/// A copy of the fixture with the credentials file at the given mode.
struct Temp(std::path::PathBuf);
impl Temp {
    fn new(name: &str, mode: u32) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("den-aws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home/.aws")).unwrap();
        for f in ["config", "credentials"] {
            std::fs::copy(
                Path::new(FIXTURE).join("home/.aws").join(f),
                root.join("home/.aws").join(f),
            )
            .unwrap();
        }
        std::fs::set_permissions(
            root.join("home/.aws/credentials"),
            std::fs::Permissions::from_mode(mode),
        )
        .unwrap();
        Self(root)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn app(root: &Path) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated AWS fixture",
    );
    a.aws.configure(aws::Context::fixture(root).unwrap());
    a.aws.finish(Ok(collect(root)));
    a.aws.requested = false;
    a.navigate(Destination::Aws);
    a
}
fn key(a: &mut App, code: KeyCode) {
    a.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
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
fn row<'a>(i: &'a aws::Inventory, name: &str, kind: Kind) -> &'a aws::Row {
    i.rows
        .iter()
        .find(|r| r.name == name && r.kind == kind)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn profiles_come_from_both_files_and_auth_follows_sdk_order() {
    let t = Temp::new("auth", 0o600);
    let i = collect(&t.0);
    let profiles: Vec<_> = i
        .rows
        .iter()
        .filter(|r| r.kind == Kind::Profile)
        .map(|r| (r.name.as_str(), r.auth.as_str()))
        .collect();
    assert_eq!(
        profiles,
        [
            ("default", "Static keys"),
            ("base", "Static keys"),
            ("broken", "Role"),
            ("ci", "Role"),
            ("corp", "SSO"),
            ("credentials-only", "Static keys"),
            ("dev", "Role"),
            ("helper", "Process"),
            ("legacy-sso", "SSO"),
            ("local", "None set"),
            ("loop-a", "Role"),
            ("loop-b", "Role"),
        ]
    );
    assert_eq!(
        row(&i, "default", Kind::Profile).status,
        "Ready · eu-west-1"
    );
    let dev = &row(&i, "dev", Kind::Profile).detail;
    assert!(dev.contains("Role: arn:aws:iam::111122223333:role/Developer"));
    assert!(dev.contains("Role chain: dev → base"));
    assert!(dev.contains("MFA device: arn:aws:iam::111122223333:mfa/alice"));
    assert!(dev.contains("external_id: withheld"));
    let corp = &row(&i, "corp", Kind::Profile).detail;
    assert!(corp.contains("Account: 777788889999") && corp.contains("Role: ReadOnly"));
    assert!(
        row(&i, "base", Kind::Profile)
            .detail
            .contains("with a session token")
    );
    assert!(
        row(&i, "ci", Kind::Profile)
            .detail
            .contains("Credential source: Ec2InstanceMetadata")
    );
}

#[test]
fn broken_chains_loops_and_unused_sessions_are_reported() {
    let t = Temp::new("problems", 0o600);
    let i = collect(&t.0);
    assert_eq!(
        row(&i, "broken", Kind::Profile).status,
        "Problem · source_profile 'nowhere' is not defined"
    );
    assert!(
        row(&i, "loop-a", Kind::Profile)
            .status
            .contains("loops back")
    );
    assert_eq!(
        row(&i, "corp", Kind::SsoSession).status,
        "Ready · used by 1 profile"
    );
    assert_eq!(
        row(&i, "unused", Kind::SsoSession).status,
        "Off · used by no profile"
    );
    assert_eq!(i.sso_sessions, 2);
}

#[test]
fn an_open_credentials_file_is_a_problem() {
    let t = Temp::new("open", 0o644);
    let i = collect(&t.0);
    let file = i.rows.iter().find(|r| r.kind == Kind::File).unwrap();
    assert!(file.status.contains("other users can read or change it"));
    let t = Temp::new("closed", 0o600);
    assert!(collect(&t.0).rows.iter().all(|r| r.kind != Kind::File));
}

#[test]
fn private_values_never_reach_the_inventory_or_the_screen() {
    let t = Temp::new("private", 0o600);
    let i = collect(&t.0);
    for r in &i.rows {
        let text = format!("{} {} {} {}", r.name, r.auth, r.status, r.detail);
        assert!(!text.contains("PRIVATE-"), "{text}");
    }
    let mut a = app(&t.0);
    for (w, h) in [(80, 24), (120, 32), (160, 45)] {
        key(&mut a, KeyCode::Home);
        for _ in 0..a.aws.rows().len() {
            loop {
                assert!(!draw(&mut a, w, h).contains("PRIVATE-"));
                if a.aws.panel.offset >= a.aws.panel.max {
                    break;
                }
                key(&mut a, KeyCode::PageDown);
            }
            key(&mut a, KeyCode::Down);
        }
    }
}

#[test]
fn cloud_is_the_third_area_and_keeps_its_place() {
    let t = Temp::new("nav", 0o600);
    let mut a = app(&t.0);
    let screen = draw(&mut a, 120, 32);
    assert!(screen.contains("Cloud  › AWS"));
    assert!(screen.contains("12 profiles · 2 SSO sessions · 3 problems"));
    assert!(screen.contains("active: default (AWS_PROFILE not set)"));
    key(&mut a, KeyCode::Char('/'));
    for c in "role".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Down);
    let selected = a.aws.selected_row().unwrap().key.clone();
    // Shift+Tab goes to Kubernetes and back, keeping AWS's place.
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.destination(), Destination::Kubernetes);
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.destination(), Destination::Aws);
    assert_eq!(a.aws.selected_row().unwrap().key, selected);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.module, Module::Docker);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.module, Module::Toolchains);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.module, Module::Homebrew);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.module, Module::Agents);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Aws);
    assert_eq!(a.aws.query, "role");
    assert_eq!(a.aws.selected_row().unwrap().key, selected);
    key(&mut a, KeyCode::Char('q'));
    assert!(a.quit);
}

#[test]
fn unconfigured_and_tiny_screens_render() {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot",
    );
    a.navigate(Destination::Aws);
    assert!(draw(&mut a, 80, 24).contains("AWS not configured in this mode"));
    key(&mut a, KeyCode::Char('r'));
    assert!(!a.aws.requested);
    let t = Temp::new("tiny", 0o600);
    let mut a = app(&t.0);
    for (w, h) in [(30, 10), (45, 20)] {
        draw(&mut a, w, h);
    }
}
