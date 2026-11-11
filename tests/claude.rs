use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    agents::Section,
    app::App,
    claude::{self, Kind, Status},
    codex,
    inventory::Inventory,
    presentation::Destination,
    ui,
};
use ratatui::{Terminal, backend::TestBackend};
use std::{path::Path, sync::atomic::AtomicBool};

const FIXTURE: &str = "fixtures/claude";

fn collect() -> claude::Inventory {
    let context = claude::Context::fixture(Path::new(FIXTURE)).unwrap();
    claude::discover(&context, &AtomicBool::new(false)).unwrap()
}
/// A Claude fixture app with Codex collected too, as the terminal loop does.
fn app() -> App {
    let mut a = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "isolated Claude fixture",
    );
    let codex = codex::Context::fixture(Path::new(FIXTURE), None).unwrap();
    a.codex.configure(codex.clone());
    a.codex.finish(
        codex::discover(&codex, &AtomicBool::new(false)),
        &mut a.skills,
    );
    a.codex.requested = false;
    a.claude
        .configure(claude::Context::fixture(Path::new(FIXTURE)).unwrap());
    a.claude.finish(Ok(collect()));
    a.claude.requested = false;
    a.navigate(Destination::Skills);
    a
}
/// The rows the Agents area shows: name and its two cells.
fn rows(a: &App) -> Vec<(String, String, String)> {
    let sources = a.agent_sources();
    a.agents
        .rows(&sources)
        .into_iter()
        .map(|r| {
            let (x, y) = sources.cells(a.agents.section, &r);
            (r.name, x, y)
        })
        .collect()
}
fn names(a: &App) -> Vec<String> {
    rows(a).into_iter().map(|r| r.0).collect()
}
fn cells(a: &App, name: &str) -> (String, String) {
    let (_, x, y) = rows(a).into_iter().find(|r| r.0 == name).unwrap();
    (x, y)
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

#[test]
fn skills_come_from_claude_folders_and_symlinks_resolve() {
    let i = collect();
    let mut names: Vec<_> = i
        .skills
        .skills
        .iter()
        .map(|s| (s.name.as_str(), s.scope.label()))
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            ("claude-only", "User"),
            ("project-skill", "Project"),
            ("shared", "User")
        ]
    );
    let shared = i.skills.skills.iter().find(|s| s.name == "shared").unwrap();
    assert!(
        shared
            .resolved
            .as_ref()
            .unwrap()
            .ends_with("home/.agents/skills/shared")
    );
}

#[test]
fn instructions_follow_reading_order_and_links_are_not_read() {
    let i = collect();
    let files: Vec<_> = i
        .instructions
        .iter()
        .map(|f| {
            (
                f.path
                    .strip_prefix(Path::new(FIXTURE).canonicalize().unwrap())
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                f.kind,
            )
        })
        .collect();
    assert_eq!(
        files,
        [
            ("managed/CLAUDE.md".to_owned(), Kind::Managed),
            ("home/.claude/CLAUDE.md".to_owned(), Kind::User),
            ("project/CLAUDE.md".to_owned(), Kind::Project),
            ("project/.claude/CLAUDE.md".to_owned(), Kind::Project),
            ("project/CLAUDE.local.md".to_owned(), Kind::Local),
        ]
    );
    let user = &i.instructions[1];
    assert_eq!(user.imports, 2);
    assert!(!user.preview.contains("PRIVATE-CLAUDE-SENTINEL"));
    assert!(user.preview.contains("[redacted line]"));
    assert_eq!(i.instructions[3].status, Status::Empty);
    assert!(matches!(&i.instructions[4].status, Status::Link(Some(_))));
    assert!(i.instructions[4].preview.is_empty());
    // The fixture root's own CLAUDE.md candidates are checked and absent.
    assert_eq!(i.absent, 3);
}

