//! The Agents area: Codex and Claude Code side by side.
//!
//! Collectors, privacy rules and per-tool semantics stay in `codex`, `skills` and
//! `claude`. This module only matches their results into rows: skills by resolved folder,
//! MCP servers by name, instruction files by folder. Settings, Hooks and Plugins do not map
//! across tools, so they are single lists that name the tool.
use crate::{
    claude, codex,
    inventory::{matches_query, safe_text, short_path},
    presentation::PanelScroll,
    skills::{self, Skill},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    #[default]
    Skills,
    Instructions,
    Settings,
    Mcp,
    Hooks,
    Plugins,
}
impl Section {
    pub const ALL: [Self; 6] = [
        Self::Skills,
        Self::Instructions,
        Self::Settings,
        Self::Mcp,
        Self::Hooks,
        Self::Plugins,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Skills => "Skills",
            Self::Instructions => "Instructions",
            Self::Settings => "Settings",
            Self::Mcp => "MCP",
            Self::Hooks => "Hooks",
            Self::Plugins => "Plugins",
        }
    }
    /// Shift+Tab order.
    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }
}

/// One status vocabulary for both tools, so the Status column means the same thing
/// whichever tool a row comes from. Instructions use Used / Not used instead of Ready / Off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready,
    Off,
    NeedsApproval,
    Problem,
    Unknown,
    Used,
    NotUsed,
}
impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Off => "Off",
            Self::NeedsApproval => "Needs approval",
            Self::Problem => "Problem",
            Self::Unknown => "Unknown",
            Self::Used => "Used",
            Self::NotUsed => "Not used",
        }
    }
}

/// One row: what each tool has for it, as indices into that tool's inventory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub key: String,
    pub name: String,
    pub codex: Vec<usize>,
    pub claude: Vec<usize>,
}

#[derive(Default)]
pub struct State {
    pub section: Section,
    /// Selected position in the visible rows, per section.
    pub selected: [usize; 6],
    pub query: String,
    pub searching: bool,
    pub panel: PanelScroll,
    pub page: usize,
}

/// Everything the Agents area reads, borrowed from the per-tool states.
pub struct Sources<'a> {
    /// Codex skills, once collected (also in the skills-only fixture).
    pub codex_skills: Option<&'a skills::Inventory>,
    pub enablement: &'a [codex::Enablement],
    pub codex: Option<&'a codex::Inventory>,
    pub chain: Option<&'a codex::instructions::Chain>,
    /// Settings shows Codex source files instead of settings (`s`).
    pub codex_sources: bool,
    pub claude: Option<&'a claude::Inventory>,
    pub project: Option<&'a Path>,
    pub home: Option<&'a Path>,
}

