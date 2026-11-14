//! The Agents area: Codex and Claude Code side by side. A header row names the columns;
//! the information panel stacks each tool's details under its own heading.
use super::*;
use crate::{
    areas::agents::claude,
    areas::agents::skills,
    areas::agents::{Row, Section, Sources, skill},
};

pub(super) const HELP: &str = "Agents: Codex and Claude Code side by side\n\
Every list has name, Tools (Both / Codex only / Claude only) and Status. Status uses one vocabulary for both tools: Ready, Off, Needs approval, Problem, Unknown (Instructions: Used, Not used). When the tools disagree it shows Codex x · Claude y.\n\
Skills match by folder: Claude usually reaches ~/.agents/skills through symlinks. MCP servers match by name; \"Both, differ\" means the same name points to a different kind of server or program.\n\
Instructions pair AGENTS.md (Codex) and CLAUDE.md (Claude Code) by where they apply: Managed, User, then each project folder from the outermost down.\n\
Settings match by name and show the value; Hooks and Plugins are Claude Code only.\n\
Settings: s shows Codex source files. The panel names the file each setting or MCP server comes from.\n\
Instructions: t shows Codex's chain as if the project were trusted. MCP: V picks the next Codex source.\n\
The information panel shows everything about the selection; PgUp/PgDn scroll it. r refreshes both tools.\n\n\
The panel adds each tool's own words: for skills, Codex's Eligible, Enabled/Disabled in config or Unknown · reason; for MCP, Codex's Locally eligible and Claude Code's Pending approval or Approved. Den does not model Claude rules that could turn a skill off.\n\
Only MCP servers, MCP approvals and project trust are read from ~/.claude.json. URLs, arguments and values are withheld. Hooks, helpers and servers are never run.\n\
When Homebrew installed Codex or Claude Code, the status line names a newer version (↑) from Homebrew's data or Codex's own update check (~/.codex/version.json). Den never checks online.\n\
Nothing here shows what a running Codex or Claude Code session actually loaded.";

/// What one frame shows, computed before drawing so the panel can scroll mutably.
struct View {
    status: String,
    filter: String,
    header: (&'static str, &'static str, &'static str),
    rows: Vec<(String, String, String)>,
    removed: Vec<String>,
    empty: String,
    text: String,
    actions: String,
}

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let view = view(app);
    let label = app.destination().label();
    let body = chrome(
        frame,
        label,
        &view.status,
        app.agents.searching,
        if app.agents.searching {
            actions(true)
        } else {
            &view.actions
        },
    );
    let panes = panes(body);
    if panes[0].width > 0 {
        let block = pane(&format!(" {} ", app.agents.section.label()), true);
        let inner = block.inner(panes[0]);
        frame.render_widget(block, panes[0]);
        let inner = filter_line(
            frame,
            inner,
            view.filter.clone(),
            &app.agents.query,
            app.agents.searching,
        );
        let inner = if inner.height > 2 && !(view.rows.is_empty() && view.removed.is_empty()) {
            // Rows start after the 2-cell selection marker; so does the header.
            let (a, b, c) = view.header;
            frame.render_widget(
                Paragraph::new(columns(a, b, c, inner.width)).style(muted()),
                Rect {
                    x: inner.x + 2,
                    width: inner.width.saturating_sub(2),
                    height: 1,
                    ..inner
                },
            );
            Rect {
                y: inner.y + 1,
                height: inner.height - 1,
                ..inner
            }
        } else {
            inner
        };
        let page = inner.height.max(1) as usize;
        app.agents.page = page;
        if view.rows.is_empty() && view.removed.is_empty() {
            frame.render_widget(
                Paragraph::new(view.empty.as_str()).wrap(Wrap { trim: true }),
                inner,
            );
        } else {
            let selected =
                app.agents.selected[app.agents.slot()].min(view.rows.len().saturating_sub(1));
            let start = (selected / page) * page;
            let mut items: Vec<_> = view
                .rows
                .iter()
                .skip(start)
                .take(page)
                .enumerate()
                .map(|(i, (name, a, b))| {
                    status_item((name, a, b), inner.width, start + i == selected)
                })
                .collect();
            let room = page - items.len();
            items.extend(removed_items(&view.removed, room, inner.width));
            let mut state = ListState::default()
                .with_selected((!view.rows.is_empty()).then_some(selected - start));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_symbol("> ")
                    .highlight_spacing(HighlightSpacing::Always)
                    .highlight_style(status_selection()),
                inner,
                &mut state,
            );
        }
    }
    if panes[1].width > 0 {
        scrolled(frame, panes[1], &view.text, &mut app.agents.panel);
    }
}

