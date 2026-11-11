// Integration tests: unwrap and expect report failures here.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    app::{App, Module},
    codex::{
        self, Context,
        config::{self, Activity, Scope},
        mcp::{self, Auth, EndpointKind, Severity, Status, Transport},
    },
    inventory,
    presentation::Destination,
    ui,
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
            "den-mcp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(p.join("project/.git")).unwrap();
        Self(fs::canonicalize(p).unwrap())
    }
    fn write(&self, path: &str, text: &str) {
        let p = self.0.join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn user(&self, text: &str) {
        self.write("home/.codex/config.toml", text);
    }
    fn context(&self) -> Context {
        Context::fixture(&self.0, None).unwrap()
    }
    fn config(&self) -> config::Configuration {
        config::collect(&self.context(), &AtomicBool::new(false)).unwrap()
    }
    fn trust(&self, trust: &str, extra: &str) {
        self.user(&format!(
            "{extra}\n[projects.\"{}\"]\ntrust_level='{trust}'",
            self.0.join("project").display()
        ));
    }
    fn app(&self) -> App {
        let mut a = App::new(
            inventory::Inventory {
                schema_version: 1,
                resources: vec![],
            },
            "MCP test",
        );
        a.module = Module::Agents;
        a.codex.configure(self.context());
        a.codex.finish(
            codex::discover(&self.context(), &AtomicBool::new(false)),
            &mut a.skills,
        );
        a.codex.requested = false;
        a.navigate(Destination::Mcp);
        a
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn key(a: &mut App, key: KeyCode) {
    a.handle_key(KeyEvent::new(key, KeyModifiers::NONE));
}
fn dump(c: &config::Configuration) -> String {
    let mut s = format!("{c:?}");
    for server in &c.mcp.servers {
        for section in 0..6 {
            s.push_str(&server.text(section, &c.layers));
        }
    }
    for n in 0..c.layers.len() {
        s.push_str(&c.source_text(n));
    }
    s
}
const STDIO: &str = "[mcp_servers.docs]\ncommand='npx'\nargs=['-y','@example/server']";

#[test]
fn user_stdio_and_http_are_distinct_and_never_claim_runtime() {
    let t = Temp::new();
    t.user(&format!(
        "{STDIO}\n[mcp_servers.web]\nurl='https://example.com/private?secret=hidden'"
    ));
    let c = t.config();
    assert_eq!(c.mcp.servers.len(), 2);
    assert_eq!(c.mcp.servers[0].transport(), Transport::Stdio);
    assert_eq!(c.mcp.servers[1].transport(), Transport::Http);
    assert!(c.mcp.servers.iter().all(|s| s.status == Status::Eligible));
    assert!(dump(&c).contains("not checked"));
    assert!(!dump(&c).contains("@example/server"));
    assert!(!dump(&c).contains("secret=hidden"));
}
#[test]
fn trusted_project_definition_is_eligible_and_untrusted_is_inactive() {
    let t = Temp::new();
    t.write("project/.codex/config.toml", STDIO);
    t.trust("trusted", "");
    assert_eq!(t.config().mcp.servers[0].status, Status::Eligible);
    t.trust("untrusted", "");
    let c = t.config();
    assert_eq!(c.mcp.servers[0].status, Status::Configured);
    assert_eq!(c.mcp.servers[0].definitions[0].activity, Activity::Inactive);
}
#[test]
fn unknown_project_trust_is_conditional() {
    let t = Temp::new();
    t.write("project/.codex/config.toml", STDIO);
    let c = t.config();
    assert_eq!(c.mcp.servers[0].status, Status::Conditional);
    assert!(c.mcp.servers[0].selected.is_none());
}
#[test]
fn selected_profile_is_discovered_but_no_profile_is_assumed() {
    let t = Temp::new();
    t.write("home/.codex/review.config.toml", STDIO);
    assert!(t.config().mcp.servers.is_empty());
    let mut cx = t.context();
    cx.profile = Some("review".into());
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    assert_eq!(c.mcp.servers[0].definitions[0].scope, Scope::Profile);
}
#[test]
fn system_configuration_and_disabled_server_are_retained() {
    let t = Temp::new();
    t.write(
        "system/config.toml",
        &format!("{STDIO}\nenabled=false\nrequired=true"),
    );
    let c = t.config();
    let s = &c.mcp.servers[0];
    assert_eq!(s.status, Status::Disabled);
    assert_eq!(s.definitions[0].scope, Scope::System);
    assert_eq!(s.definitions[0].required, Some(true));
}
#[test]
fn cross_layer_mcp_tables_are_not_merged_by_guessing() {
    let t = Temp::new();
    t.trust("trusted", STDIO);
    t.write(
        "project/.codex/config.toml",
        "[mcp_servers.docs]\nenabled=false\nargs=['SECRET-ARG']",
    );
    let c = t.config();
    let s = &c.mcp.servers[0];
    assert_eq!(s.definitions.len(), 2);
    assert_eq!(s.status, Status::Conditional);
    assert!(s.selected.is_none());
    assert!(!s.definitions[1].invalid());
    assert!(!dump(&c).contains("SECRET-ARG"));
}
#[test]
fn inactive_override_does_not_hide_active_definition() {
    let t = Temp::new();
    t.trust("untrusted", STDIO);
    t.write(
        "project/.codex/config.toml",
        "[mcp_servers.docs]\nenabled=false",
    );
    let c = t.config();
    assert_eq!(c.mcp.servers[0].status, Status::Eligible);
    assert_eq!(c.mcp.servers[0].selected, Some(0));
}
#[test]
fn stable_identity_distinguishes_plugins_and_standalone_servers() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\n[plugins.\"sample@source\".mcp_servers.docs]\nenabled=true\nenabled_tools=['search']"));
    let c = t.config();
    assert_eq!(c.mcp.servers.len(), 2);
    assert_ne!(c.mcp.servers[0].id, c.mcp.servers[1].id);
    assert_eq!(c.mcp.servers[1].status, Status::PluginUnresolved);
    assert_eq!(c.mcp.servers[1].transport(), Transport::Unknown);
    assert_eq!(c.mcp.counts()["Unresolved plugin references"], 1);
}
#[test]
fn plugin_overrides_never_invent_a_transport_or_launch_definition() {
    let t = Temp::new();
    t.user("[plugins.\"sample@source\".mcp_servers.docs]\nenabled=false\ncommand='/private/DO-NOT-INSPECT'\nurl='https://PRIVATE'\ndefault_tools_approval_mode='prompt'\n[plugins.\"sample@source\".mcp_servers.docs.tools.search]\napproval_mode='approve'");
    let c = t.config();
    let s = &c.mcp.servers[0];
    assert_eq!(s.status, Status::PluginUnresolved);
    assert_eq!(s.definitions[0].enabled, Some(false));
    assert!(!dump(&c).contains("DO-NOT-INSPECT"));
    assert!(!dump(&c).contains("https://PRIVATE"));
    assert!(s.definitions[0].endpoint.is_none());
}
#[test]
fn missing_transport_and_conflicting_transports_are_errors() {
    for text in [
        "[mcp_servers.x]\nenabled=true",
        "[mcp_servers.x]\ncommand='npx'\nurl='https://example.com'",
        "[mcp_servers.x]\nurl=''",
        "[mcp_servers.x]\ncommand=12",
    ] {
        let t = Temp::new();
        t.user(text);
        let c = t.config();
        assert_eq!(c.mcp.servers[0].status, Status::Invalid);
    }
}
#[test]
fn package_manager_launchers_are_not_reported_missing_binaries() {
    for launcher in ["npx", "uvx", "bunx"] {
        let t = Temp::new();
        t.user(&format!(
            "[mcp_servers.x]\ncommand='{launcher}'\nargs=['not-a-file']"
        ));
        let c = t.config();
        let d = &c.mcp.servers[0].definitions[0];
        assert!(!d.invalid());
        assert!(d.launcher.contains(launcher));
        assert!(d.launcher.contains("unresolved"));
    }
}
#[test]
fn absolute_launcher_and_working_directory_observations_are_bounded() {
    let t = Temp::new();
    t.user(&format!(
        "[mcp_servers.x]\ncommand='{}'\ncwd='{}'",
        t.0.join("missing-launcher").display(),
        t.0.join("missing-dir").display()
    ));
    let c = t.config();
    let d = &c.mcp.servers[0].definitions[0];
    assert!(d.invalid());
    assert!(
        d.findings
            .iter()
            .filter(|f| f.message.contains("missing"))
            .count()
            >= 2
    );
}
#[test]
fn command_arguments_and_helpers_are_never_executed() {
    let t = Temp::new();
    t.write("launcher", "#!/bin/sh\ntouch SHOULD-NOT-EXIST\n");
    t.user(&format!("[mcp_servers.x]\ncommand='{}'\nargs=['$(touch SHOULD-NOT-EXIST)']\n[mcp_servers.web]\nurl='https://example.com'\nhttp_headers_helper='touch SHOULD-NOT-EXIST'",t.0.join("launcher").display()));
    let c = t.config();
    assert_eq!(c.mcp.servers.len(), 2);
    assert!(!t.0.join("SHOULD-NOT-EXIST").exists());
    assert!(!dump(&c).contains("touch SHOULD-NOT-EXIST"));
}
#[test]
fn remote_stdio_does_not_stat_remote_launcher_or_directory() {
    let t = Temp::new();
    t.user("[mcp_servers.remote]\ncommand='/remote/nonexistent-launcher'\ncwd='/remote/directory'\nexperimental_environment='remote'\nenv_vars=['LOCAL_TOKEN',{name='REMOTE_TOKEN',source='remote'}]");
    let c = t.config();
    let d = &c.mcp.servers[0].definitions[0];
    assert!(!d.invalid());
    assert_eq!(d.forwarded.len(), 2);
    assert_eq!(d.forwarded[1].source, "remote");
    assert_eq!(c.mcp.servers[0].status, Status::Eligible);
}
#[test]
fn invalid_remote_context_and_forwarding_are_diagnosed() {
    for text in [
        "command='npx'\nenv_vars=[{name='REMOTE_TOKEN',source='remote'}]",
        "url='https://example.com'\nexperimental_environment='remote'",
        "command='npx'\nexperimental_environment='private-environment'",
        "command='npx'\nenv_vars=[{name='A',source='elsewhere'}]",
    ] {
        let t = Temp::new();
        t.user(&format!("[mcp_servers.x]\n{text}"));
        assert_eq!(t.config().mcp.servers[0].status, Status::Invalid);
    }
}
#[test]
fn inline_environment_values_are_never_retained_or_rendered() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\ncommand='npx'\nenv={INNOCENT='MCP-SECRET-CANARY',PASSWORD='MCP-SECRET-CANARY'}\nenv_vars=['PUBLIC_VARIABLE']");
    let c = t.config();
    let d = &c.mcp.servers[0].definitions[0];
    assert_eq!(d.inline_env, Some(2));
    assert!(d.findings.iter().any(|f| f.severity == Severity::Review));
    assert!(dump(&c).contains("PUBLIC_VARIABLE"));
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[test]
fn endpoint_classification_uses_literals_without_dns() {
    for (url, kind) in [
        ("http://127.0.0.1/mcp", EndpointKind::Loopback),
        ("http://[::1]/mcp", EndpointKind::Loopback),
        ("http://localhost/mcp", EndpointKind::Loopback),
        ("https://10.0.0.1/mcp", EndpointKind::PrivateIp),
        ("https://[fd00::1]/mcp", EndpointKind::PrivateIp),
        ("http://[::ffff:127.0.0.1]/mcp", EndpointKind::Loopback),
        ("https://8.8.8.8/mcp", EndpointKind::OtherIp),
        ("https://private.internal/mcp", EndpointKind::RemoteHostname),
    ] {
        let t = Temp::new();
        t.user(&format!("[mcp_servers.x]\nurl='{url}'"));
        assert_eq!(
            t.config().mcp.servers[0].definitions[0]
                .endpoint
                .as_ref()
                .unwrap()
                .kind,
            kind
        );
    }
}
#[test]
fn invalid_endpoints_are_errors_without_echoing_input() {
    for url in [
        "not a URL",
        "https://",
        "file:///private/SECRET",
        "https://example.com:99999",
        "https://example.com\\SECRET",
        "https://[invalid]/mcp",
    ] {
        let t = Temp::new();
        t.user(&format!("[mcp_servers.x]\nurl='{url}'"));
        let c = t.config();
        assert_eq!(c.mcp.servers[0].status, Status::Invalid);
        assert!(!dump(&c).contains(url));
    }
}
#[test]
fn url_userinfo_path_query_fragment_and_host_are_withheld() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='https://MCP-SECRET-CANARY:password@MCP-SECRET-CANARY.example/MCP-SECRET-CANARY?key=MCP-SECRET-CANARY#MCP-SECRET-CANARY'");
    let c = t.config();
    assert!(c.mcp.servers[0].definitions[0].endpoint.is_some());
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[test]
fn plaintext_non_loopback_is_review_not_maliciousness() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='http://10.0.0.1/mcp'");
    let c = t.config();
    assert_eq!(c.mcp.servers[0].status, Status::Eligible);
    assert!(
        c.mcp.servers[0].definitions[0]
            .findings
            .iter()
            .any(|f| f.severity == Severity::Review && f.message.contains("plaintext"))
    );
}
#[test]
fn auth_mechanisms_can_coexist_without_credential_resolution() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='https://example.com'\nauth='chatgpt'\nbearer_token_env_var='BEARER_VARIABLE'\nhttp_headers={authorization='MCP-SECRET-CANARY'}\nenv_http_headers={Authorization='HEADER_VARIABLE'}\nhttp_headers_helper='MCP-SECRET-CANARY'\nscopes=['MCP-SECRET-CANARY']\n[mcp_servers.x.oauth]\nclient_id='MCP-SECRET-CANARY'\nclient_secret='MCP-SECRET-CANARY'\ncallback_url='http://localhost/MCP-SECRET-CANARY'\ncallback_port=7777");
    let c = t.config();
    let d = &c.mcp.servers[0].definitions[0];
    for auth in [
        Auth::ChatGpt,
        Auth::OAuth,
        Auth::BearerEnvironment,
        Auth::StaticAuthorization,
        Auth::EnvironmentHeaders,
        Auth::HeaderHelper,
    ] {
        assert!(d.auth.contains(&auth));
    }
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
    assert!(dump(&c).contains("BEARER_VARIABLE"));
    assert!(dump(&c).contains("precedence"));
}
#[test]
fn no_explicit_auth_does_not_mean_unauthenticated_runtime() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='https://example.com'");
    let c = t.config();
    assert_eq!(
        c.mcp.servers[0].definitions[0].auth,
        vec![Auth::NoneIdentified]
    );
    assert!(dump(&c).contains("stored OAuth if available"));
}
#[test]
fn oauth_client_secrets_require_nonempty_client_id() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='https://example.com'\n[mcp_servers.x.oauth]\nclient_secret='MCP-SECRET-CANARY'");
    let c = t.config();
    assert_eq!(c.mcp.servers[0].status, Status::Invalid);
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[test]
fn header_helpers_require_local_http() {
    for config in [
        "command='npx'",
        "url='https://example.com'\nexperimental_environment='remote'",
    ] {
        let t = Temp::new();
        t.user(&format!(
            "[mcp_servers.x]\n{config}\nhttp_headers_helper='SECRET-HELPER'"
        ));
        let c = t.config();
        assert!(c.mcp.servers[0].definitions[0].invalid());
        assert!(!dump(&c).contains("SECRET-HELPER"));
    }
}
#[test]
fn allowlist_then_denylist_remain_configured_restrictions() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\nenabled_tools=['read','write']\ndisabled_tools=['write','unadvertised']\ndefault_tools_approval_mode='prompt'\n[mcp_servers.docs.tools.read]\napproval_mode='approve'\noutput_token_limit=500"));
    let c = t.config();
    let tools = &c.mcp.servers[0].definitions[0].tools;
    assert_eq!(tools.allow.as_ref().unwrap(), &["read", "write"]);
    assert_eq!(tools.deny.as_ref().unwrap(), &["write", "unadvertised"]);
    assert_eq!(tools.approval, Some(mcp::Approval::Prompt));
    assert_eq!(tools.overrides[0].output_token_limit, Some(500));
    let text = c.mcp.servers[0].text(3, &c.layers);
    assert!(text.find("Allowlist first").unwrap() < text.find("Denylist second").unwrap());
    assert!(text.contains("configured restrictions only"));
}
#[test]
fn every_documented_approval_mode_is_preserved() {
    for mode in ["auto", "prompt", "writes", "approve"] {
        let t = Temp::new();
        t.user(&format!("{STDIO}\ndefault_tools_approval_mode='{mode}'"));
        let c = t.config();
        assert_eq!(
            c.mcp.servers[0].definitions[0]
                .tools
                .approval
                .unwrap()
                .label(),
            mode
        );
    }
}
#[test]
fn unknown_policy_and_nonpositive_tool_limit_are_errors() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\ndefault_tools_approval_mode='SECRET-POLICY'\n[mcp_servers.docs.tools.read]\noutput_token_limit=0"));
    let c = t.config();
    assert!(c.mcp.servers[0].definitions[0].invalid());
    assert!(!dump(&c).contains("SECRET-POLICY"));
}
#[test]
fn explicit_empty_allowlist_is_distinguished_from_absence() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\nenabled_tools=[]"));
    let c = t.config();
    assert_eq!(c.mcp.servers[0].definitions[0].tools.allow, Some(vec![]));
    assert!(dump(&c).contains("explicitly empty"));
}
#[test]
fn timeout_aliases_and_required_setting_are_retained() {
    let t = Temp::new();
    t.user(&format!(
        "{STDIO}\nstartup_timeout_ms=2500\ntool_timeout_sec=1.5\nrequired=true"
    ));
    let c = t.config();
    let d = &c.mcp.servers[0].definitions[0];
    assert_eq!(d.startup_seconds, Some(2.5));
    assert_eq!(d.tool_seconds, Some(1.5));
    assert_eq!(d.required, Some(true));
    t.user(&format!(
        "{STDIO}\nstartup_timeout_ms=2500\nstartup_timeout_sec=3"
    ));
    assert_eq!(t.config().mcp.servers[0].status, Status::Conditional);
}
#[test]
fn unsupported_values_and_unknown_fields_do_not_leak() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\ncommand='npx'\nenabled='MCP-SECRET-CANARY'\nhttp_headers='MCP-SECRET-CANARY'\nunknown={key='MCP-SECRET-CANARY'}\nargs=[12,'MCP-SECRET-CANARY']");
    let c = t.config();
    assert!(c.mcp.servers[0].definitions[0].invalid());
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[test]
fn malformed_toml_is_incomplete_not_a_valid_empty_inventory() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='MCP-SECRET-CANARY\n");
    let c = t.config();
    assert!(c.mcp.servers.is_empty());
    assert!(!c.mcp.findings.is_empty());
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[test]
fn invalid_utf8_and_oversized_config_remain_bounded() {
    let t = Temp::new();
    t.user(&"x".repeat(codex::FILE_LIMIT + 1));
    assert!(!t.config().mcp.findings.is_empty());
    fs::write(t.0.join("home/.codex/config.toml"), [255]).unwrap();
    assert!(!t.config().mcp.findings.is_empty());
}
#[test]
fn oversized_mcp_collections_report_partial_coverage() {
    let t = Temp::new();
    let text = (0..mcp::MAX_DEFINITIONS + 1)
        .map(|n| format!("[mcp_servers.s{n}]\ncommand='npx'\n"))
        .collect::<String>();
    t.user(&text);
    let c = t.config();
    assert_eq!(c.mcp.servers.len(), mcp::MAX_DEFINITIONS);
    assert!(c.mcp.partial);
    assert!(
        c.mcp
            .servers
            .iter()
            .all(|s| s.status == Status::Conditional)
    );
}
#[test]
fn oversized_policy_arrays_fail_without_retaining_values() {
    let t = Temp::new();
    t.user(&format!(
        "{STDIO}\nenabled_tools=[{}]",
        vec!["'MCP-SECRET-CANARY'"; mcp::MAX_ITEMS + 1].join(",")
    ));
    let c = t.config();
    assert!(c.mcp.servers[0].definitions[0].invalid());
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[cfg(unix)]
#[test]
fn unsafe_config_symlink_is_not_read() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    t.write("home/.codex/auth.json", "MCP-SECRET-CANARY");
    symlink("auth.json", t.0.join("home/.codex/config.toml")).unwrap();
    let c = t.config();
    assert!(c.mcp.servers.is_empty());
    assert!(!c.mcp.findings.is_empty());
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
}
#[cfg(unix)]
#[test]
fn launcher_symlink_cycles_do_not_block_or_expose_targets() {
    use std::os::unix::fs::symlink;
    let t = Temp::new();
    symlink("cycle", t.0.join("cycle")).unwrap();
    t.user(&format!(
        "[mcp_servers.x]\ncommand='{}'",
        t.0.join("cycle").display()
    ));
    let c = t.config();
    assert!(
        c.mcp.servers[0].definitions[0]
            .findings
            .iter()
            .any(|f| f.message.contains("symlink"))
    );
}
#[cfg(unix)]
#[test]
fn inaccessible_working_directory_is_reported_without_reading_contents() {
    use std::os::unix::fs::PermissionsExt;
    let t = Temp::new();
    t.write("locked/private", "MCP-SECRET-CANARY");
    fs::set_permissions(t.0.join("locked"), fs::Permissions::from_mode(0o0)).unwrap();
    t.user(&format!("{STDIO}\ncwd='{}'", t.0.join("locked").display()));
    let c = t.config();
    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } != 0 {
        assert!(c.mcp.servers[0].definitions[0].invalid());
    }
    assert!(!dump(&c).contains("MCP-SECRET-CANARY"));
    fs::set_permissions(t.0.join("locked"), fs::Permissions::from_mode(0o700)).unwrap();
}
#[test]
fn hostile_terminal_names_are_withheld() {
    let t = Temp::new();
    t.user("[mcp_servers.\"bad\\u001b[31m\"]\ncommand='npx'\nenabled_tools=['bad control']");
    let c = t.config();
    assert!(!dump(&c).contains('\u{1b}'));
    assert_eq!(c.mcp.servers[0].name, "[withheld name]");
}
/// Names of the MCP rows the Agents area shows.
fn rows(a: &App) -> Vec<String> {
    let sources = a.agent_sources();
    a.agents
        .rows(&sources)
        .into_iter()
        .map(|r| r.name)
        .collect()
}
#[test]
fn search_and_tools_column_for_codex_only_servers() {
    let t = Temp::new();
    t.trust("trusted", STDIO);
    t.write(
        "project/.codex/config.toml",
        "[mcp_servers.web]\nurl='https://example.com'\nenabled=false",
    );
    let mut a = t.app();
    assert_eq!(rows(&a).len(), 2);
    a.agents.query = "web".into();
    assert_eq!(rows(&a), ["web"]);
    // Claude Code is not configured here, so every server is Codex only; the disabled
    // project server is Off.
    let sources = a.agent_sources();
    let row = a.agents.rows(&sources).remove(0);
    assert_eq!(
        sources.cells(den::agents::Section::Mcp, &row),
        ("Codex only".into(), "Off".into())
    );
    a.agents.query.clear();
    assert_eq!(rows(&a).len(), 2);
}
#[test]
fn refresh_preserves_identity_and_failure_keeps_mcp_snapshot() {
    let t = Temp::new();
    t.user("[mcp_servers.z]\ncommand='npx'");
    let mut a = t.app();
    assert_eq!(a.agent_row().unwrap().name, "z");
    t.user("[mcp_servers.a]\ncommand='npx'\n[mcp_servers.z]\ncommand='npx'");
    a.finish_codex(codex::discover(&t.context(), &AtomicBool::new(false)));
    // A new server sorts first; the selection stays on z.
    assert_eq!(rows(&a), ["a", "z"]);
    assert_eq!(a.agent_row().unwrap().name, "z");
    a.finish_codex(Err(std::io::Error::other("MCP-SECRET-CANARY")));
    let error = a.codex.error.clone().unwrap();
    assert!(error.contains("stale") && !error.contains("MCP-SECRET-CANARY"));
    assert_eq!(
        a.codex
            .inventory
            .as_ref()
            .unwrap()
            .configuration
            .mcp
            .servers
            .len(),
        2
    );
}
#[test]
fn snapshot_mcp_is_unavailable_and_never_requests_live_collection() {
    let mut a = App::new(
        inventory::Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "snapshot",
    );
    a.navigate(Destination::Skills);
    for _ in 0..3 {
        key(&mut a, KeyCode::F(3));
    }
    assert_eq!(a.destination(), Destination::Mcp);
    key(&mut a, KeyCode::Char('r'));
    assert!(!a.codex.requested);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 30)).unwrap();
    terminal.draw(|f| ui::render(f, &mut a)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(text.contains("Codex not collected in this mode"));
}
#[test]
fn rendering_every_panel_page_size_and_empty_inventory_never_leaks_secrets() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='https://user:MCP-SECRET-CANARY@host/MCP-SECRET-CANARY'\nhttp_headers={Authorization='MCP-SECRET-CANARY'}\nhttp_headers_helper='MCP-SECRET-CANARY'");
    let mut a = t.app();
    for (w, h) in [(120, 35), (40, 16), (20, 5)] {
        a.agents.panel.offset = 0;
        loop {
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
            assert!(!text.contains("MCP-SECRET-CANARY"));
            if a.agents.panel.offset >= a.agents.panel.max {
                break;
            }
            a.agents.panel.by(1);
        }
    }
    a.agents.query = "no-match".into();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| ui::render(f, &mut a)).unwrap();
    assert!(rows(&a).is_empty());
}
#[test]
fn source_inspection_uses_existing_safe_summary() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\nenv={{INNOCENT='MCP-SECRET-CANARY'}}"));
    let c = t.config();
    let text = c.source_text(1);
    assert!(text.contains("MCP: docs"));
    assert!(!text.contains("MCP-SECRET-CANARY"));
}
#[test]
fn cancelling_existing_collector_also_cancels_mcp() {
    let t = Temp::new();
    t.user(STDIO);
    assert!(config::collect(&t.context(), &AtomicBool::new(true)).is_err());
}