impl Sources<'_> {
    pub fn short(&self, path: &Path) -> String {
        match self.project {
            Some(project) => short_path(path, project, self.home),
            None => safe_text(&path.display().to_string()),
        }
    }
    fn claude_items(&self, section: Section) -> &[claude::config::Item] {
        let Some(c) = self.claude else {
            return &[];
        };
        match section {
            Section::Settings => &c.config.settings,
            Section::Mcp => &c.config.mcp,
            Section::Hooks => &c.config.hooks,
            Section::Plugins => &c.config.plugins,
            _ => &[],
        }
    }
    pub fn claude_item(&self, section: Section, n: usize) -> Option<&claude::config::Item> {
        self.claude_items(section).get(n)
    }

    /// All rows of a section, before filters and search.
    pub fn rows(&self, section: Section) -> Vec<Row> {
        match section {
            Section::Skills => self.skill_rows(),
            Section::Instructions => self.instruction_rows(),
            Section::Mcp => self.mcp_rows(),
            Section::Settings if self.codex_sources => self
                .codex
                .map(|i| {
                    i.configuration
                        .layers
                        .iter()
                        .enumerate()
                        .map(|(n, l)| Row {
                            key: format!("codex-source:{}", l.path.display()),
                            name: self.short(&l.path),
                            codex: vec![n],
                            claude: vec![],
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Section::Settings => {
                // One row per setting name: a key both tools set (like model) is one row.
                let mut rows: Vec<Row> = vec![];
                for (n, s) in self
                    .codex
                    .into_iter()
                    .flat_map(|i| i.configuration.settings.iter().enumerate())
                {
                    rows.push(Row {
                        key: format!("setting:{}", s.key),
                        name: s.key.clone(),
                        codex: vec![n],
                        claude: vec![],
                    });
                }
                for (n, item) in self.claude_items(section).iter().enumerate() {
                    match rows
                        .iter_mut()
                        .find(|r| r.name == item.name && !r.codex.is_empty())
                    {
                        Some(row) => row.claude.push(n),
                        None => rows.push(Row {
                            key: format!("setting:{}", item.key),
                            name: item.name.clone(),
                            codex: vec![],
                            claude: vec![n],
                        }),
                    }
                }
                rows
            }
            Section::Hooks | Section::Plugins => self.claude_rows(section),
        }
    }
    fn claude_rows(&self, section: Section) -> Vec<Row> {
        self.claude_items(section)
            .iter()
            .enumerate()
            .map(|(n, item)| Row {
                key: format!("claude:{}", item.key),
                name: item.name.clone(),
                codex: vec![],
                claude: vec![n],
            })
            .collect()
    }
    /// One row per skill folder: Codex and Claude reach the same folder through symlinks.
    fn skill_rows(&self) -> Vec<Row> {
        let mut rows: BTreeMap<(String, PathBuf), Row> = BTreeMap::new();
        let id = |s: &Skill| s.resolved.clone().unwrap_or_else(|| s.path.clone());
        let mut add = |s: &Skill, n: usize, codex: bool| {
            let row = rows
                .entry((s.name.to_lowercase(), id(s)))
                .or_insert_with(|| Row {
                    key: format!("skill:{}", id(s).display()),
                    name: s.name.clone(),
                    ..Row::default()
                });
            if codex {
                row.codex.push(n);
            } else {
                row.claude.push(n);
            }
        };
        for (n, s) in self
            .codex_skills
            .map_or(&[][..], |i| &i.skills)
            .iter()
            .enumerate()
        {
            add(s, n, true);
        }
        for (n, s) in self
            .claude
            .map_or(&[][..], |i| &i.skills.skills)
            .iter()
            .enumerate()
        {
            add(s, n, false);
        }
        rows.into_values().collect()
    }
    /// One row per server name; `differs` marks names that point to different servers.
    fn mcp_rows(&self) -> Vec<Row> {
        let mut rows: BTreeMap<String, Row> = BTreeMap::new();
        for (n, s) in self
            .codex
            .map_or(&[][..], |i| &i.configuration.mcp.servers)
            .iter()
            .enumerate()
        {
            rows.entry(s.name.clone())
                .or_insert_with(|| Row {
                    key: format!("mcp:{}", s.name),
                    name: s.name.clone(),
                    ..Row::default()
                })
                .codex
                .push(n);
        }
        for (n, item) in self.claude_items(Section::Mcp).iter().enumerate() {
            rows.entry(item.name.clone())
                .or_insert_with(|| Row {
                    key: format!("mcp:{}", item.name),
                    name: item.name.clone(),
                    ..Row::default()
                })
                .claude
                .push(n);
        }
        rows.into_values().collect()
    }
    /// Instruction files paired by where they apply: Managed, User, then each project
    /// folder from the outermost down.
    fn instruction_rows(&self) -> Vec<Row> {
        let mut rows: BTreeMap<(u8, usize, String), Row> = BTreeMap::new();
        let mut add = |group: (u8, usize, String), name: String, n: usize, codex: bool| {
            let row = rows.entry(group.clone()).or_insert_with(|| Row {
                key: format!("instructions:{}", group.2),
                name,
                ..Row::default()
            });
            if codex {
                row.codex.push(n);
            } else {
                row.claude.push(n);
            }
        };
        let folder = |dir: &Path| (2u8, dir.components().count(), dir.display().to_string());
        if let Some(chain) = self.chain {
            for (n, f) in chain.files.iter().enumerate() {
                if f.status == codex::instructions::Status::Missing {
                    continue;
                }
                if f.global {
                    add((1, 0, "user".into()), "User".into(), n, true);
                } else if let Some(dir) = f.path.parent() {
                    add(folder(dir), self.short(&dir.join("")), n, true);
                }
            }
        }
        for (n, f) in self
            .claude
            .map_or(&[][..], |i| &i.instructions)
            .iter()
            .enumerate()
        {
            match f.kind {
                claude::Kind::Managed => add((0, 0, "managed".into()), "Managed".into(), n, false),
                claude::Kind::User => add((1, 0, "user".into()), "User".into(), n, false),
                _ => {
                    let Some(mut dir) = f.path.parent() else {
                        continue;
                    };
                    // .claude/CLAUDE.md belongs to the folder that holds .claude.
                    if dir.file_name().is_some_and(|n| n == ".claude") {
                        dir = dir.parent().unwrap_or(dir);
                    }
                    add(folder(dir), self.short(&dir.join("")), n, false);
                }
            }
        }
        rows.into_values().collect()
    }

    /// Which tools have this row.
    pub fn tools(&self, section: Section, row: &Row) -> &'static str {
        match (row.codex.is_empty(), row.claude.is_empty()) {
            (false, false) if section == Section::Mcp && self.differs(row) => "Both, differ",
            (false, false) => "Both",
            (false, true) => "Codex only",
            _ => "Claude only",
        }
    }
    /// The Tools and Status cells. When both tools have a row and agree, one status;
    /// otherwise "Codex x · Claude y". Settings show the value instead of a status.
    pub fn cells(&self, section: Section, row: &Row) -> (String, String) {
        let tools = self.tools(section, row).to_owned();
        let (codex, claude) = (
            self.codex_cell(section, row),
            self.claude_cell(section, row),
        );
        let status = match (codex, claude) {
            (Some(a), Some(b)) if a == b => a,
            (Some(a), Some(b)) => format!("Codex {a} · Claude {b}"),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => "—".into(),
        };
        (tools, status)
    }
    fn codex_cell(&self, section: Section, row: &Row) -> Option<String> {
        if section == Section::Settings {
            let c = &self.codex?.configuration;
            let &n = row.codex.first()?;
            if self.codex_sources {
                let l = &c.layers[n];
                return Some(format!("{:?} · {}", l.activity, l.status));
            }
            let s = &c.settings[n];
            // The warning first, so it survives a narrow column.
            return Some(format!(
                "{}{}",
                if c.is_incomplete() {
                    "Provisional · "
                } else {
                    ""
                },
                s.value
            ));
        }
        self.codex_status(section, row)
            .map(|(s, _)| s.label().to_owned())
    }
    fn claude_cell(&self, section: Section, row: &Row) -> Option<String> {
        if section == Section::Settings {
            return row
                .claude
                .first()
                .and_then(|&n| self.claude_item(section, n))
                .map(|item| item.summary.clone());
        }
        let (status, detail) = self.claude_status(section, row)?;
        Some(match section {
            // The status word, then what the hook runs or what the plugin brings.
            Section::Hooks | Section::Plugins if !detail.is_empty() => {
                format!("{} · {detail}", status.label())
            }
            _ => status.label().to_owned(),
        })
    }
    /// Codex's status for a row, with Codex's own words for the panel.
    pub fn codex_status(&self, section: Section, row: &Row) -> Option<(Status, String)> {
        let &n = row.codex.first()?;
        match section {
            Section::Skills => {
                let skill = self.codex_skills?.skills.get(n)?;
                let (label, why) = codex_skill_status(self.enablement, n);
                Some((skill_status(skill, &label), format!("{label}: {why}")))
            }
            Section::Mcp => {
                let server = self.codex?.configuration.mcp.servers.get(n)?;
                use codex::mcp::Status as S;
                let status = match server.status {
                    S::Eligible => Status::Ready,
                    S::Configured | S::Disabled => Status::Off,
                    S::Invalid => Status::Problem,
                    S::Conditional | S::PluginUnresolved => Status::Unknown,
                };
                Some((status, server.status.label().to_owned()))
            }
            Section::Instructions => {
                use codex::instructions::Status as S;
                let chain = self.chain?;
                let files: Vec<_> = row.codex.iter().map(|&n| chain.files[n].status).collect();
                let status = if files.contains(&S::Unavailable) {
                    Status::Problem
                } else if files
                    .iter()
                    .any(|s| matches!(s, S::Selected | S::Truncated))
                {
                    Status::Used
                } else {
                    Status::NotUsed
                };
                Some((status, String::new()))
            }
            _ => None,
        }
    }
    /// Claude Code's status for a row, and the detail shown after it.
    pub fn claude_status(&self, section: Section, row: &Row) -> Option<(Status, String)> {
        let &n = row.claude.first()?;
        match section {
            Section::Skills => {
                let skill = self.claude?.skills.skills.get(n)?;
                let (label, why) = claude_skill_status(skill);
                Some((skill_status(skill, label), why.to_owned()))
            }
            Section::Mcp => {
                let item = self.claude_winner(row)?;
                let label = item.summary.split(" · ").next().unwrap_or("");
                let status = if label.starts_with("Approved by project settings, if") {
                    Status::Unknown
                } else if label.starts_with("Configured") || label.starts_with("Approved") {
                    Status::Ready
                } else if label.starts_with("Pending") {
                    Status::NeedsApproval
                } else if label.starts_with("Not used") {
                    Status::Problem
                } else {
                    // Rejected, turned off, blocked or shadowed.
                    Status::Off
                };
                Some((status, label.to_owned()))
            }
            Section::Instructions => {
                let files = &self.claude?.instructions;
                let statuses: Vec<_> = row.claude.iter().map(|&n| &files[n].status).collect();
                let status = if statuses.contains(&&claude::Status::Unreadable) {
                    Status::Problem
                } else if statuses.contains(&&claude::Status::Read) {
                    Status::Used
                } else if statuses
                    .iter()
                    .any(|s| matches!(s, claude::Status::Link(_)))
                {
                    Status::Unknown
                } else {
                    Status::NotUsed
                };
                Some((status, String::new()))
            }
            Section::Hooks => {
                let item = self.claude_item(section, n)?;
                let off = ["Off (", "Blocked (", "Plugin not enabled"];
                if off.iter().any(|p| item.summary.starts_with(p)) {
                    let command = item
                        .summary
                        .split_once(" · ")
                        .map_or(String::new(), |(_, rest)| rest.to_owned());
                    Some((Status::Off, command))
                } else {
                    Some((Status::Ready, item.summary.clone()))
                }
            }
            Section::Plugins => {
                let item = self.claude_item(section, n)?;
                let status = match (item.source.as_str(), item.summary.as_str()) {
                    (_, "Not installed") => Status::Problem,
                    ("Enabled", _) => Status::Ready,
                    _ => Status::Off,
                };
                Some((status, item.summary.clone()))
            }
            Section::Settings => None,
        }
    }
    /// Claude's entry that wins for this name (others are shadowed).
    pub fn claude_winner(&self, row: &Row) -> Option<&claude::config::Item> {
        let items = self.claude_items(Section::Mcp);
        row.claude
            .iter()
            .map(|&n| &items[n])
            .find(|i| !i.summary.starts_with("Shadowed"))
            .or_else(|| row.claude.first().map(|&n| &items[n]))
    }
    /// Same name in both tools, but a different kind of server or a different program.
    pub fn differs(&self, row: &Row) -> bool {
        let (Some(&n), Some(item), Some(i)) =
            (row.codex.first(), self.claude_winner(row), self.codex)
        else {
            return false;
        };
        let Some((kind, program)) = &item.endpoint else {
            return false;
        };
        let server = &i.configuration.mcp.servers[n];
        let codex_stdio = server.transport() == codex::mcp::Transport::Stdio;
        if codex_stdio != (kind == "stdio") {
            return true;
        }
        let codex_program = server
            .selected
            .and_then(|d| server.definitions.get(d))
            .or(server.definitions.first())
            .and_then(|d| d.launcher.split_whitespace().next())
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .map_or(p.to_owned(), |n| n.to_string_lossy().into_owned())
            });
        codex_stdio && matches!((codex_program, program), (Some(a), Some(b)) if a != *b)
    }
}