fn view(app: &App) -> View {
    let sources = app.agent_sources();
    let a = &app.agents;
    let section = a.section;
    let rows = a.rows(&sources);
    let now = std::time::SystemTime::now();
    let changes = &a.changes;
    let cells: Vec<_> = rows
        .iter()
        .map(|row| {
            let (x, y) = sources.cells(section, row);
            let mark = changes.mark(section.label(), &row.key);
            (format!("{mark}{}", row.name), x, y)
        })
        .collect();
    let removed: Vec<String> = a.removed().iter().map(|e| safe_text(&e.name)).collect();
    let all = sources.rows(section);
    let selected = rows.get(a.selected[a.slot()].min(rows.len().saturating_sub(1)));
    // With no rows the list says why once; the panel only adds what follows (coverage).
    let nothing = if changes.only() && !a.query.is_empty() {
        "No matching changes.\n/:search · d:all rows".into()
    } else if changes.only() && !removed.is_empty() {
        format!(
            "Removed {}: {}.",
            changes.since(now),
            crate::inventory::count(removed.len(), "row")
        )
    } else if changes.only() {
        format!("No changes {}.\nd:all rows", changes.since(now))
    } else {
        empty(app, &sources, section, !a.query.is_empty())
    };
    let (nothing, more) = match nothing.split_once("\n\n") {
        Some((short, more)) => (short.to_owned(), more.to_owned()),
        None => (nothing, String::new()),
    };
    let text = selected.map_or(more, |row| {
        let what = changes.panel(section.label(), &row.key, now);
        format!("{}{what}", panel(app, &sources, section, row))
    });
    let mut filter: Vec<String> = vec![];
    if let Some(text) = changes.filter(section.label(), now) {
        filter.push(text);
    }
    match section {
        Section::Instructions => {
            filter.push("In reading order".into());
            if app.codex.conditional {
                filter.push("Codex IF TRUSTED".into());
            }
        }
        Section::Settings if app.codex.sources => filter.push("Codex source files".into()),
        _ => {}
    }
    let changed = changes.any();
    View {
        status: format!(
            "{}{}",
            status(app, &sources, section, &all),
            changes.status()
        ),
        filter: filter.join(" · "),
        header: match section {
            Section::Instructions => ("Folder", "Tools", "Status"),
            Section::Settings => ("Setting", "Tools", "Value"),
            _ => ("Name", "Tools", "Status"),
        },
        rows: cells,
        removed,
        empty: nothing,
        text,
        actions: format!(
            "{}{}",
            match section {
                Section::Instructions => "t:if trusted ",
                Section::Settings => "s:sources ",
                Section::Skills | Section::Mcp | Section::Hooks | Section::Plugins => "",
            },
            list_actions(false, false, changed)
        ),
    }
}

