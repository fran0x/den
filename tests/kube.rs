// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Cloud › Kubernetes: contexts from kubeconfig files, with secrets withheld.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, CloudSection, Module},
    inventory::Inventory,
    kube,
    presentation::Destination,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

const FIXTURE: &str = "fixtures/kube/home/.kube/config";

/// A temporary home with kubeconfig files at mode 600 (checkouts may widen the fixture's).
struct Home(PathBuf);
impl Home {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!("den-kube-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home/.kube")).unwrap();
        for (file, text) in files {
            let path = root.join("home/.kube").join(file);
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        Self(root)
    }
    fn fixture(name: &str) -> Self {
        Self::new(
            name,
            &[("config", &std::fs::read_to_string(FIXTURE).unwrap())],
        )
    }
    fn context(&self, files: &[&str]) -> kube::Context {
        let mut c = kube::Context::fixture(&self.0).unwrap();
        if !files.is_empty() {
            c.files = files.iter().map(|f| c.files[0].with_file_name(f)).collect();
            c.from_env = true;
        }
        c
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn collect(context: &kube::Context) -> kube::Inventory {
    kube::discover(context, &AtomicBool::new(false)).unwrap()
}
fn app(context: kube::Context) -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated Kubernetes fixture",
    );
    let inventory = collect(&context);
    a.kube.configure(context);
    a.kube.finish(Ok(inventory));
    a.kube.requested = false;
    a.navigate(Destination::Kubernetes);
    a
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
fn row<'a>(i: &'a kube::Inventory, name: &str) -> &'a kube::Row {
    i.rows.iter().find(|r| r.name == name).unwrap()
}

#[test]
fn every_context_gets_its_auth_and_status() {
    let home = Home::fixture("rows");
    let i = collect(&home.context(&[]));
    assert!(i.found);
    assert_eq!(i.contexts, 7);
    assert_eq!(i.current.as_deref(), Some("prod-eks"));
    let expect = [
        ("docker-desktop", "Client cert", "Ready · default"),
        ("prod-eks", "Exec · aws", "Ready · current · payments"),
        (
            "staging-gke",
            "gcp provider",
            "Problem · the gcp auth provider was removed",
        ),
        ("legacy", "Token", "Problem · the server uses plain http"),
        (
            "lab",
            "Basic auth",
            "Problem · a password is stored in the kubeconfig",
        ),
        (
            "orphan",
            "Client cert",
            "Problem · cluster 'gone' is not defined",
        ),
        ("minikube", "Client cert", "Ready · default"),
    ];
    for (name, auth, status) in expect {
        let r = row(&i, name);
        assert_eq!(r.auth, auth, "{name}");
        assert!(r.status.starts_with(status), "{name}: {}", r.status);
    }
    // TLS verification off is reported too, after the first problem.
    assert!(row(&i, "legacy").detail.contains("TLS verification is off"));
    let coverage = i.coverage.join("\n");
    assert!(
        coverage.contains("Not used by any context: 1 cluster and 1 user"),
        "{coverage}"
    );
}