#[test]
#[ignore = "opt-in aggregate local MCP configuration check; no MCP execution or networking"]
fn live_mcp_configuration() {
    let cx = Context::local(std::env::current_dir().unwrap(), None).unwrap();
    let c = config::collect(&cx, &AtomicBool::new(false)).unwrap();
    for (label, count) in c.mcp.counts() {
        println!("{label}: {count}");
    }
    println!("Partial coverage: {}", c.mcp.partial);
    println!("{}", mcp::LIMITATIONS);
    assert_eq!(
        c.mcp.definition_count(),
        c.mcp
            .servers
            .iter()
            .map(|s| s.definitions.len())
            .sum::<usize>()
    );
}

#[test]
fn invalid_values_and_timeout_conflicts_never_display_defaults() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\nenabled='bad'\nrequired='bad'\nstartup_timeout_sec=2\nstartup_timeout_ms=30\ntool_timeout_sec=-1"));
    let c = t.config();
    let text = dump(&c);
    assert!(!text.contains("default enabled (modeled)"));
    assert!(!text.contains("optional (documented default)"));
    assert!(!text.contains("10 (default)"));
    assert!(!text.contains("60 (default)"));
    assert!(text.contains("Unresolved (not a default)"));
}
#[test]
fn uppercase_url_scheme_and_nonpublic_reserved_literals_are_not_misclassified() {
    let t = Temp::new();
    t.user("[mcp_servers.x]\nurl='HTTPS://example.com/private'");
    assert_eq!(t.config().mcp.servers[0].status, Status::Eligible);
    t.user("[mcp_servers.x]\nurl='http://0.0.0.0/mcp'");
    assert_eq!(
        t.config().mcp.servers[0].definitions[0]
            .endpoint
            .as_ref()
            .unwrap()
            .kind,
        EndpointKind::OtherIp
    );
}

#[test]
fn malformed_inactive_project_collection_does_not_disable_user_evidence() {
    let t = Temp::new();
    t.trust("untrusted", STDIO);
    t.write("project/.codex/config.toml", "mcp_servers='PRIVATE'");
    let c = t.config();
    assert!(c.mcp.partial);
    assert_eq!(c.mcp.servers[0].status, Status::Eligible);
}
#[test]
fn invalid_counts_and_tool_overrides_are_unresolved_in_presentation() {
    let t = Temp::new();
    t.user(&format!("{STDIO}\nenv=12\n[mcp_servers.docs.tools.read]\napproval_mode='INVALID'\noutput_token_limit=-1"));
    let c = t.config();
    assert!(dump(&c).contains("Inline environment: Unresolved"));
    assert!(dump(&c).contains("approval Unresolved; output token limit Unresolved"));
}