/// Ready, Off, Problem or Unknown for a skill, from its files and the tool's own label.
fn skill_status(skill: &Skill, label: &str) -> Status {
    match skill.unknown_reason() {
        Some(("Unknown · not checked", _)) => Status::Unknown,
        Some(_) => Status::Problem,
        None if label == "Disabled in config" => Status::Off,
        None if label.starts_with("Unknown") || label == "Not evaluated" => Status::Unknown,
        None => Status::Ready,
    }
}

/// Codex's status for a skill: Eligible, Enabled/Disabled in config or Unknown · reason.
pub fn codex_skill_status(enablement: &[codex::Enablement], n: usize) -> (String, String) {
    enablement.get(n).map_or(
        (
            "Not evaluated".into(),
            "Codex configuration was not read".into(),
        ),
        |e| (e.label.clone(), e.explanation.clone()),
    )
}

/// Claude's status for a skill: the same Unknown · reason values, otherwise Valid. Den
/// does not model Claude Code rules that could turn a skill off, so it never says Eligible.
pub fn claude_skill_status(s: &Skill) -> (&'static str, &'static str) {
    s.unknown_reason().unwrap_or((
        "Valid",
        "Valid SKILL.md in a Claude Code skills folder; Claude Code rules that could turn it off are not modeled",
    ))
}