#[test]
fn skills_say_which_tools_have_them_with_one_status_vocabulary() {
    let mut a = app();
    let screen = draw(&mut a, 160, 45);
    assert!(screen.contains("Agents  › Skills"), "{screen}");
    assert!(screen.contains("3 skills · 1 in both · 0 Codex only · 2 Claude only"));
    assert!(screen.contains("Tools") && screen.contains("Status"));
    // shared is ~/.agents/skills/shared for Codex and a symlink to it for Claude.
    assert_eq!(cells(&a, "shared"), ("Both".into(), "Ready".into()));
    assert_eq!(
        cells(&a, "claude-only"),
        ("Claude only".into(), "Ready".into())
    );
    // Without Codex results, nothing is claimed for Codex.
    a.skills.loaded = false;
    assert_eq!(cells(&a, "shared"), ("Claude only".into(), "Ready".into()));
}

#[test]
fn panel_shows_each_tool_and_scrolls_in_place() {
    let mut a = app();
    a.agents_select(
        &a.agent_sources()
            .rows(Section::Skills)
            .into_iter()
            .find(|r| r.name == "shared")
            .unwrap()
            .key,
    );
    let screen = draw(&mut a, 160, 45);
    assert!(screen.contains("Codex: Ready · Eligible"));
    assert!(screen.contains("Claude Code: Ready"));
    assert!(screen.contains("Links to:"));
    assert!(screen.contains("Same folder in both tools"));
    // No inspector: Enter does nothing and the panel scrolls in place.
    key(&mut a, KeyCode::Enter);
    assert!(!a.inspecting());
    key(&mut a, KeyCode::BackTab); // Instructions
    key(&mut a, KeyCode::Down); // User: AGENTS.md and CLAUDE.md
    let screen = draw(&mut a, 80, 24);
    assert!(screen.contains("PgDn for more"));
    assert!(!screen.contains("Enter"));
    key(&mut a, KeyCode::PageDown);
    assert!(a.agents.panel.offset > 0);
    key(&mut a, KeyCode::Down);
    assert_eq!(a.agents.panel.offset, 0); // A new selection starts at the top.
}

#[test]
fn mcp_servers_match_by_name_and_flag_different_servers() {
    let mut a = app();
    a.navigate(Destination::Mcp);
    assert_eq!(
        names(&a),
        [
            "codex-only",
            "docs",
            "local-only",
            "pending",
            "shared-name",
            "tools-mcp",
            "user-only"
        ]
    );
    assert_eq!(cells(&a, "docs"), ("Both".into(), "Ready".into()));
    assert_eq!(
        cells(&a, "codex-only"),
        ("Codex only".into(), "Ready".into())
    );
    assert_eq!(
        cells(&a, "pending"),
        ("Claude only".into(), "Needs approval".into())
    );
    assert_eq!(
        cells(&a, "local-only"),
        ("Claude only".into(), "Off".into())
    );
    // Codex points shared-name at a URL; Claude's winning entry runs a local program.
    assert_eq!(
        cells(&a, "shared-name"),
        (
            "Both, differ".into(),
            "Codex Ready · Claude Needs approval".into()
        )
    );
    assert!(draw(&mut a, 120, 32).contains("2 in both · 1 Codex only · 4 Claude only · 1 differ"));
    // f and i no longer filter.
    key(&mut a, KeyCode::Char('f'));
    key(&mut a, KeyCode::Char('i'));
    assert_eq!(names(&a).len(), 7);
    a.agents_select("mcp:shared-name");
    let screen = draw(&mut a, 160, 45);
    assert!(screen.contains("Codex: Ready · Locally eligible"));
    assert!(screen.contains("Claude Code: Needs approval · Pending approval"));
    assert!(screen.contains("different kind of server or program"));
}