/// Collection problems first (they matter most), then the count and how rows split.
fn status(app: &App, sources: &Sources, section: Section, all: &[Row]) -> String {
    if let Some(notice) = &app.presentation.notice {
        return notice.clone();
    }
    // The list says so; zero counts would only repeat it.
    if neither_set_up(app).is_some() {
        return String::new();
    }
    let both = all
        .iter()
        .filter(|r| !r.codex.is_empty() && !r.claude.is_empty())
        .count();
    let codex = all.iter().filter(|r| !r.codex.is_empty()).count();
    let claude = all.iter().filter(|r| !r.claude.is_empty()).count();
    let split = format!(
        "{both} in both · {} Codex only · {} Claude only",
        codex - both,
        claude - both
    );
    let text = match section {
        Section::Skills => format!("{} · {split}", crate::inventory::count(all.len(), "skill")),
        Section::Instructions => {
            format!("{} · {split}", crate::inventory::count(all.len(), "folder"))
        }
        Section::Mcp => format!(
            "{} · {split} · {} differ",
            crate::inventory::count(all.len(), "server"),
            all.iter().filter(|r| sources.differs(r)).count()
        ),
        Section::Settings if app.codex.sources => {
            crate::inventory::count(all.len(), "Codex source file")
        }
        Section::Settings => format!(
            "{}{} · {split}",
            if sources
                .codex
                .is_some_and(|i| i.configuration.is_incomplete())
            {
                "Codex INCOMPLETE configuration, values provisional · "
            } else {
                ""
            },
            crate::inventory::count(all.len(), "setting")
        ),
        Section::Hooks => format!(
            "{} · they run automatically; Den never runs them",
            crate::inventory::count(all.len(), "hook")
        ),
        Section::Plugins => crate::inventory::count(all.len(), "plugin"),
    };
    let (mut notes, info) = collection(app);
    notes.extend(updates(app));
    notes.push(text);
    notes.extend(info);
    notes.join(" · ")
}

/// "↑ Codex 0.167.2 available", for a tool Homebrew installed, from Homebrew's data or
/// Codex's own update check. Without Homebrew's record the installed version is unknown.
fn updates(app: &App) -> Option<String> {
    let own = app
        .codex
        .inventory
        .as_ref()
        .and_then(|i| i.update_check.as_ref())
        .map(|c| c.latest.as_str());
    let found: Vec<_> = [
        (
            "Codex",
            crate::updates::tool(&app.inventory, &["codex"], own),
        ),
        (
            "Claude Code",
            crate::updates::tool(&app.inventory, &["claude-code"], None),
        ),
    ]
    .into_iter()
    .filter_map(|(tool, v)| v.map(|v| format!("{tool} {}", safe_text(&v))))
    .collect();
    (!found.is_empty()).then(|| format!("{UPDATE} {} available", found.join(" and ")))
}

/// Each tool's missing home folder, once collected.
fn missing(app: &App) -> (Option<&String>, Option<&String>) {
    (
        app.codex
            .inventory
            .as_ref()
            .and_then(|i| i.missing_home.as_ref()),
        app.claude
            .inventory
            .as_ref()
            .and_then(|i| i.missing_home.as_ref()),
    )
}

/// "Codex and Claude Code are not set up …" when neither tool has its home folder.
fn neither_set_up(app: &App) -> Option<String> {
    match missing(app) {
        (Some(codex), Some(claude)) => Some(format!(
            "Codex and Claude Code are not set up here: no {codex} or {claude}."
        )),
        _ => None,
    }
}

/// Each tool's collection state, when it is not simply loaded: (problems, information).
fn collection(app: &App) -> (Vec<String>, Vec<String>) {
    let (mut problems, mut info) = (vec![], vec![]);
    if app.codex.running || (app.codex.requested && app.codex.context.is_some()) {
        info.push("collecting Codex".into());
    } else if let Some(e) = &app.codex.error {
        problems.push(e.clone());
    } else if app.codex.context.is_none() && !app.skills.loaded {
        info.push("Codex not collected in this mode".into());
    } else if let Some(home) = missing(app).0 {
        info.push(format!("Codex not set up (no {home})"));
    }
    if app.claude.running || (app.claude.requested && app.claude.context.is_some()) {
        info.push("collecting Claude Code".into());
    } else if let Some(e) = &app.claude.error {
        problems.push(e.clone());
    } else if app.claude.context.is_none() {
        info.push("Claude Code not configured".into());
    } else if let Some(home) = missing(app).1 {
        info.push(format!("Claude Code not set up (no {home})"));
    }
    (problems, info)
}

fn empty(app: &App, _: &Sources, section: Section, searching: bool) -> String {
    if searching {
        return "No matches.\n/:search".into();
    }
    if let Some(text) = neither_set_up(app) {
        return text;
    }
    let mut text = match section {
        Section::Skills => "No skills found.",
        Section::Instructions => "No instruction files found.",
        Section::Settings => "No settings found.",
        Section::Mcp => "No MCP servers found.",
        Section::Hooks => "No hooks found.",
        Section::Plugins => "No plugins found.",
    }
    .to_owned();
    let (problems, info) = collection(app);
    for note in problems.iter().chain(&info) {
        text.push_str(&format!("\n{note}"));
    }
    if section == Section::Skills && app.skills.loaded {
        text.push_str(&format!(
            "\n\n{HEADING}Codex discovery coverage\n{}",
            super::skills::coverage(&app.skills)
        ));
    }
    text
}

