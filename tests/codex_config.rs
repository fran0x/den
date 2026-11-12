// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    areas::agents::codex::{
        self, Context,
        config::{self, Activity, Trust},
        instructions::Status,
    },
    inventory,
    tui::presentation::Destination,
    tui::ui,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "den-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("project/.git")).unwrap();
        Self(fs::canonicalize(p).unwrap())
    }
    fn write(&self, p: &str, s: &str) {
        let p = self.0.join(p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }
    fn context(&self) -> Context {
        Context::fixture(&self.0, None).unwrap()
    }
    fn config(&self) -> config::Configuration {
        config::collect(&self.context(), &AtomicBool::new(false)).unwrap()
    }
    fn collect(&self) -> codex::Inventory {
        codex::discover(&self.context(), &AtomicBool::new(false)).unwrap()
    }
    fn trust(&self, level: &str, extra: &str) {
        self.write(
            "home/.codex/config.toml",
            &format!(
                "{extra}\n[projects.\"{}\"]\ntrust_level = \"{level}\"\n",
                self.0.join("project").display()
            ),
        );
    }
    fn skill(&self, p: &str) {
        self.write(
            &format!("{p}/SKILL.md"),
            "---\nname: example\ndescription: Synthetic example\n---\nRead only example.\n",
        );
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn value(c: &config::Configuration, key: &str) -> String {
    c.settings
        .iter()
        .find(|s| s.key == key)
        .unwrap()
        .value
        .clone()
}
fn key(a: &mut App, k: KeyCode) {
    a.handle_key(KeyEvent::new(k, KeyModifiers::NONE));
}
fn app(t: &Temp) -> App {
    let mut a = App::new(
        inventory::Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "test",
    );
    a.module = Module::Agents;
    a.codex.configure(t.context());
    a.codex.finish(Ok(t.collect()), &mut a.skills);
    a.codex.requested = false;
    a
}

/// Names of the rows the Agents area shows in its current section.
fn rows(a: &App) -> Vec<String> {
    let sources = a.agent_sources();
    a.agents
        .rows(&sources)
        .into_iter()
        .map(|r| r.name)
        .collect()
}
fn search(a: &mut App, query: &str) {
    a.agents.query = query.into();
    a.agents_clamp();
}
fn draw(a: &mut App, width: u16, height: u16) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::render(frame, a)).unwrap();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn incomplete_configuration_is_provisional_in_inventory_and_overview() {
    for failure in ["invalid TOML", "unavailable", "missing profile"] {
        let t = Temp::new();
        t.trust("trusted", "sandbox_mode='read-only'");
        match failure {
            "invalid TOML" => t.write(
                "project/.codex/config.toml",
                "secret='PRIVATE-INCOMPLETE-SENTINEL\n",
            ),
            "unavailable" => {
                t.write("project/.codex/config.toml", "");
                fs::write(t.0.join("project/.codex/config.toml"), [255]).unwrap();
            }
            "missing profile" => (),
            _ => unreachable!(),
        }
        let mut a = app(&t);
        if failure == "missing profile" {
            let mut context = t.context();
            context.profile = Some("absent".into());
            a.codex.configure(context.clone());
            a.codex.finish(
                codex::discover(&context, &AtomicBool::new(false)),
                &mut a.skills,
            );
            a.codex.requested = false;
        }
        a.navigate(den::tui::presentation::Destination::Settings);
        let c = &a.codex.inventory.as_ref().unwrap().configuration;
        assert_eq!(value(c, "sandbox_mode"), "read-only");
        assert_eq!(value(c, "project_doc_max_bytes"), "32768");
        let setting = c
            .settings
            .iter()
            .position(|s| s.key == "sandbox_mode")
            .unwrap();
        assert_eq!(
            c.layers[c.settings[setting].source.unwrap()].scope,
            config::Scope::User
        );
        assert!(
            c.setting_text(setting)
                .contains("Provisional value: read-only")
        );
        {
            let sources = a.agent_sources();
            let section = den::areas::agents::Section::Settings;
            assert!(
                sources
                    .rows(section)
                    .iter()
                    .filter(|r| !r.codex.is_empty())
                    .all(|r| sources.cells(section, r).1.starts_with("Provisional · "))
            );
        }
        for (width, height) in [(80, 24), (120, 32), (160, 45)] {
            for setting in ["sandbox_mode", "project_doc_max_bytes"] {
                search(&mut a, setting);
                let screen = draw(&mut a, width, height);
                assert!(
                    screen
                        .lines()
                        .nth(1)
                        .unwrap()
                        .contains("INCOMPLETE configuration"),
                    "{failure}, {width}"
                );
                assert!(
                    screen
                        .lines()
                        // The search narrows the list to this setting; it is the selected row.
                        .find(|line| line.starts_with("> ") && !line.contains('─'))
                        .unwrap()
                        .contains("Provi"),
                    "{failure}, {width}, {setting}"
                );
                // The panel shows the value; there is no inspector.
                assert!(
                    screen.contains("Provisional value:"),
                    "{failure}, {width}, {setting}"
                );
                assert!(
                    screen.contains("Configuration incomplete"),
                    "{failure}, {width}, {setting}"
                );
                assert!(!screen.contains("Locally resolved:"));
                assert!(!screen.contains("PRIVATE-INCOMPLETE-SENTINEL"));
            }
        }
    }
}

#[test]
fn complete_and_inactive_configuration_keep_normal_presentation() {
    for (trust, project, expected) in [
        ("trusted", None, "read-only"),
        (
            "trusted",
            Some("sandbox_mode='workspace-write'"),
            "workspace-write",
        ),
        (
            "untrusted",
            Some("secret='PRIVATE-INCOMPLETE-SENTINEL\n"),
            "read-only",
        ),
    ] {
        let t = Temp::new();
        t.trust(trust, "sandbox_mode='read-only'");
        if let Some(project) = project {
            t.write("project/.codex/config.toml", project);
        }
        let mut a = app(&t);
        a.navigate(den::tui::presentation::Destination::Settings);
        search(&mut a, "sandbox_mode");
        assert_eq!(
            value(
                &a.codex.inventory.as_ref().unwrap().configuration,
                "sandbox_mode"
            ),
            expected
        );
        for (width, height) in [(80, 24), (120, 32)] {
            let screen = draw(&mut a, width, height);
            assert!(!screen.contains("INCOMPLETE configuration"));
            assert!(!screen.contains("Provisional ·"));
            assert!(screen.contains(&format!("Locally resolved: {expected}")));
            assert!(!screen.contains("Configuration incomplete"));
        }
    }
}

#[test]
fn configuration_refresh_updates_provisional_presentation() {
    let t = Temp::new();
    t.trust("trusted", "sandbox_mode='read-only'");
    t.write("project/.codex/config.toml", "sandbox_mode=[invalid");
    let mut a = app(&t);
    a.navigate(den::tui::presentation::Destination::Settings);
    search(&mut a, "sandbox_mode");
    assert!(draw(&mut a, 80, 24).contains("Provisional value: read-only"));

    t.write(
        "project/.codex/config.toml",
        "sandbox_mode='workspace-write'",
    );
    a.finish_codex(Ok(t.collect()));
    let repaired = draw(&mut a, 80, 24);
    assert!(repaired.contains("Locally resolved: workspace-write"));
    assert!(!repaired.contains("INCOMPLETE configuration"));
    assert_eq!(a.agents.query, "sandbox_mode");

    t.write("project/.codex/config.toml", "sandbox_mode=[invalid");
    a.finish_codex(Ok(t.collect()));
    let incomplete = draw(&mut a, 80, 24);
    assert!(incomplete.contains("Provisional value: read-only"));
    assert!(incomplete.contains("INCOMPLETE configuration"));
    assert_eq!(a.agents.query, "sandbox_mode");
}

#[test]
fn system_user_profile_and_closest_trusted_project_precedence() {
    let t = Temp::new();
    t.write(
        "system/config.toml",
        "model='gpt-4'\nsandbox_mode='read-only'",
    );
    t.trust("trusted", "model='gpt-5'");
    t.write("home/.codex/review.config.toml", "model='gpt-6'");
    t.write("project/.codex/config.toml", "model='gpt-6.1'");
    t.write("project/nested/.codex/config.toml", "model='gpt-6.2'");
    let mut cx = t.context();
    cx.project = t.0.join("project/nested");
    cx.profile = Some("review".into());
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.trust, Trust::Trusted);
    assert_eq!(value(&c, "model"), "gpt-6.2");
    let s = c.settings.iter().find(|s| s.key == "model").unwrap();
    assert_eq!(s.definitions.len(), 5);
    assert!(
        c.setting_text(c.settings.iter().position(|s| s.key == "model").unwrap())
            .contains("Overridden")
    );
    assert_eq!(value(&c, "sandbox_mode"), "read-only");
}
#[test]
fn untrusted_project_is_visible_but_inactive() {
    let t = Temp::new();
    t.trust("untrusted", "model='gpt-5'");
    t.write("project/.codex/config.toml", "model='gpt-6'");
    let c = t.config();
    assert_eq!(value(&c, "model"), "gpt-5");
    assert_eq!(c.layers.last().unwrap().activity, Activity::Inactive);
    assert!(c.settings[0].conditional.is_none());
}
#[test]
fn unknown_trust_has_explicit_conditional_values() {
    let t = Temp::new();
    t.write("home/.codex/config.toml", "model='gpt-5'");
    t.write("project/.codex/config.toml", "model='gpt-6'");
    let c = t.config();
    assert_eq!(c.trust, Trust::Unknown);
    assert_eq!(value(&c, "model"), "gpt-5");
    assert_eq!(c.settings[0].conditional.as_ref().unwrap().0, "gpt-6");
}
#[test]
fn profile_is_explicit_and_missing_profile_is_reported() {
    let t = Temp::new();
    t.write("home/.codex/review.config.toml", "model='gpt-6'");
    assert!(!t.config().settings.iter().any(|s| s.key == "model"));
    let mut cx = t.context();
    cx.profile = Some("absent".into());
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    assert!(
        c.findings
            .iter()
            .any(|f| f.contains("profile file missing"))
    );
}
#[test]
fn legacy_profiles_and_forbidden_project_keys_are_not_applied() {
    let t = Temp::new();
    t.trust("trusted", "profile='old'\n[profiles.old]\nmodel='gpt-6'");
    t.write("project/.codex/config.toml","openai_base_url='https://private'\nnotify=['execute-me']\n[model_providers.private]\napi_key='SECRET-SENTINEL'");
    let c = t.config();
    assert!(!c.settings.iter().any(|s| s.key == "model"));
    let text = c.source_text(2);
    assert!(text.contains("ignored at project scope"));
    assert!(!text.contains("SECRET-SENTINEL"));
    assert!(!text.contains("https://private"));
    assert!(c.source_text(1).contains("legacy"));
}
#[test]
fn feature_fields_merge_independently_without_unknown_tables() {
    let t = Temp::new();
    t.write(
        "system/config.toml",
        "[features]\nshell_tool=true\nmulti_agent=false",
    );
    t.write(
        "home/.codex/config.toml",
        "[features]\nshell_tool=false\nunknown='PRIVATE'\n[mystery]\nvalue='PRIVATE'",
    );
    let c = t.config();
    assert_eq!(value(&c, "features.shell_tool"), "false");
    assert_eq!(value(&c, "features.multi_agent"), "false");
    assert!(!format!("{c:?}").contains("PRIVATE"));
}
#[test]
fn malformed_toml_errors_never_echo_values() {
    let t = Temp::new();
    t.write("home/.codex/config.toml", "secret='PRIVATE-MARKER\n");
    let c = t.config();
    assert_eq!(c.layers[1].status, "Invalid TOML");
    assert!(!format!("{c:?}").contains("PRIVATE-MARKER"));
    assert!(c.findings.iter().any(|f| f.contains("incomplete")));
}
#[test]
fn unsupported_values_are_redacted_without_false_fallback() {
    let t = Temp::new();
    t.write("system/config.toml", "sandbox_mode='read-only'");
    t.write(
        "home/.codex/config.toml",
        "sandbox_mode='PRIVATE-MARKER'\napproval_policy='untrusted'\nmodel='sk-fake-secret'",
    );
    let c = t.config();
    assert!(value(&c, "sandbox_mode").contains("uncertain"));
    assert!(!format!("{c:?}").contains("PRIVATE-MARKER"));
    assert!(!format!("{c:?}").contains("sk-fake"));
}
#[test]
fn only_named_configuration_files_are_read() {
    let t = Temp::new();
    t.write("home/.codex/auth.json", "PRIVATE-MARKER");
    t.write("home/.codex/sessions/session.json", "PRIVATE-MARKER");
    t.write("home/.codex/config.toml", "model='gpt-6'");
    let c = t.config();
    assert!(!format!("{c:?}").contains("PRIVATE-MARKER"));
    assert_eq!(c.layers.iter().filter(|l| l.present).count(), 1);
}
#[test]
fn nested_root_markers_and_empty_markers_are_respected() {
    let t = Temp::new();
    t.write("project/ROOT", "");
    t.write("project/nested/AGENTS.md", "Nested");
    t.write("home/.codex/config.toml", "project_root_markers=['ROOT']");
    let mut cx = t.context();
    cx.project = t.0.join("project/nested");
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.directories.len(), 2);
    t.write("home/.codex/config.toml", "project_root_markers=[]");
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.directories, vec![cx.project]);
}
#[test]
fn conflicting_trust_is_unknown() {
    let t = Temp::new();
    t.write("project/sub/file", "");
    t.trust("trusted", "");
    let p = t.0.join("home/.codex/config.toml");
    let mut text = fs::read_to_string(&p).unwrap();
    text += &format!(
        "[projects.\"{}\"]\ntrust_level='untrusted'",
        t.0.join("project/sub").display()
    );
    fs::write(p, text).unwrap();
    let mut cx = t.context();
    cx.project = t.0.join("project/sub");
    assert_eq!(
        config::collect(&cx, &AtomicBool::new(false)).unwrap().trust,
        Trust::Unknown
    );
}
#[test]
fn instruction_order_overrides_fallback_and_empty_files() {
    let t = Temp::new();
    t.write("home/.codex/AGENTS.override.md", "  \n");
    t.write("home/.codex/AGENTS.md", "Global");
    t.write(
        "home/.codex/config.toml",
        "project_doc_fallback_filenames=['TEAM.md']",
    );
    t.write("project/AGENTS.override.md", "Root override");
    t.write("project/AGENTS.md", "Shadowed");
    t.write("project/nested/TEAM.md", "Nested fallback");
    let mut cx = t.context();
    cx.project = t.0.join("project/nested");
    let i = codex::discover(&cx, &AtomicBool::new(false)).unwrap();
    let selected: Vec<_> = i
        .instructions
        .files
        .iter()
        .filter(|f| f.status == Status::Selected)
        .collect();
    assert_eq!(selected.len(), 3);
    assert_eq!(selected[0].preview, "Global");
    assert_eq!(selected[1].preview, "Root override");
    assert_eq!(selected[2].preview, "Nested fallback");
    assert!(
        i.instructions
            .files
            .iter()
            .any(|f| f.status == Status::Shadowed)
    );
    assert!(
        i.instructions
            .files
            .iter()
            .any(|f| f.status == Status::Empty)
    );
}
#[test]
fn instruction_limit_truncates_on_utf8_boundary_and_omits_later_files() {
    let t = Temp::new();
    t.write("home/.codex/config.toml", "project_doc_max_bytes=3");
    t.write("home/.codex/AGENTS.md", "ééé");
    t.write("project/AGENTS.md", "Root");
    let i = t.collect();
    assert!(i.instructions.included_bytes <= 3);
    assert_eq!(
        i.instructions
            .files
            .iter()
            .find(|f| f.status == Status::Truncated)
            .unwrap()
            .preview,
        "ééé"
    );
    assert!(
        i.instructions
            .files
            .iter()
            .any(|f| matches!(f.status, Status::Omitted | Status::Truncated))
    );
    assert!(i.instructions.total_bytes > 3);
}
#[test]
fn conditional_instruction_controls_have_separate_scenario() {
    let t = Temp::new();
    t.write("project/.codex/config.toml", "project_doc_max_bytes=2");
    t.write("project/AGENTS.md", "Root instructions");
    let i = t.collect();
    assert_eq!(i.instructions.limit, 32768);
    assert_eq!(i.conditional_instructions.unwrap().limit, 2);
}
#[test]
fn unsafe_fallback_filenames_do_not_read_credentials() {
    let t = Temp::new();
    t.write(
        "home/.codex/config.toml",
        "project_doc_fallback_filenames=['../auth.json']",
    );
    t.write("project/auth.json", "PRIVATE");
    let i = t.collect();
    assert!(i.instructions.uncertain);
    assert!(!format!("{:?}", i.instructions).contains("PRIVATE"));
}
#[test]
fn instruction_preview_is_sanitized_redacted_and_on_demand() {
    let t = Temp::new();
    t.write("project/AGENTS.md","Ordinary guidance\ninnocent = 'PRIVATE'\nAuthorization: PRIVATE\n-----BEGIN PRIVATE KEY-----\nPRIVATE\n-----END PRIVATE KEY-----\nHello\u{1b}[31m");
    let i = t.collect();
    let n = i
        .instructions
        .files
        .iter()
        .position(|f| f.status == Status::Selected)
        .unwrap();
    assert!(!i.instructions.text(n, false).contains("Ordinary guidance"));
    let text = i.instructions.text(n, true);
    assert!(text.contains("Ordinary guidance"));
    assert!(!text.contains("PRIVATE"));
    assert!(!text.contains('\u{1b}'));
}
#[test]
fn unknown_sensitive_tables_and_keys_are_withheld() {
    let t = Temp::new();
    t.write("home/.codex/config.toml","innocent='PRIVATE'\n[mcp_servers.PRIVATE]\nheaders={Authorization='PRIVATE'}\n[model_providers.PRIVATE]\nexperimental_bearer_token='PRIVATE'");
    let c = t.config();
    assert!(!format!("{c:?}").contains("PRIVATE"));
    assert!(c.source_text(1).contains("withheld"));
}
#[test]
fn oversized_and_invalid_utf8_configuration_are_unavailable() {
    let t = Temp::new();
    t.write(
        "home/.codex/config.toml",
        &"x".repeat(codex::FILE_LIMIT + 1),
    );
    assert_eq!(t.config().layers[1].status, "Unavailable");
    fs::write(t.0.join("home/.codex/config.toml"), [255, 254]).unwrap();
    assert_eq!(t.config().layers[1].status, "Unavailable");
}
#[cfg(unix)]
#[test]
fn config_instruction_symlinks_cannot_read_auth_or_external_files() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    t.write("home/.codex/auth.json", "PRIVATE");
    symlink("auth.json", t.0.join("home/.codex/config.toml")).unwrap();
    symlink(
        t.0.join("home/.codex/auth.json"),
        t.0.join("project/AGENTS.md"),
    )
    .unwrap();
    let i = t.collect();
    assert_eq!(i.configuration.layers[1].status, "Unavailable");
    assert!(
        i.instructions
            .files
            .iter()
            .any(|f| f.status == Status::Unavailable)
    );
    assert!(!format!("{:?}", i.instructions).contains("PRIVATE"));
}
#[test]
fn skill_enabled_disabled_and_discovered_states_remain_distinct() {
    let t = Temp::new();
    t.skill("home/.agents/skills/example");
    let path = t.0.join("home/.agents/skills/example");
    t.write(
        "home/.codex/config.toml",
        &format!(
            "[[skills.config]]\npath='{}'\nenabled=false",
            path.display()
        ),
    );
    let i = t.collect();
    assert_eq!(i.enablement[0].label, "Disabled in config");
    assert_eq!(i.enablement[0].source, Some(1));
    t.write(
        "home/.codex/config.toml",
        &format!(
            "[[skills.config]]\npath='{}'\nenabled=true",
            path.join("SKILL.md").display()
        ),
    );
    assert_eq!(t.collect().enablement[0].label, "Enabled in config");
    t.write("home/.codex/config.toml", "");
    assert_eq!(t.collect().enablement[0].label, "Eligible");
}
#[test]
fn unobserved_system_cache_activation_is_not_eligibility() {
    let t = Temp::new();
    t.skill("home/.codex/skills/.system/example");
    assert_eq!(t.collect().enablement[0].label, "Unknown · bundled copy");
}
#[test]
fn relative_project_skill_paths_use_containing_codex_directory() {
    let t = Temp::new();
    t.trust("trusted", "");
    t.skill("project/.agents/skills/example");
    t.write(
        "project/.codex/config.toml",
        "[[skills.config]]\npath='../.agents/skills/example'\nenabled=false",
    );
    assert_eq!(t.collect().enablement[0].label, "Disabled in config");
}
#[test]
fn unknown_relative_global_paths_are_not_guessed() {
    let t = Temp::new();
    t.skill("home/.agents/skills/example");
    t.write(
        "home/.codex/config.toml",
        "[[skills.config]]\npath='../.agents/skills/example'\nenabled=false",
    );
    let i = t.collect();
    assert_eq!(i.enablement[0].label, "Unknown · config unclear");
    assert_eq!(i.configuration.unresolved_skills(&i.skills), 1);
}
#[test]
fn unknown_trust_and_cross_layer_skill_arrays_do_not_infer_merge() {
    let t = Temp::new();
    t.skill("project/.agents/skills/example");
    t.write(
        "project/.codex/config.toml",
        "[[skills.config]]\npath='../.agents/skills/example'\nenabled=false",
    );
    assert_eq!(t.collect().enablement[0].label, "Unknown · config unclear");
    let path = t.0.join("project/.agents/skills/example");
    t.trust(
        "trusted",
        &format!("[[skills.config]]\npath='{}'\nenabled=true", path.display()),
    );
    assert_eq!(t.collect().enablement[0].label, "Unknown · config unclear");
}
#[test]
fn unmatched_skill_reference_is_reported_without_inventing_skill() {
    let t = Temp::new();
    t.write(
        "home/.codex/config.toml",
        "[[skills.config]]\npath='/nonexistent-synthetic-skill'\nenabled=false",
    );
    let i = t.collect();
    assert!(i.skills.skills.is_empty());
    assert_eq!(i.configuration.unresolved_skills(&i.skills), 1);
}
#[cfg(unix)]
#[test]
fn skill_symlink_correlation_uses_canonical_identity() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    t.skill("shared/example");
    fs::create_dir_all(t.0.join("home/.agents/skills")).unwrap();
    symlink(
        t.0.join("shared/example"),
        t.0.join("home/.agents/skills/example"),
    )
    .unwrap();
    t.write(
        "home/.codex/config.toml",
        &format!(
            "[[skills.config]]\npath='{}'\nenabled=false",
            t.0.join("shared/example").display()
        ),
    );
    assert_eq!(t.collect().enablement[0].label, "Disabled in config");
}
#[cfg(unix)]
#[test]
fn fixture_skill_symlinks_cannot_escape_fixture() {
    use std::os::unix::fs::symlink;
    let outside = Temp::new();
    outside.skill("private/example");
    let t = Temp::new();
    fs::create_dir_all(t.0.join("home/.agents/skills")).unwrap();
    symlink(
        outside.0.join("private/example"),
        t.0.join("home/.agents/skills/escape"),
    )
    .unwrap();
    let i = t.collect();
    assert!(i.skills.skills[0].instructions.is_empty());
    assert!(!i.skills.skills[0].accessible);
}
#[cfg(unix)]
#[test]
fn unreadable_files_are_not_empty_or_valid() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    t.write("home/.codex/config.toml", "model='gpt-6'");
    let p = t.0.join("home/.codex/config.toml");
    fs::set_permissions(&p, fs::Permissions::from_mode(0o0)).unwrap();
    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!(t.config().layers[1].status, "Unavailable");
    }
    fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn refresh_failure_retains_matching_configuration_and_skills() {
    let t = Temp::new();
    t.skill("home/.agents/skills/example");
    t.write("home/.codex/config.toml", "model='gpt-6'");
    let mut a = app(&t);
    let count = a.skills.inventory.skills.len();
    a.finish_codex(Err(std::io::Error::other("PRIVATE")));
    assert_eq!(a.skills.inventory.skills.len(), count);
    assert_eq!(
        value(&a.codex.inventory.as_ref().unwrap().configuration, "model"),
        "gpt-6"
    );
    let error = a.codex.error.clone().unwrap();
    assert!(error.contains("stale") && !error.contains("PRIVATE"));
    assert!(draw(&mut a, 120, 32).contains("stale"));
}
#[test]
fn codex_navigation_sources_back_and_snapshot_isolation() {
    let t = Temp::new();
    t.skill("home/.agents/skills/example");
    t.write(
        "home/.codex/config.toml",
        &format!(
            "model='gpt-6'\n[[skills.config]]\npath='{}'\nenabled=false",
            t.0.join("home/.agents/skills/example").display()
        ),
    );
    let mut a = app(&t);
    // e no longer jumps to sources.
    key(&mut a, KeyCode::Char('e'));
    assert_eq!(a.destination(), Destination::Skills);
    assert!(!a.codex.sources);
    key(&mut a, KeyCode::F(3)); // Instructions
    key(&mut a, KeyCode::F(3)); // Settings
    key(&mut a, KeyCode::Char('s'));
    key(&mut a, KeyCode::Enter); // Enter opens nothing.
    assert!(a.codex.sources && !a.inspecting());
    key(&mut a, KeyCode::F(3));
    assert_eq!(a.destination(), Destination::Mcp);
    let mut snapshot = App::new(
        inventory::Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot",
    );
    key(&mut snapshot, KeyCode::F(2));
    key(&mut snapshot, KeyCode::F(3));
    key(&mut snapshot, KeyCode::Char('r'));
    assert!(snapshot.codex.context.is_none());
    assert!(!snapshot.codex.requested);
    assert!(draw(&mut snapshot, 120, 32).contains("Codex not collected in this mode"));
}
#[test]
fn narrow_and_wide_rendering_never_displays_raw_toml_secrets() {
    let t = Temp::new();
    t.write(
        "home/.codex/config.toml",
        "model='gpt-6'\npassword='PRIVATE-SENTINEL'",
    );
    let mut a = app(&t);
    a.navigate(Destination::Sources);
    a.agents.selected[2] = 1;
    for (w, h) in [(120, 35), (40, 16), (20, 5)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| ui::render(f, &mut a)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(!text.contains("PRIVATE-SENTINEL"));
    }
}
#[test]
fn refresh_preserves_skill_selection_and_filters() {
    let t = Temp::new();
    t.skill("home/.agents/skills/example");
    let mut a = app(&t);
    search(&mut a, "example");
    let selected = a.agent_row().unwrap().key;
    a.finish_codex(Ok(t.collect()));
    assert_eq!(a.agents.query, "example");
    assert_eq!(a.agent_row().unwrap().key, selected);
}
#[test]
fn cancellation_stops_discovery() {
    let t = Temp::new();
    assert!(codex::discover(&t.context(), &AtomicBool::new(true)).is_err());
}
#[test]
fn selected_profile_rejects_path_traversal() {
    let t = Temp::new();
    assert!(Context::fixture(&t.0, Some("../auth".into())).is_err());
}