impl State {
    pub fn slot(&self) -> usize {
        self.section as usize
    }
    /// The rows shown, after the search.
    pub fn rows(&self, sources: &Sources) -> Vec<Row> {
        sources
            .rows(self.section)
            .into_iter()
            .filter(|row| {
                let (a, b) = sources.cells(self.section, row);
                // Skills also match their description, as before.
                let description = if self.section == Section::Skills {
                    skill(sources, row).map_or("", |s| s.description.as_str())
                } else {
                    ""
                };
                matches_query(
                    &self.query,
                    &safe_text(&format!("{} {a} {b} {description}", row.name)),
                )
            })
            .collect()
    }
    pub fn selected_row(&self, sources: &Sources) -> Option<Row> {
        self.rows(sources).get(self.selected[self.slot()]).cloned()
    }
    pub fn clamp(&mut self, sources: &Sources) {
        let len = self.rows(sources).len();
        let slot = self.slot();
        self.selected[slot] = self.selected[slot].min(len.saturating_sub(1));
    }
    pub fn select_key(&mut self, sources: &Sources, key: &str) {
        if let Some(n) = self.rows(sources).iter().position(|r| r.key == key) {
            self.selected[self.slot()] = n;
        }
    }
}

/// The skill a Skills row shows: Codex's copy if Codex has it, else Claude's. Both point
/// to the same folder, so their contents are the same.
pub fn skill<'a>(sources: &'a Sources, row: &Row) -> Option<&'a Skill> {
    row.codex
        .first()
        .and_then(|&n| sources.codex_skills.and_then(|i| i.skills.get(n)))
        .or_else(|| {
            row.claude
                .first()
                .and_then(|&n| sources.claude.and_then(|c| c.skills.skills.get(n)))
        })
}