#[test]
fn instructions_pair_agents_md_and_claude_md_by_folder() {
    let mut a = app();
    a.navigate(Destination::Instructions);
    assert_eq!(names(&a), ["Managed", "User", "./"]);
    assert_eq!(cells(&a, "Managed"), ("Claude only".into(), "Used".into()));
    assert_eq!(cells(&a, "User"), ("Both".into(), "Used".into()));
    assert_eq!(cells(&a, "./"), ("Both".into(), "Used".into()));
    // The panel names the files for each tool.
    a.agents_select("instructions:user");
    let screen = draw(&mut a, 160, 45);
    assert!(screen.contains("Codex: Used") && screen.contains("Claude Code: Used"));
    assert!(screen.contains("AGENTS.md") && screen.contains("CLAUDE.md"));
}

#[test]
fn settings_merge_by_name_and_hooks_plugins_use_the_same_words() {
    let mut a = app();
    a.navigate(Destination::Settings);
    let model: Vec<_> = rows(&a).into_iter().filter(|r| r.0 == "model").collect();
    assert_eq!(model.len(), 1);
    assert_eq!(model[0].1, "Both");
    assert!(model[0].2.starts_with("Codex ") && model[0].2.ends_with(" · Claude project-model"));
    assert_eq!(
        cells(&a, "permissions.defaultMode"),
        ("Claude only".into(), "acceptEdits".into())
    );
    a.navigate(Destination::Hooks);
    assert!(rows(&a).iter().all(|r| r.1 == "Claude only"));
    assert_eq!(cells(&a, "Stop · all").1, "Ready · prompt check");
    a.navigate(Destination::Plugins);
    assert!(cells(&a, "tools@market").1.starts_with("Ready · v1.0.0"));
    assert_eq!(cells(&a, "ghost@market").1, "Problem · Not installed");
}

#[test]
fn tab_switches_two_areas_and_shift_tab_cycles_six_sections() {
    let mut a = app();
    key(&mut a, KeyCode::BackTab);
    assert_eq!(a.destination(), Destination::Instructions);
    key(&mut a, KeyCode::Down);
    let selected = a.agent_row().unwrap().key;
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Aws);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Containers);
    key(&mut a, KeyCode::Tab);
    assert_eq!(a.destination(), Destination::Homebrew);
    key(&mut a, KeyCode::Tab);
    // The Agents area keeps its section and selection.
    assert_eq!(a.destination(), Destination::Instructions);
    assert_eq!(a.agent_row().unwrap().key, selected);
    for expected in [
        Destination::Settings,
        Destination::Mcp,
        Destination::Hooks,
        Destination::Plugins,
        Destination::Skills,
    ] {
        key(&mut a, KeyCode::BackTab);
        assert_eq!(a.destination(), expected);
        let section = expected.label().split(" / ").nth(1).unwrap();
        assert!(draw(&mut a, 80, 24).contains(&format!("Agents  › {section}")));
    }
}

#[test]
fn search_filters_and_keys_stay_text_while_searching() {
    let mut a = app();
    key(&mut a, KeyCode::Char('/'));
    for c in "projq".chars() {
        key(&mut a, KeyCode::Char(c));
    }
    assert!(!a.quit && names(&a).is_empty());
    key(&mut a, KeyCode::Backspace);
    key(&mut a, KeyCode::Enter);
    assert_eq!(names(&a), ["project-skill"]);
    assert!(draw(&mut a, 80, 24).contains("Search: proj"));
    key(&mut a, KeyCode::Char('q'));
    assert!(a.quit);
}

#[test]
fn renders_at_small_and_supported_sizes() {
    let mut a = app();
    for (w, h) in [(30, 10), (45, 20), (80, 24), (120, 32), (160, 45)] {
        for _ in 0..6 {
            draw(&mut a, w, h);
            key(&mut a, KeyCode::BackTab);
        }
    }
    let mut empty = App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "empty",
    );
    empty.navigate(Destination::Skills);
    let screen = draw(&mut empty, 80, 24);
    assert!(screen.contains("Claude Code not configured"));
    assert!(screen.contains("No skills found"));
}