fn panel(app: &App, sources: &Sources, section: Section, row: &Row) -> String {
    match section {
        Section::Skills => skill_text(app, sources, row),
        Section::Instructions => instruction_text(sources, row),
        Section::Mcp => mcp_text(sources, row),
        Section::Settings => {
            if let (Some(&n), Some(i)) = (row.codex.first(), sources.codex) {
                let c = &i.configuration;
                let text = if sources.codex_sources {
                    c.source_text(n)
                } else {
                    format!(
                        "{}{}\n\ne: open the Codex file that sets it",
                        if c.is_incomplete() {
                            "Configuration incomplete; values may change.\n"
                        } else {
                            ""
                        },
                        c.setting_text(n)
                    )
                };
                let context = format!(
                    "Codex context: trust {:?} · profile {} · {} files present",
                    c.trust,
                    c.profile.as_deref().unwrap_or("none selected"),
                    c.layers.iter().filter(|l| l.present).count()
                );
                return format!(
                    "{HEADING}Codex · {text}\n\n{context}\n\n{HEADING}Codex coverage\n{}",
                    c.findings.join("\n\n")
                );
            }
            claude_item(sources, section, row)
        }
        Section::Hooks | Section::Plugins => claude_item(sources, section, row),
    }
}

fn claude_item(sources: &Sources, section: Section, row: &Row) -> String {
    row.claude
        .first()
        .and_then(|&n| sources.claude_item(section, n))
        .map_or(String::new(), |item| format!("{HEADING}{}", item.detail))
}

/// "Codex: Ready · <Codex's own words>", or that the tool does not have it.
fn tool_line(
    tool: &str,
    status: Option<(crate::areas::agents::Status, String)>,
    missing: &str,
) -> String {
    match status {
        Some((s, why)) if why.is_empty() => format!("{tool}: {}", s.label()),
        Some((s, why)) => format!("{tool}: {} · {why}", s.label()),
        None => format!("{tool}: — {missing}"),
    }
}

fn skill_text(app: &App, sources: &Sources, row: &Row) -> String {
    let Some(s) = skill(sources, row) else {
        return String::new();
    };
    let mut lines = vec![tool_line(
        "Codex",
        sources.codex_status(Section::Skills, row),
        "not in Codex's skill folders",
    )];
    if let Some(&n) = row.codex.first() {
        let codex = &sources.codex_skills.map_or(&[][..], |i| &i.skills)[n];
        lines.push(format!(
            "  Where: {} · {}",
            codex.scope.label(),
            codex.scope.meaning()
        ));
    }
    lines.push(tool_line(
        "Claude Code",
        sources.claude_status(Section::Skills, row),
        "not in Claude Code's skill folders",
    ));
    if let Some(&n) = row.claude.first() {
        let claude = &sources.claude.map_or(&[][..], |c| &c.skills.skills)[n];
        lines.push(format!(
            "  Where: {}",
            match claude.scope {
                skills::Scope::Project => "Project · This project's .claude/skills folder",
                _ => "User · Your Claude Code skills folder (~/.claude/skills)",
            }
        ));
        if let Some(target) = claude.resolved.as_ref().filter(|t| **t != claude.path) {
            lines.push(format!("  Links to: {}", sources.short(target)));
        }
    }
    if !row.codex.is_empty() && !row.claude.is_empty() {
        lines.push("Same folder in both tools, so edits apply to both".into());
    }
    let mut text = super::skills::skill_panel(s, &lines, "either tool");
    if app.skills.loaded {
        text.push_str(&format!(
            "\n\n{HEADING}Codex discovery coverage\n{}",
            super::skills::coverage(&app.skills)
        ));
    }
    if let Some(c) = sources.claude {
        text.push_str(&format!(
            "\n\n{HEADING}Claude Code coverage\n{}",
            c.coverage.join("\n")
        ));
    }
    text
}

