use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    inventory,
    skills::{self, Config, Scope, Severity, Validity, state::Section},
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "den-skills-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn skill(&self, path: &str, name: &str) {
        self.write(
            &format!("{path}/SKILL.md"),
            &format!(
                "---\nname: {name}\ndescription: Synthetic skill description\n---\nBody text\n"
            ),
        );
    }
    fn collect(&self) -> skills::Inventory {
        skills::discover(&Config::fixture(&self.0).unwrap(), &AtomicBool::new(false)).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> skills::Inventory {
    skills::discover(
        &Config::fixture(Path::new("fixtures/skills")).unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap()
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn app() -> App {
    let mut app = App::new(
        inventory::Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "test",
    );
    app.module = Module::Skills;
    app.skills.config = Some(Config::fixture(Path::new("fixtures/skills")).unwrap());
    app.skills.finish(Ok(fixture()));
    app
}
fn draw(app: &mut App, w: u16, h: u16) -> String {
    let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
    t.draw(|f| ui::render(f, app)).unwrap();
    t.backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect()
}
#[test]
fn fixture_scopes_metadata_mcp_and_files_are_preserved_as_data() {
    let i = fixture();
    assert_eq!(i.skills.len(), 4);
    for scope in [Scope::User, Scope::Project, Scope::Admin, Scope::System] {
        assert_eq!(i.skills.iter().filter(|s| s.scope == scope).count(), 1);
    }
    let alpha = i.skills.iter().find(|s| s.scope == Scope::User).unwrap();
    assert_eq!(alpha.metadata, Validity::Valid);
    assert_eq!(alpha.optional, Validity::Valid);
    assert_eq!(alpha.mcp.len(), 1);
    assert_eq!(alpha.mcp[0].value, "fixture-docs");
    assert!(alpha.instructions.starts_with("# Alpha"));
    assert!(
        alpha
            .files
            .iter()
            .any(|f| f.path == Path::new("scripts/example.sh"))
    );
    assert!(
        alpha
            .findings
            .iter()
            .any(|f| f.message.contains("references/missing.md"))
    );
    assert_eq!(i.skills.iter().filter(|s| s.duplicate).count(), 2);
}
#[test]
fn project_ancestors_stop_at_git_root_and_do_not_scan_siblings() {
    let t = Temp::new();
    fs::create_dir_all(t.0.join("repo/.git")).unwrap();
    fs::create_dir_all(t.0.join("repo/a/b")).unwrap();
    t.skill("repo/.agents/skills/root", "root");
    t.skill("repo/a/.agents/skills/nested", "nested");
    t.skill(".agents/skills/outside", "outside");
    t.skill("repo/sibling/.agents/skills/sibling", "sibling");
    let c = Config::for_context(t.0.join("repo/a/b"), None, None, None).unwrap();
    assert_eq!(c.roots.len(), 3);
    let i = skills::discover(&c, &AtomicBool::new(false)).unwrap();
    assert_eq!(i.skills.len(), 2);
    assert!(i.skills.iter().all(|s| s.scope == Scope::Project));
}
#[test]
fn worktree_git_file_is_a_boundary_and_no_repo_uses_only_cwd() {
    let t = Temp::new();
    t.write("repo/.git", "gitdir: /unread-unrelated-path\n");
    fs::create_dir_all(t.0.join("repo/sub")).unwrap();
    let c = Config::for_context(t.0.join("repo/sub"), None, None, None).unwrap();
    assert_eq!(c.roots.len(), 2);
    let c = Config::for_context(t.0.clone(), None, None, None).unwrap();
    assert_eq!(c.roots.len(), 1);
}
#[test]
fn codex_home_is_a_separate_legacy_and_system_candidate_not_user_home() {
    let t = Temp::new();
    t.skill("home/.agents/skills/user", "user");
    t.skill("codex/skills/old", "old");
    t.skill("codex/skills/.system/builtin", "builtin");
    let c = Config::for_context(
        t.0.clone(),
        Some(t.0.join("home")),
        Some(t.0.join("codex")),
        None,
    )
    .unwrap();
    let i = skills::discover(&c, &AtomicBool::new(false)).unwrap();
    assert_eq!(i.skills.len(), 3);
    assert_eq!(
        i.skills.iter().find(|s| s.name == "old").unwrap().scope,
        Scope::Legacy
    );
    assert!(i.coverage.iter().any(|s| s.contains("activation")));
}
#[test]
fn yaml_errors_required_fields_duplicate_keys_and_unsupported_tags_are_rejected() {
    let t = Temp::new();
    for (n, front) in [
        "name: broken\ndescription: [oops",
        "name: missing",
        "name: numeric\ndescription: 123",
        "name: first\nname: second\ndescription: text",
        "name: include\ndescription: !include /etc/passwd",
    ]
    .iter()
    .enumerate()
    {
        t.write(
            &format!("user/bad{n}/SKILL.md"),
            &format!("---\n{front}\n---\nbody"),
        );
    }
    let i = t.collect();
    assert!(i.skills.iter().all(|s| s.metadata == Validity::Invalid));
    assert!(
        i.skills
            .iter()
            .all(|s| s.findings.iter().any(|f| f.severity == Severity::Error))
    );
}
#[test]
fn bom_crlf_and_multiline_yaml_work_without_truncating_instructions() {
    let t = Temp::new();
    t.write("user/good/SKILL.md","\u{feff}---\r\nname: good\r\ndescription: |\r\n  First line\r\n  Second line\r\n---\r\nComplete body\r\nEND");
    let i = t.collect();
    assert_eq!(i.skills[0].metadata, Validity::Valid);
    assert_eq!(i.skills[0].instructions, "Complete body\r\nEND");
    assert!(i.skills[0].description.contains("Second line"));
}
#[test]
fn missing_skill_md_empty_directories_and_stray_files_are_findings() {
    let t = Temp::new();
    fs::create_dir_all(t.0.join("user/empty")).unwrap();
    t.write("user/stray.txt", "not a skill");
    let i = t.collect();
    assert_eq!(i.skills.len(), 2);
    assert!(i.skills.iter().all(|s| !s.findings.is_empty()));
    let empty = i.skills.iter().find(|s| s.name == "empty").unwrap();
    assert_eq!(empty.metadata, Validity::Invalid);
    assert!(empty.files.is_empty());
}
#[test]
fn duplicate_names_include_invalid_metadata_without_inferring_conflict() {
    let t = Temp::new();
    t.skill("user/a", "same");
    t.write("project/b/SKILL.md", "---\nname: same\n---\nbody");
    let i = t.collect();
    assert!(i.skills.iter().all(|s| s.duplicate));
    assert!(
        i.skills
            .iter()
            .flat_map(|s| &s.findings)
            .any(|f| f.severity == Severity::Info && f.message.contains("No conflict"))
    );
}
#[test]
fn invalid_optional_metadata_does_not_invalidate_skill_frontmatter() {
    let t = Temp::new();
    t.skill("user/a", "a");
    t.write("user/a/agents/openai.yaml","interface:\n  display_name: [wrong]\npolicy:\n  allow_implicit_invocation: text\ndependencies:\n  tools: wrong\n");
    let i = t.collect();
    assert_eq!(i.skills[0].metadata, Validity::Valid);
    assert_eq!(i.skills[0].optional, Validity::Invalid);
}

#[test]
fn mcp_metadata_urls_are_withheld_before_inventory_publication() {
    for url in [
        "https://user:URL-REDACTION-CANARY@URL-REDACTION-CANARY.invalid/URL-REDACTION-CANARY?key=URL-REDACTION-CANARY#URL-REDACTION-CANARY",
        "http://127.0.0.1:8080/URL-REDACTION-CANARY",
        "https://example.invalid/percent%2Dencoded?key=URL%2DREDACTION%2DCANARY",
        "file:///private/URL-REDACTION-CANARY",
        "not a URL: URL-REDACTION-CANARY",
        "https://example.invalid/URL-REDACTION-CANARY\n\u{1b}[31m",
        "",
    ] {
        let t = Temp::new();
        t.skill("user/docs", "docs");
        let metadata = format!(
            "dependencies:\n  tools:\n    - type: mcp\n      value: docs\n      transport: streamable_http\n      description: Declared only\n      url: {}\n",
            serde_json::to_string(url).unwrap()
        );
        t.write("user/docs/agents/openai.yaml", &metadata);
        let inventory = t.collect();
        let skill = &inventory.skills[0];
        assert_eq!(skill.metadata, Validity::Valid);
        // Redaction does not introduce URL validation or change metadata eligibility.
        assert_eq!(skill.optional, Validity::Valid);
        assert_eq!(skill.mcp.len(), 1);
        let dependency = &skill.mcp[0];
        assert_eq!(dependency.value, "docs");
        assert_eq!(dependency.transport.as_deref(), Some("streamable_http"));
        assert_eq!(dependency.description.as_deref(), Some("Declared only"));
        assert_eq!(dependency.url.as_deref(), Some("[URL withheld]"));
        let debug = format!("{inventory:?}");
        assert!(!debug.contains("URL-REDACTION-CANARY"));
        assert!(!debug.contains("URL%2DREDACTION%2DCANARY"));
        assert!(!debug.contains("https://") && !debug.contains("http://"));
        let source = skill
            .files
            .iter()
            .find(|f| f.path == Path::new("agents/openai.yaml"))
            .unwrap();
        assert!(
            source
                .text
                .as_deref()
                .unwrap()
                .contains("Raw optional metadata withheld")
        );
        assert_eq!(
            fs::read_to_string(t.0.join("user/docs/agents/openai.yaml")).unwrap(),
            metadata
        );
    }

    let t = Temp::new();
    t.skill("user/docs", "docs");
    t.write(
        "user/docs/agents/openai.yaml",
        "dependencies:\n  tools:\n    - type: mcp\n      value: docs\n",
    );
    let inventory = t.collect();
    assert_eq!(inventory.skills[0].optional, Validity::Valid);
    assert_eq!(inventory.skills[0].mcp[0].url, None);
}

#[test]
fn invalid_optional_metadata_does_not_expose_urls_in_source_previews() {
    for text in [
        "dependencies:\n  tools:\n    - type: mcp\n      value: docs\n      url: 'https://example.invalid/URL-REDACTION-CANARY\n",
        "dependencies:\n  tools:\n    - type: mcp\n      value: docs\n      url: [https://example.invalid/URL-REDACTION-CANARY]\n",
    ] {
        let t = Temp::new();
        t.skill("user/docs", "docs");
        t.write("user/docs/agents/openai.yaml", text);
        let inventory = t.collect();
        assert_eq!(inventory.skills[0].metadata, Validity::Valid);
        assert_eq!(inventory.skills[0].optional, Validity::Invalid);
        assert!(inventory.skills[0].mcp.is_empty());
        assert!(!format!("{inventory:?}").contains("URL-REDACTION-CANARY"));
    }
}

#[test]
fn mcp_url_redaction_is_visible_in_evidence_and_metadata_source_preview() {
    let t = Temp::new();
    t.skill("user/docs", "docs");
    t.write("user/docs/agents/openai.yaml", "dependencies:\n  tools:\n    - type: mcp\n      value: docs\n      transport: streamable_http\n      url: https://user:URL-REDACTION-CANARY@example.invalid/private?key=URL-REDACTION-CANARY\n");
    let mut a = app();
    a.skills.configure(Config::fixture(&t.0).unwrap());
    a.skills.finish(Ok(t.collect()));
    a.skills.requested = false;
    for (width, height) in [(80, 24), (120, 32), (160, 45)] {
        key(&mut a, KeyCode::Char('v'));
        assert!(!draw(&mut a, width, height).contains("URL-REDACTION-CANARY"));
        key(&mut a, KeyCode::End);
        let evidence = draw(&mut a, width, height);
        assert!(evidence.contains("docs") && evidence.contains("streamable_http"));
        assert!(evidence.contains("[URL withheld]"));
        assert!(!evidence.contains("URL-REDACTION-CANARY"));

        key(&mut a, KeyCode::Char('3'));
        a.skills.file = a
            .skills
            .selected_skill()
            .unwrap()
            .files
            .iter()
            .position(|f| f.path == Path::new("agents/openai.yaml"))
            .unwrap();
        key(&mut a, KeyCode::Enter);
        let preview = draw(&mut a, width, height);
        assert!(a.skills.file_open);
        assert!(preview.contains("Raw optional metadata withheld"));
        assert!(!preview.contains("URL-REDACTION-CANARY"));
    }
}

#[test]
fn large_text_binary_data_and_depth_limits_are_explicit() {
    let t = Temp::new();
    t.write(
        "user/large/SKILL.md",
        &"x".repeat(skills::MAX_INSTRUCTIONS + 1),
    );
    t.skill("user/good", "good");
    t.write("user/good/asset.bin", "abc\0def");
    t.write(
        "user/good/references/huge.txt",
        &"x".repeat(skills::MAX_FILE + 1),
    );
    t.write("user/good/a/b/c/d/e/f/g/h/i/j/hidden.txt", "not scanned");
    let i = t.collect();
    let large = i.skills.iter().find(|s| s.name == "large").unwrap();
    assert_eq!(large.metadata, Validity::NotEvaluated);
    assert!(large.instructions.is_empty());
    let good = i.skills.iter().find(|s| s.name == "good").unwrap();
    assert!(
        good.files
            .iter()
            .find(|f| f.path == Path::new("asset.bin"))
            .unwrap()
            .text
            .is_none()
    );
    assert!(!good.files.iter().any(|f| f.path.ends_with("hidden.txt")));
    assert!(good.incomplete);
    assert!(
        good.findings
            .iter()
            .any(|f| f.message.contains("depth limit"))
    );
}
#[test]
fn metadata_recursion_and_alias_budgets_fail_safely() {
    let t = Temp::new();
    let text = format!(
        "---\nname: bad\ndescription: text\nextra: {}0{}\n---\nbody",
        "[".repeat(100),
        "]".repeat(100)
    );
    t.write("user/deep/SKILL.md", &text);
    let i = t.collect();
    assert_eq!(i.skills[0].metadata, Validity::Invalid);
}
#[test]
fn cancelled_collection_is_not_an_empty_success() {
    let t = Temp::new();
    let c = Config::fixture(&t.0).unwrap();
    assert!(skills::discover(&c, &AtomicBool::new(true)).is_err());
}
#[cfg(unix)]
#[test]
fn external_folder_symlinks_are_labeled_but_external_supporting_files_are_not_read() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    t.skill("outside/actual", "linked");
    fs::create_dir_all(t.0.join("user")).unwrap();
    symlink(t.0.join("outside/actual"), t.0.join("user/link")).unwrap();
    t.write("secret.txt", "DO_NOT_READ");
    symlink(t.0.join("secret.txt"), t.0.join("outside/actual/secret.md")).unwrap();
    symlink("SKILL.md", t.0.join("outside/actual/internal.md")).unwrap();
    let i = t.collect();
    assert_eq!(i.skills.len(), 1);
    let s = &i.skills[0];
    assert_eq!(s.metadata, Validity::Valid);
    assert!(
        s.findings
            .iter()
            .any(|f| f.message.contains("External skill-folder"))
    );
    assert!(
        s.files
            .iter()
            .find(|f| f.path == Path::new("secret.md"))
            .unwrap()
            .text
            .is_none()
    );
    assert!(
        s.files
            .iter()
            .find(|f| f.path == Path::new("internal.md"))
            .unwrap()
            .text
            .is_some()
    );
    assert!(
        !s.files
            .iter()
            .filter_map(|f| f.text.as_ref())
            .any(|s| s.contains("DO_NOT_READ"))
    );
}
#[cfg(unix)]
#[test]
fn broken_links_cycles_and_links_to_non_skill_directories_do_not_expand() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    fs::create_dir_all(t.0.join("user")).unwrap();
    t.write("unrelated/deep/private.txt", "secret");
    symlink("missing", t.0.join("user/broken")).unwrap();
    symlink("cycle", t.0.join("user/cycle")).unwrap();
    symlink(t.0.join("unrelated"), t.0.join("user/unrelated")).unwrap();
    let i = t.collect();
    assert_eq!(i.skills.len(), 3);
    assert!(
        i.skills
            .iter()
            .all(|s| s.files.is_empty() && !s.findings.is_empty())
    );
}
#[cfg(unix)]
#[test]
fn unreadable_files_and_executable_bits_are_reported_without_running_anything() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    t.skill("user/a", "a");
    t.write("user/a/scripts/run", "touch /not-executed");
    fs::set_permissions(
        t.0.join("user/a/scripts/run"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    t.write("user/a/private.md", "private");
    fs::set_permissions(
        t.0.join("user/a/private.md"),
        fs::Permissions::from_mode(0o0),
    )
    .unwrap();
    let i = t.collect();
    let s = &i.skills[0];
    assert!(
        s.files
            .iter()
            .find(|f| f.path == Path::new("scripts/run"))
            .unwrap()
            .executable
    );
    let inaccessible = s
        .files
        .iter()
        .find(|f| f.path == Path::new("private.md"))
        .unwrap();
    if unsafe { libc::geteuid() } != 0 {
        assert!(inaccessible.text.is_none());
    }
    fs::set_permissions(
        t.0.join("user/a/private.md"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}
#[cfg(unix)]
#[test]
fn special_files_are_never_opened_for_reading() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let t = Temp::new();
    t.skill("user/a", "a");
    let name = CString::new(t.0.join("user/a/pipe").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let i = t.collect();
    assert!(
        i.skills[0]
            .files
            .iter()
            .find(|f| f.path == Path::new("pipe"))
            .unwrap()
            .text
            .is_none()
    );
}
#[test]
fn skills_search_filters_and_module_switch_preserve_homebrew_state() {
    let mut a = app();
    a.query = "preserved brew search".into();
    key(&mut a, KeyCode::Char('/'));
    for ch in "synthetic evidence".chars() {
        key(&mut a, KeyCode::Char(ch));
    }
    key(&mut a, KeyCode::Enter);
    assert_eq!(a.skills.visible.len(), 1);
    key(&mut a, KeyCode::F(2));
    assert_eq!(a.query, "preserved brew search");
    key(&mut a, KeyCode::F(2));
    assert_eq!(a.skills.query, "synthetic evidence");
    key(&mut a, KeyCode::Char('/'));
    a.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    key(&mut a, KeyCode::Enter);
    key(&mut a, KeyCode::Char('f'));
    assert_eq!(a.skills.scope, Some(Scope::User));
    key(&mut a, KeyCode::Char('i'));
    assert_eq!(a.skills.visible.len(), 1);
}
#[test]
fn file_navigation_full_instructions_and_back_are_scrollable_and_sanitized() {
    let mut a = app();
    a.skills.scope = Some(Scope::User);
    a.skills.refresh_visible();
    key(&mut a, KeyCode::Char('3'));
    draw(&mut a, 100, 30);
    key(&mut a, KeyCode::End);
    let selected = a.skills.file;
    key(&mut a, KeyCode::Enter);
    assert!(a.skills.file_open);
    key(&mut a, KeyCode::Backspace);
    assert_eq!(a.skills.file, selected);
    assert!(!a.skills.file_open);
    key(&mut a, KeyCode::Char('2'));
    let n = a.skills.visible[0];
    a.skills.inventory.skills[n].instructions = format!(
        "\x1b]52;bad\x07\u{202e}\n{}\nFINAL LINE",
        "line\n".repeat(100)
    );
    for (w, h) in [(120, 30), (50, 20), (30, 10), (12, 5)] {
        let text = draw(&mut a, w, h);
        assert!(!text.contains('\x1b'));
        assert!(!text.contains('\u{202e}'));
    }
    draw(&mut a, 50, 20);
    key(&mut a, KeyCode::End);
    assert!(draw(&mut a, 50, 20).contains("FINAL LINE"));
}
#[test]
fn refresh_preserves_skill_filter_selection_file_and_stale_data_on_failure() {
    let mut a = app();
    a.skills.scope = Some(Scope::User);
    a.skills.refresh_visible();
    a.skills.section = Section::Files;
    a.skills.file = 1;
    a.skills.file_open = true;
    let path = a.skills.selected_skill().unwrap().path.clone();
    let file = a.skills.selected_skill().unwrap().files[1].path.clone();
    a.skills.finish(Ok(fixture()));
    assert_eq!(a.skills.selected_skill().unwrap().path, path);
    assert_eq!(
        a.skills.selected_skill().unwrap().files[a.skills.file].path,
        file
    );
    assert!(a.skills.file_open);
    a.skills.finish(Err(io::Error::other("test")));
    assert_eq!(a.skills.inventory.skills.len(), 4);
    assert!(a.skills.status().contains("STALE"));
}
#[test]
fn snapshots_and_unconfigured_states_never_request_filesystem_collection() {
    let mut a = App::new(
        inventory::Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot",
    );
    key(&mut a, KeyCode::F(2));
    key(&mut a, KeyCode::Char('r'));
    assert!(!a.skills.requested);
    assert!(a.skills.config.is_none());
    assert!(draw(&mut a, 120, 30).contains("no filesystem discovery"));
}
#[test]
fn empty_inventory_coverage_help_and_search_shortcuts_are_consistent() {
    let mut a = app();
    a.skills.finish(Ok(skills::Inventory::default()));
    for ch in ['1', '2', '3', '4', '5', '?'] {
        key(&mut a, KeyCode::Char(ch));
        draw(&mut a, 30, 10);
    }
    key(&mut a, KeyCode::Esc);
    key(&mut a, KeyCode::Char('/'));
    key(&mut a, KeyCode::Char('r'));
    assert!(
        matches!(&a.presentation.page,den::presentation::Page::Inspector{query,..} if query=="r")
    );
    assert!(!a.skills.requested);
}

#[test]
#[ignore = "opt-in local filesystem discovery; prints counts and duplicate names only, never instructions or paths"]
fn live_skills_inventory() {
    let c = Config::local(std::env::current_dir().unwrap()).unwrap();
    let i = skills::discover(&c, &AtomicBool::new(false)).unwrap();
    println!(
        "Skills discovered: {}; traversal partial: {}",
        i.skills.len(),
        i.partial
    );
    for scope in Scope::ALL {
        println!(
            "{}: {}",
            scope.label(),
            i.skills.iter().filter(|s| s.scope == scope).count()
        );
    }
    for status in [Validity::Valid, Validity::Invalid, Validity::NotEvaluated] {
        println!(
            "Metadata {}: {}",
            status.label(),
            i.skills.iter().filter(|s| s.metadata == status).count()
        );
    }
    let duplicates: std::collections::BTreeSet<_> = i
        .skills
        .iter()
        .filter(|s| s.duplicate)
        .map(|s| inventory::safe_text(&s.name))
        .collect();
    println!("Duplicate declared names: {duplicates:?}");
    println!(
        "Skill directories inaccessible: {}",
        i.skills.iter().filter(|s| !s.accessible).count()
    );
    for scope in Scope::ALL {
        for root in i.locations.iter().filter(|r| r.scope == scope) {
            println!(
                "{} root: {}",
                scope.label(),
                if root.status.starts_with("Accessible") {
                    "Accessible"
                } else {
                    root.status.as_str()
                }
            );
        }
    }
    for severity in [Severity::Error, Severity::Warning, Severity::Info] {
        println!(
            "{severity:?} findings: {}",
            i.skills
                .iter()
                .flat_map(|s| &s.findings)
                .filter(|f| f.severity == severity)
                .count()
        );
    }
    println!(
        "Optional metadata invalid: {}",
        i.skills
            .iter()
            .filter(|s| s.optional == Validity::Invalid)
            .count()
    );
    println!(
        "Missing supporting references: {}",
        i.skills
            .iter()
            .flat_map(|s| &s.findings)
            .filter(|f| f.message.starts_with("Referenced file not observed"))
            .count()
    );
    println!(
        "Coverage: plugins/session skills unavailable; system and legacy caches do not establish activation; config not evaluated."
    );
}

#[test]
fn alias_expansion_and_display_line_limits_are_bounded() {
    let t = Temp::new();
    let mut front = String::from("---\nname: aliases\ndescription: text\nv0: &v0 [a]\n");
    for n in 1..10 {
        front.push_str(&format!(
            "v{n}: &v{n} [{}]\n",
            vec![format!("*v{}", n - 1); 20].join(",")
        ));
    }
    front.push_str("---\nbody");
    t.write("user/aliases/SKILL.md", &front);
    t.write("user/lines/SKILL.md", &"line\n".repeat(20_001));
    let i = t.collect();
    assert_eq!(
        i.skills
            .iter()
            .find(|s| s.name == "aliases")
            .unwrap()
            .metadata,
        Validity::Invalid
    );
    assert!(
        i.skills
            .iter()
            .find(|s| s.name == "lines")
            .unwrap()
            .instructions
            .is_empty()
    );
}
#[test]
fn oversized_directory_keeps_required_metadata_and_reports_partial_coverage() {
    let t = Temp::new();
    t.skill("user/many", "many");
    for n in 0..550 {
        t.write(&format!("user/many/file-{n}"), "test");
    }
    let i = t.collect();
    assert_eq!(i.skills[0].metadata, Validity::Valid);
    assert!(i.skills[0].files.len() <= skills::MAX_FILES);
    assert!(i.skills[0].incomplete);
    assert!(i.partial);
}
#[cfg(unix)]
#[test]
fn inaccessible_roots_are_not_presented_as_a_successful_empty_refresh() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    fs::create_dir_all(t.0.join("restricted")).unwrap();
    fs::set_permissions(t.0.join("restricted"), fs::Permissions::from_mode(0o0)).unwrap();
    let config = Config {
        project: t.0.clone(),
        roots: vec![skills::Root {
            path: t.0.join("restricted"),
            scope: Scope::User,
        }],
        coverage: vec![],
    };
    if unsafe { libc::geteuid() } != 0 {
        assert!(skills::discover(&config, &AtomicBool::new(false)).is_err());
    }
    fs::set_permissions(t.0.join("restricted"), fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn markdown_examples_are_not_treated_as_missing_supporting_files() {
    let t = Temp::new();
    t.write("user/example/SKILL.md","---\nname: example\ndescription: example\n---\n```md\n[example](missing-example.md)\n```\n`[inline](missing-inline.md)`\n[real](missing-real.md)\n");
    let i = t.collect();
    let findings = &i.skills[0].findings;
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.message.starts_with("Referenced file"))
            .count(),
        1
    );
    assert!(
        findings
            .iter()
            .any(|f| f.message.contains("missing-real.md"))
    );
}