fn config() -> claude::config::Config {
    collect().config
}
fn item<'a>(items: &'a [claude::config::Item], name: &str) -> &'a claude::config::Item {
    items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn settings_resolve_by_precedence_and_lists_combine() {
    let c = config();
    assert_eq!(c.files, "Files: User, Project, Managed");
    // The invalid local file is called out first and its value is not used.
    assert!(c.settings[0].warning && c.settings[0].summary.contains("invalid JSON"));
    let model = item(&c.settings, "model");
    assert_eq!(
        (model.source.as_str(), model.summary.as_str()),
        ("Project", "project-model")
    );
    assert!(model.detail.contains("Overridden: User: user-model"));
    assert_eq!(
        item(&c.settings, "permissions.defaultMode").summary,
        "acceptEdits"
    );
    assert_eq!(
        item(&c.settings, "permissions.disableBypassPermissionsMode").source,
        "Managed"
    );
    let allow = item(&c.settings, "permissions.allow");
    assert_eq!(
        (allow.source.as_str(), allow.summary.as_str()),
        ("Combined", "3 entries")
    );
    assert!(allow.detail.contains("Project · Bash(just check)"));
    assert_eq!(item(&c.settings, "permissions.deny").summary, "2 entries");
    assert_eq!(
        item(&c.settings, "apiKeyHelper").summary,
        "Runs a command · withheld"
    );
    assert_eq!(
        item(&c.settings, "env").summary,
        "2 variables · values withheld"
    );
    assert!(
        item(&c.settings, "Other settings")
            .detail
            .contains("syntheticUnknownSetting")
    );
}

#[test]
fn mcp_scopes_collisions_and_approvals() {
    let c = config();
    let status = |name: &str, source: &str| {
        let i = c
            .mcp
            .iter()
            .find(|i| i.name == name && i.source == source)
            .unwrap_or_else(|| panic!("{name} {source}"));
        i.summary.split(" · ").next().unwrap().to_owned()
    };
    assert_eq!(status("docs", "Project"), "Approved in Project settings");
    assert_eq!(status("pending", "Project"), "Pending approval");
    assert_eq!(status("shared-name", "Project"), "Pending approval");
    assert_eq!(status("shared-name", "User"), "Shadowed by Project");
    assert_eq!(status("local-only", "Local"), "Turned off for this project");
    assert_eq!(status("user-only", "User"), "Configured");
    assert_eq!(status("tools-mcp", "Plugin"), "Configured");
    let user = item(&c.mcp, "user-only");
    assert!(user.detail.contains("Program: user-server"));
    assert!(
        user.detail
            .contains("Environment variables: API_KEY (values withheld)")
    );
    assert!(item(&c.mcp, "pending").detail.contains("URL: withheld"));
    assert!(
        item(&c.mcp, "pending")
            .detail
            .contains("Headers: Authorization (values withheld)")
    );
}

#[test]
fn hooks_and_plugins_come_from_settings_and_enabled_plugins() {
    let c = config();
    let names: Vec<_> = c
        .hooks
        .iter()
        .map(|h| (h.name.as_str(), h.source.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("PreToolUse · Bash", "User"),
            ("PreToolUse · Bash", "User"),
            ("Stop · all", "User"),
            ("PostToolUse · Edit|Write", "Plugin"),
        ]
    );
    assert_eq!(c.hooks[1].summary, "[redacted line]");
    assert!(c.hooks[0].detail.contains("Timeout: 5s"));
    let tools = item(&c.plugins, "tools@market");
    assert_eq!(tools.source, "Enabled");
    assert_eq!(
        tools.summary,
        "v1.0.0 · 1 skill · 1 command · 1 agent · 1 hook · 1 MCP"
    );
    assert!(tools.detail.contains("GitHub example/market"));
    assert_eq!(item(&c.plugins, "ghost@market").summary, "Not installed");
}