#[test]
#[ignore = "opt-in local configuration counts only; just live codex"]
fn live_codex_configuration() {
    let cx = Context::local(std::env::current_dir().unwrap(), None).unwrap();
    let i = codex::discover(&cx, &AtomicBool::new(false)).unwrap();
    println!(
        "Configuration layers present: {}; parsed: {}; unavailable: {}; invalid: {}; trust: {:?}",
        i.configuration.layers.iter().filter(|l| l.present).count(),
        i.configuration
            .layers
            .iter()
            .filter(|l| l.status == "Parsed")
            .count(),
        i.configuration
            .layers
            .iter()
            .filter(|l| l.status == "Unavailable")
            .count(),
        i.configuration
            .layers
            .iter()
            .filter(|l| l.status == "Invalid TOML")
            .count(),
        i.configuration.trust
    );
    for scope in [
        config::Scope::User,
        config::Scope::System,
        config::Scope::Profile,
        config::Scope::Project,
    ] {
        println!(
            "Configuration {scope:?}: {} present",
            i.configuration
                .layers
                .iter()
                .filter(|l| l.scope == scope && l.present)
                .count()
        );
    }
    println!(
        "Instructions discovered: {}; selected: {}; shadowed: {}; empty: {}; unavailable: {}; selected bytes: {}; included bytes: {}",
        i.instructions
            .files
            .iter()
            .filter(|f| !matches!(f.status, Status::Missing | Status::Unavailable))
            .count(),
        i.instructions.selected(),
        i.instructions
            .files
            .iter()
            .filter(|f| f.status == Status::Shadowed)
            .count(),
        i.instructions
            .files
            .iter()
            .filter(|f| f.status == Status::Empty)
            .count(),
        i.instructions
            .files
            .iter()
            .filter(|f| f.status == Status::Unavailable)
            .count(),
        i.instructions.total_bytes,
        i.instructions.included_bytes
    );
    for label in [
        "Enabled in config",
        "Disabled in config",
        "Eligible",
        "Unknown",
    ] {
        println!(
            "{label}: {}",
            i.enablement
                .iter()
                .filter(|e| e.label.starts_with(label))
                .count()
        );
    }
    println!(
        "Unresolved skill references: {}",
        i.configuration.unresolved_skills(&i.skills)
    );
    println!("{}", codex::LIMITATIONS);
    assert_eq!(i.skills.skills.len(), i.enablement.len());
}