impl crate::app::App {
    /// The per-tool results the Agents area reads.
    pub fn agent_sources(&self) -> Sources<'_> {
        let (project, home) = if let Some(c) = &self.claude.context {
            (Some(c.project.as_path()), c.home.as_deref())
        } else if let Some(c) = &self.codex.context {
            (Some(c.project.as_path()), c.home.as_deref())
        } else {
            (
                self.skills.config.as_ref().map(|c| c.project.as_path()),
                None,
            )
        };
        Sources {
            codex_skills: self.skills.loaded.then_some(&self.skills.inventory),
            enablement: &self.skills.enablement,
            codex: self.codex.inventory.as_ref(),
            chain: self.codex.chain(),
            codex_sources: self.codex.sources,
            claude: self.claude.inventory.as_ref(),
            project,
            home,
        }
    }
    fn agent_rows(&self) -> Vec<Row> {
        self.agents.rows(&self.agent_sources())
    }
    pub fn agent_row(&self) -> Option<Row> {
        self.agents.selected_row(&self.agent_sources())
    }
    pub fn agents_clamp(&mut self) {
        let len = self.agent_rows().len();
        let slot = self.agents.slot();
        self.agents.selected[slot] = self.agents.selected[slot].min(len.saturating_sub(1));
    }
    /// Select the row with this key, if it is visible.
    pub fn agents_select(&mut self, key: &str) {
        if let Some(n) = self.agent_rows().iter().position(|r| r.key == key) {
            let slot = self.agents.slot();
            self.agents.selected[slot] = n;
        }
    }
    /// A new selection starts the panel at the top.
    fn agents_reset_panel(&mut self) {
        self.agents.panel.offset = 0;
    }
    /// Finish a collection and keep the same row selected, matched by its key.
    fn keep_selection(&mut self, finish: impl FnOnce(&mut Self)) {
        let key = self.agent_row().map(|r| r.key);
        finish(self);
        if let Some(key) = key {
            self.agents_select(&key);
        }
        self.agents_clamp();
    }
    pub fn finish_codex(&mut self, result: std::io::Result<codex::Inventory>) {
        self.keep_selection(|app| app.codex.finish(result, &mut app.skills));
    }
    pub fn finish_claude(&mut self, result: std::io::Result<claude::Inventory>) {
        self.keep_selection(|app| app.claude.finish(result));
    }
    pub fn finish_skills(&mut self, result: std::io::Result<skills::Inventory>) {
        self.keep_selection(|app| app.skills.finish(result));
    }
    /// `r`: refresh every configured collection; snapshots and fixtures stay offline.
    pub fn request_agents(&mut self) {
        if self.codex.context.is_some() && !self.codex.running {
            self.codex.requested = true;
        }
        if self.claude.context.is_some() && !self.claude.running {
            self.claude.requested = true;
        }
        if self.codex.context.is_none() && self.skills.config.is_some() && !self.skills.running {
            self.skills.requested = true;
        }
    }
    pub(crate) fn agents_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let a = &mut self.agents;
        if a.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => a.searching = false,
                KeyCode::Backspace => {
                    a.query.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    a.query.clear()
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && a.query.len() < 256
                        && safe_text(&ch.to_string()) == ch.to_string() =>
                {
                    a.query.push(ch)
                }
                _ => {}
            }
            let slot = a.slot();
            a.selected[slot] = 0;
            self.agents_reset_panel();
            self.agents_clamp();
            return;
        }
        let section = self.agents.section;
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') => self.agents.searching = true,
            KeyCode::Char('r') => self.request_agents(),
            KeyCode::Down | KeyCode::Char('j') => self.agents_move(1),
            KeyCode::Up | KeyCode::Char('k') => self.agents_move(-1),
            KeyCode::Home => self.agents_move(-isize::MAX),
            KeyCode::End => self.agents_move(isize::MAX),
            KeyCode::PageDown => self.agents.panel.by(1),
            KeyCode::PageUp => self.agents.panel.by(-1),
            KeyCode::Char('s')
                if section == Section::Settings && self.codex.inventory.is_some() =>
            {
                self.codex.sources = !self.codex.sources;
                self.agents.selected[Section::Settings as usize] = 0;
                self.agents_reset_panel();
            }
            KeyCode::Char('t') if section == Section::Instructions => {
                self.codex.conditional = !self.codex.conditional;
                self.agents_reset_panel();
                self.agents_clamp();
            }
            KeyCode::Char('V') if section == Section::Mcp => {
                self.codex.mcp_source = self.codex.mcp_source.wrapping_add(1);
                self.agents.panel.offset = 0;
            }
            _ => {}
        }
    }
    fn agents_move(&mut self, delta: isize) {
        let len = self.agent_rows().len();
        let slot = self.agents.slot();
        let before = self.agents.selected[slot];
        self.agents.selected[slot] = before
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        if self.agents.selected[slot] != before {
            self.agents_reset_panel();
        }
    }
    /// The Codex configuration layer that defines the selected row, for `e`.
    pub(crate) fn agents_source(&self) -> Option<usize> {
        let sources = self.agent_sources();
        let row = self.agents.selected_row(&sources)?;
        let &n = row.codex.first()?;
        let config = &self.codex.inventory.as_ref()?.configuration;
        match self.agents.section {
            Section::Skills => self.skills.enablement.get(n).and_then(|e| e.source),
            Section::Settings if !self.codex.sources => config
                .settings
                .get(n)
                .and_then(|s| s.source.or_else(|| s.conditional.as_ref().map(|(_, l)| *l))),
            Section::Mcp => {
                let server = config.mcp.servers.get(n)?;
                server
                    .definitions
                    .get(self.codex.mcp_source % server.definitions.len().max(1))
                    .map(|d| d.source)
            }
            _ => None,
        }
    }
    /// Show a Codex configuration layer in Settings › source files.
    pub(crate) fn agents_jump_source(&mut self, layer: usize) {
        let Some(path) = self
            .codex
            .inventory
            .as_ref()
            .and_then(|i| i.configuration.layers.get(layer))
            .map(|l| l.path.display().to_string())
        else {
            return;
        };
        self.agents.section = Section::Settings;
        self.codex.sources = true;
        self.agents.query.clear();
        self.agents_reset_panel();
        self.agents_select(&format!("codex-source:{path}"));
    }
}