#[test]
fn private_values_never_reach_the_screen() {
    let c = config();
    for i in c
        .settings
        .iter()
        .chain(&c.mcp)
        .chain(&c.hooks)
        .chain(&c.plugins)
    {
        let text = format!("{} {} {} {}", i.name, i.source, i.summary, i.detail);
        assert!(!text.contains("PRIVATE-"), "{text}");
    }
    let mut a = app();
    for _ in 0..6 {
        for _ in 0..names(&a).len() {
            assert!(!draw(&mut a, 120, 32).contains("PRIVATE-"));
            // Every page of the panel.
            loop {
                assert!(!draw(&mut a, 80, 24).contains("PRIVATE-"));
                if a.agents.panel.offset >= a.agents.panel.max {
                    break;
                }
                key(&mut a, KeyCode::PageDown);
            }
            key(&mut a, KeyCode::Down);
        }
        key(&mut a, KeyCode::BackTab);
    }
}

/// A temporary Claude fixture with the given settings files.
struct Temp(std::path::PathBuf);
impl Temp {
    fn new(name: &str, files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!("den-claude-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("project")).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        Self(root)
    }
    fn config(&self) -> claude::config::Config {
        let context = claude::Context::fixture(&self.0).unwrap();
        claude::discover(&context, &AtomicBool::new(false))
            .unwrap()
            .config
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn hook_switches_and_untrusted_project_approvals_are_explained() {
    let hooks = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#;
    let t = Temp::new(
        "off",
        &[
            ("home/.claude/settings.json", hooks),
            (
                "project/.claude/settings.json",
                r#"{"disableAllHooks":true}"#,
            ),
        ],
    );
    assert!(
        t.config().hooks[0]
            .summary
            .starts_with("Off (disableAllHooks)")
    );
    let t = Temp::new(
        "managed",
        &[
            ("home/.claude/settings.json", hooks),
            (
                "managed/managed-settings.json",
                r#"{"allowManagedHooksOnly":true}"#,
            ),
        ],
    );
    assert!(
        t.config().hooks[0]
            .summary
            .starts_with("Blocked (allowManagedHooksOnly)")
    );
    // Without a recorded trust decision, project-settings approval is conditional.
    let t = Temp::new(
        "trust",
        &[
            (
                "project/.mcp.json",
                r#"{"mcpServers":{"docs":{"command":"docs"}}}"#,
            ),
            (
                "project/.claude/settings.json",
                r#"{"enableAllProjectMcpServers":true}"#,
            ),
        ],
    );
    assert!(
        t.config().mcp[0]
            .summary
            .starts_with("Approved by project settings, if the folder is trusted")
    );
    let t = Temp::new(
        "reject",
        &[
            (
                "project/.mcp.json",
                r#"{"mcpServers":{"docs":{"command":"docs"}}}"#,
            ),
            (
                "project/.claude/settings.json",
                r#"{"enableAllProjectMcpServers":true}"#,
            ),
            (
                "home/.claude/settings.json",
                r#"{"disabledMcpjsonServers":["docs"]}"#,
            ),
        ],
    );
    assert!(t.config().mcp[0].summary.starts_with("Rejected"));
    let t = Temp::new(
        "managedmcp",
        &[
            (
                "project/.mcp.json",
                r#"{"mcpServers":{"docs":{"command":"docs"}}}"#,
            ),
            (
                "managed/managed-mcp.json",
                r#"{"mcpServers":{"corp":{"command":"corp"}}}"#,
            ),
        ],
    );
    let c = t.config();
    assert!(
        item(&c.mcp, "docs")
            .summary
            .starts_with("Blocked: managed-mcp.json")
    );
    assert!(item(&c.mcp, "corp").summary.starts_with("Configured"));
}

#[test]
fn f_i_and_o_are_not_agents_keys() {
    let mut a = app();
    for c in ['f', 'i', 'o'] {
        key(&mut a, KeyCode::Char(c));
        assert_eq!(names(&a).len(), 3);
    }
    let footer = draw(&mut a, 80, 24).lines().last().unwrap().to_owned();
    assert!(!footer.contains("f/i") && !footer.contains("o:files"));
}