#[test]
fn defaults_have_documented_provenance_and_view_searches_are_preserved() {
    let t = Temp::new();
    let c = t.config();
    assert_eq!(value(&c, "project_doc_max_bytes"), "32768");
    assert!(
        c.setting_text(
            c.settings
                .iter()
                .position(|s| s.key == "project_doc_max_bytes")
                .unwrap()
        )
        .contains("Documented default")
    );
    let mut a = app(&t);
    key(&mut a, KeyCode::F(3)); // Instructions
    a.agents.query = "max_bytes".into();
    key(&mut a, KeyCode::F(3)); // Settings: its own, empty search.
    assert!(a.agents.query.is_empty());
    for _ in 0..4 {
        key(&mut a, KeyCode::F(3)); // MCP, Hooks, Plugins, Skills
    }
    assert_eq!(a.destination(), Destination::Skills);
    key(&mut a, KeyCode::F(3));
    assert_eq!(a.agents.query, "max_bytes");
}

#[test]
fn instruction_accounting_includes_separators_and_keeps_shadowed_preview_separate() {
    let t = Temp::new();
    t.write("home/.codex/AGENTS.md", "abc");
    t.write("project/AGENTS.override.md", "def");
    t.write("project/AGENTS.md", "shadowed");
    let i = t.collect();
    assert_eq!(i.instructions.total_bytes, 8);
    assert_eq!(i.instructions.included_bytes, 8);
    assert_eq!(i.instructions.separator_bytes, 2);
    let shadowed = i
        .instructions
        .files
        .iter()
        .find(|f| f.status == Status::Shadowed)
        .unwrap();
    assert_eq!(shadowed.included, 0);
    assert_eq!(shadowed.preview, "shadowed");
}
#[test]
fn conflicting_rules_and_invalid_enabled_values_are_undetermined() {
    let t = Temp::new();
    t.skill("home/.agents/skills/example");
    let p = t.0.join("home/.agents/skills/example");
    t.write("home/.codex/config.toml",&format!("[[skills.config]]\npath='{}'\nenabled=true\n[[skills.config]]\npath='{}'\nenabled=false",p.display(),p.display()));
    assert_eq!(t.collect().enablement[0].label, "Unknown · config unclear");
    t.write(
        "home/.codex/config.toml",
        &format!("[[skills.config]]\npath='{}'\nenabled='false'", p.display()),
    );
    assert_eq!(t.collect().enablement[0].label, "Unknown · config unclear");
}
#[test]
fn parser_nesting_is_bounded_and_deceptive_model_values_are_withheld() {
    let t = Temp::new();
    t.write(
        "home/.codex/config.toml",
        &format!("unknown={}0{}", "[".repeat(1000), "]".repeat(1000)),
    );
    assert_eq!(t.config().layers[1].status, "Invalid TOML");
    t.write("home/.codex/config.toml", "model='gpt-private-secret'");
    assert!(!format!("{:?}", t.config()).contains("gpt-private-secret"));
}
#[test]
fn refreshing_configuration_keeps_selected_setting_by_identity() {
    let t = Temp::new();
    t.write("home/.codex/config.toml", "sandbox_mode='workspace-write'");
    let mut a = app(&t);
    a.navigate(Destination::Settings);
    a.agents_select("codex-setting:sandbox_mode");
    t.write(
        "home/.codex/config.toml",
        "model='gpt-6'\nsandbox_mode='workspace-write'",
    );
    a.finish_codex(Ok(t.collect()));
    assert_eq!(a.agent_row().unwrap().name, "sandbox_mode");
    assert!(rows(&a).contains(&"model".to_owned()));
}

#[test]
fn profile_trust_table_is_not_silently_merged_with_user_trust() {
    let t = Temp::new();
    t.trust("trusted", "");
    t.write("project/.codex/config.toml", "model='gpt-6'");
    t.write(
        "home/.codex/review.config.toml",
        &format!(
            "[projects.\"{}\"]\ntrust_level='untrusted'",
            t.0.join("project").display()
        ),
    );
    let mut cx = t.context();
    cx.profile = Some("review".into());
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.trust, Trust::Unknown);
    assert!(
        c.settings
            .iter()
            .find(|s| s.key == "model")
            .unwrap()
            .conditional
            .is_some()
    );
}