fn instruction_text(sources: &Sources, row: &Row) -> String {
    let mut parts = vec![
        format!("{HEADING}{}", row.name),
        [
            tool_line(
                "Codex",
                sources.codex_status(Section::Instructions, row),
                "no AGENTS.md here",
            ),
            tool_line(
                "Claude Code",
                sources.claude_status(Section::Instructions, row),
                "no CLAUDE.md here",
            ),
        ]
        .join("\n"),
    ];
    if let Some(chain) = sources.chain {
        for &n in &row.codex {
            parts.push(format!(
                "{HEADING}Codex · {}\n{}",
                sources.short(&chain.files[n].path),
                chain.text(n, true)
            ));
        }
    }
    if row.codex.is_empty() {
        parts.push(format!("{HEADING}Codex\n— no AGENTS.md here"));
    }
    if let Some(c) = sources.claude {
        for &n in &row.claude {
            parts.push(format!(
                "{HEADING}Claude Code\n{}",
                claude_instruction(sources, c, n)
            ));
        }
    }
    if row.claude.is_empty() {
        parts.push(format!("{HEADING}Claude Code\n— no CLAUDE.md here"));
    }
    parts.join("\n\n")
}

fn claude_instruction(sources: &Sources, inventory: &claude::Inventory, n: usize) -> String {
    let f = &inventory.instructions[n];
    let mut lines = vec![
        sources.short(&f.path),
        format!("{} · {}", f.kind.label(), f.kind.meaning()),
        format!(
            "Order: {} of {} found; later files are read after earlier ones",
            n + 1,
            inventory.instructions.len()
        ),
    ];
    match &f.status {
        claude::Status::Read => {
            lines.push(format!("Size: {}", size(f.bytes)));
            if f.imports > 0 {
                lines.push(format!(
                    "@imports: {}; Claude Code may load them, Den does not follow them",
                    f.imports
                ));
            }
            lines.push(String::new());
            lines.push(f.preview.clone());
        }
        claude::Status::Empty => lines.push("Empty file".into()),
        claude::Status::Link(target) => lines.push(format!(
            "Link to {}; links are reported, not read",
            target
                .as_ref()
                .map_or("an unreadable target".into(), |t| sources.short(t))
        )),
        claude::Status::Unreadable => {
            lines.push("Unreadable, oversized, binary or not UTF-8; contents withheld".into())
        }
    }
    lines.join("\n")
}

fn mcp_text(sources: &Sources, row: &Row) -> String {
    let mut parts = vec![
        format!("{HEADING}{}", row.name),
        [
            tool_line(
                "Codex",
                sources.codex_status(Section::Mcp, row),
                "not configured in Codex",
            ),
            tool_line(
                "Claude Code",
                sources.claude_status(Section::Mcp, row),
                "not configured in Claude Code",
            ),
        ]
        .join("\n"),
    ];
    if !row.codex.is_empty() && !row.claude.is_empty() {
        parts.push(if sources.differs(row) {
            "Same name in both tools, but a different kind of server or program: check which one you expect.".into()
        } else {
            "Same name in both tools.".into()
        });
    }
    parts.push("Den never starts or connects to MCP servers.".into());
    match (row.codex.first(), sources.codex) {
        (Some(&n), Some(i)) => parts.push(format!(
            "{HEADING}Codex\n{}",
            // The row name is already the heading; drop the server's own name line.
            i.configuration.mcp.servers[n]
                .panel(&i.configuration.layers)
                .split_once('\n')
                .map_or(String::new(), |(_, rest)| rest.to_owned())
        )),
        _ => parts.push(format!("{HEADING}Codex\n— not configured in Codex")),
    }
    for &n in &row.claude {
        if let Some(item) = sources.claude_item(Section::Mcp, n) {
            parts.push(format!(
                "{HEADING}Claude Code · {}\n{}",
                item.source, item.detail
            ));
        }
    }
    if row.claude.is_empty() {
        parts.push(format!(
            "{HEADING}Claude Code\n— not configured in Claude Code"
        ));
    }
    parts.join("\n\n")
}