#[test]
fn secrets_hosts_paths_and_exec_arguments_never_reach_the_inventory_or_screen() {
    let home = Home::fixture("privacy");
    let i = collect(&home.context(&[]));
    let mut text = i
        .rows
        .iter()
        .map(|r| format!("{r:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str(&i.coverage.join("\n"));
    let mut a = app(home.context(&[]));
    for _ in 0..i.rows.len() {
        text.push_str(&draw(&mut a, 140, 40));
        a.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    }
    assert!(!text.contains("PRIVATE"), "{text}");
    let eks = row(&i, "prod-eks").detail.clone();
    for shown in [
        "Server: https · host name · port 443 · host withheld",
        "Certificate authority: embedded · withheld",
        "Proxy: set · URL withheld",
        "Auth: exec plugin · aws (runs when kubectl connects; not run)",
        "Env (names only): AWS_PROFILE",
        "API version: client.authentication.k8s.io/v1beta1",
    ] {
        assert!(eks.contains(shown), "{shown}\n{eks}");
    }
    assert!(
        row(&i, "docker-desktop")
            .detail
            .contains("Server: https · this machine · port 6443")
    );
    assert!(
        row(&i, "minikube")
            .detail
            .contains("private network address · port 8443")
    );
}

#[test]
fn with_several_files_the_first_definition_wins() {
    let home = Home::new(
        "merge",
        &[
            (
                "a",
                "current-context: one\ncontexts:\n- name: one\n  context: {cluster: c, user: u, namespace: first}\nclusters:\n- name: c\n  cluster: {server: 'https://127.0.0.1'}\nusers:\n- name: u\n  user: {token: PRIVATE}\n",
            ),
            (
                "b",
                "current-context: two\ncontexts:\n- name: one\n  context: {cluster: c, user: u, namespace: second}\n- name: two\n  context: {cluster: c, user: u}\n",
            ),
        ],
    );
    let i = collect(&home.context(&["a", "b", "missing"]));
    assert_eq!(i.current.as_deref(), Some("one"));
    assert_eq!(i.contexts, 2);
    assert!(row(&i, "one").status.ends_with("first"));
    assert!(row(&i, "two").detail.contains("Defined in: ~/.kube/b"));
    let coverage = i.coverage.join("\n");
    assert!(coverage.contains("1 entry ignored"), "{coverage}");
    assert!(coverage.contains("from KUBECONFIG"), "{coverage}");
}

#[test]
fn file_problems_are_rows_and_other_files_still_count() {
    let home = Home::new(
        "problems",
        &[
            ("broken", "contexts: [unclosed\n"),
            (
                "ok",
                "current-context: nowhere\ncontexts:\n- name: x\n  context: {cluster: c}\nclusters:\n- name: c\n  cluster: {server: 'https://127.0.0.1'}\n",
            ),
        ],
    );
    std::fs::set_permissions(
        home.0.join("home/.kube/ok"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let i = collect(&home.context(&["broken", "ok"]));
    let statuses: Vec<_> = i.rows.iter().map(|r| r.status.as_str()).collect();
    assert!(
        statuses.contains(&"Problem · not a valid kubeconfig"),
        "{statuses:?}"
    );
    assert!(
        statuses.contains(&"Problem · other users can read or change it (expected 600)"),
        "{statuses:?}"
    );
    assert!(
        statuses.contains(&"Problem · 'nowhere' is not defined"),
        "{statuses:?}"
    );
    assert_eq!(row(&i, "x").auth, "None set");
}

#[test]
fn without_a_kubeconfig_it_says_not_set_up_once() {
    let home = Home::new("missing", &[]);
    let mut a = app(home.context(&[]));
    let screen = draw(&mut a, 140, 30);
    assert_eq!(
        screen
            .matches("Kubernetes not set up here: no ~/.kube/config.")
            .count(),
        1,
        "{screen}"
    );
    assert!(screen.lines().nth(1).unwrap().trim().is_empty(), "{screen}");
    assert!(!screen.contains("0 contexts") && !screen.contains("Context "));
}

#[test]
fn shift_tab_switches_cloud_sections_and_each_keeps_its_place() {
    let home = Home::fixture("nav");
    let mut a = app(home.context(&[]));
    assert_eq!(a.module, Module::Cloud);
    let screen = draw(&mut a, 140, 30);
    assert!(screen.contains("Cloud  › Kubernetes"), "{screen}");
    assert!(
        screen.contains("7 contexts · 4 problems · current: prod-eks"),
        "{screen}"
    );
    a.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    a.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let chosen = a.kube.selected_row().unwrap().key.clone();
    a.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(a.cloud, CloudSection::Aws);
    assert!(draw(&mut a, 140, 30).contains("Cloud  › AWS"));
    a.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(a.cloud, CloudSection::Kubernetes);
    assert_eq!(a.kube.selected_row().unwrap().key, chosen);
    // Tab away and back returns to Kubernetes, not AWS.
    for _ in 0..5 {
        a.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert_eq!(
        (a.module, a.cloud),
        (Module::Cloud, CloudSection::Kubernetes)
    );
}

#[test]
fn the_fixture_reads_nothing_outside_its_root() {
    let home = Home::fixture("boundary");
    let mut c = home.context(&[]);
    c.files.push(Path::new("/etc/hosts").to_path_buf());
    let i = collect(&c);
    assert_eq!(i.contexts, 7);
    assert!(!i.rows.iter().any(|r| r.name.contains("hosts")));
}
