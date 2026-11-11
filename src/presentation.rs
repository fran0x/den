//! Area → section → resource. Tab switches areas, Shift+Tab sections; each section owns its Back history.
mod location;
use crate::{
    agents::Section as AgentSection,
    app::{App, Focus, Module},
    diagnostics::Tab,
    explorer::Section,
    inventory::matches_query,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use location::Location;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Destination {
    Homebrew,
    Unresolved,
    Missing,
    Leaves,
    Cleanup,
    Skills,
    Instructions,
    Settings,
    /// Settings showing Codex source files (`s`, or `e` from a row).
    Sources,
    Mcp,
    Hooks,
    Plugins,
    /// Cloud › AWS profiles.
    Aws,
    Containers,
    Images,
    Volumes,
}
impl Destination {
    pub fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew / Packages",
            Self::Unresolved | Self::Missing | Self::Leaves | Self::Cleanup => {
                "Homebrew / Diagnostics"
            }
            Self::Skills => "Agents / Skills",
            Self::Instructions => "Agents / Instructions",
            Self::Settings | Self::Sources => "Agents / Settings",
            Self::Mcp => "Agents / MCP",
            Self::Hooks => "Agents / Hooks",
            Self::Plugins => "Agents / Plugins",
            Self::Aws => "Cloud / AWS",
            Self::Containers => "Docker / Containers",
            Self::Images => "Docker / Images",
            Self::Volumes => "Docker / Volumes",
        }
    }
    /// The Agents section this destination shows, if it is in the Agents area.
    pub fn agents_section(self) -> Option<AgentSection> {
        Some(match self {
            Self::Skills => AgentSection::Skills,
            Self::Instructions => AgentSection::Instructions,
            Self::Settings | Self::Sources => AgentSection::Settings,
            Self::Mcp => AgentSection::Mcp,
            Self::Hooks => AgentSection::Hooks,
            Self::Plugins => AgentSection::Plugins,
            _ => return None,
        })
    }
    fn of_agents(section: AgentSection) -> Self {
        match section {
            AgentSection::Skills => Self::Skills,
            AgentSection::Instructions => Self::Instructions,
            AgentSection::Settings => Self::Settings,
            AgentSection::Mcp => Self::Mcp,
            AgentSection::Hooks => Self::Hooks,
            AgentSection::Plugins => Self::Plugins,
        }
    }
    /// The top-level section this destination belongs to; Shift+Tab cycles sections.
    pub fn section(self) -> Self {
        match self {
            Self::Missing | Self::Leaves | Self::Cleanup => Self::Unresolved,
            Self::Sources => Self::Settings,
            other => other,
        }
    }
}
/// Scroll position of an information panel; PgUp/PgDn move it by `page` lines.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelScroll {
    pub offset: u16,
    pub max: u16,
    pub page: u16,
}
impl PanelScroll {
    pub fn by(&mut self, pages: i32) {
        let step = i32::from(self.page.max(1));
        self.offset = (i32::from(self.offset) + pages * step).clamp(0, i32::from(self.max)) as u16;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Overview,
    Unresolved,
    Missing,
    Leaves,
    Cleanup,
    RunCheck,
    OriginalOutput,
    FullFinding,
}
impl Detail {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Unresolved => "Unresolved",
            Self::Missing => "Missing",
            Self::Leaves => "Leaves",
            Self::Cleanup => "Cleanup preview",
            Self::RunCheck => "Run selected Homebrew check",
            Self::OriginalOutput => "Original Homebrew report",
            Self::FullFinding => "Full finding / summary",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InspectorFocus {
    Sections,
    Content,
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Browse,
    Inspector {
        detail: Detail,
        focus: InspectorFocus,
        selected: usize,
        query: String,
    },
}
#[derive(Default)]
pub struct Presentation {
    pub page: Page,
    /// Back history per section. Switching areas or sections never adds a Back step;
    /// a jump to another section returns with Back from the section it landed in.
    history: BTreeMap<Destination, Vec<Location>>,
    /// The latest context, filed under the section the action lands in.
    pending: Option<Location>,
    saved: BTreeMap<Destination, Location>,
    areas: [Option<Location>; 4],
    pub(super) revision: u64,
    pub menu_searching: bool,
    pub notice: Option<String>,
}
impl App {
    /// Area switching remembers the complete view. Tab: Homebrew → Agents → Cloud → Docker.
    pub fn switch_area(&mut self) {
        self.commit_history();
        let current = self.module.index();
        self.presentation.areas[current] = Some(self.location());
        self.presentation.menu_searching = false;
        self.presentation.notice = None;
        let next = (current + 1) % 4;
        if let Some(location) = self.presentation.areas[next].clone() {
            self.restore(location);
        } else {
            self.navigate(
                [
                    Destination::Homebrew,
                    Destination::Skills,
                    Destination::Aws,
                    Destination::Containers,
                ][next],
            );
        }
    }
    pub(crate) fn focus_inventory(&mut self) {
        if self.module == Module::Homebrew {
            self.focus = Focus::Inventory;
        }
    }
    /// Shift+Tab: Homebrew Packages ↔ Diagnostics; Agents Skills → Instructions → Settings →
    /// MCP → Hooks → Plugins. Each section restores its last context.
    pub fn next_section(&mut self) {
        let next = match self.destination().section() {
            Destination::Homebrew if self.is_homebrew() => Destination::Unresolved,
            Destination::Homebrew => return,
            Destination::Unresolved => Destination::Homebrew,
            // Cloud has one section so far; Kubernetes would be the next.
            Destination::Aws => return,
            Destination::Containers => Destination::Images,
            Destination::Images => Destination::Volumes,
            Destination::Volumes => Destination::Containers,
            d => Destination::of_agents(d.agents_section().unwrap_or_default().next()),
        };
        self.searching = false;
        self.agents.searching = false;
        self.docker.searching = false;
        self.presentation.menu_searching = false;
        self.navigate(next);
    }
    pub fn destination(&self) -> Destination {
        match self.module {
            Module::Homebrew => match self.diagnostics.tab {
                Some(Tab::Unresolved) => Destination::Unresolved,
                Some(Tab::Missing) => Destination::Missing,
                Some(Tab::Leaves) => Destination::Leaves,
                Some(Tab::Cleanup) => Destination::Cleanup,
                None => Destination::Homebrew,
            },
            Module::Agents
                if self.agents.section == AgentSection::Settings && self.codex.sources =>
            {
                Destination::Sources
            }
            Module::Agents => Destination::of_agents(self.agents.section),
            Module::Cloud => Destination::Aws,
            Module::Docker => match self.docker.section {
                crate::docker::Section::Containers => Destination::Containers,
                crate::docker::Section::Images => Destination::Images,
                crate::docker::Section::Volumes => Destination::Volumes,
            },
        }
    }
    pub fn inspecting(&self) -> bool {
        matches!(self.presentation.page, Page::Inspector { .. })
    }
    pub fn active_focus(&self) -> Focus {
        match self.module {
            Module::Homebrew => self.focus,
            Module::Agents | Module::Cloud | Module::Docker => Focus::Inventory,
        }
    }
    pub fn active_searching(&self) -> bool {
        match self.module {
            Module::Homebrew => self.searching,
            Module::Agents => self.agents.searching,
            Module::Cloud => self.aws.searching,
            Module::Docker => self.docker.searching,
        }
    }
    fn focus_content(&mut self) {
        if self.module == Module::Homebrew {
            self.focus = Focus::Details;
        }
    }
    pub(crate) fn remember(&mut self) {
        self.push_location(self.location());
    }
    fn push_location(&mut self, location: Location) {
        self.commit_history();
        self.presentation.pending = Some(location);
        self.presentation.revision = self.presentation.revision.wrapping_add(1);
    }
    /// File the pending context under the section the last action landed in.
    fn commit_history(&mut self) {
        if let Some(location) = self.presentation.pending.take() {
            let history = self
                .presentation
                .history
                .entry(self.destination().section())
                .or_default();
            if history.len() == 64 {
                history.remove(0);
            }
            history.push(location);
        }
    }
    pub fn back(&mut self) {
        self.presentation.notice = None;
        self.commit_history();
        let section = self.destination().section();
        if let Some(location) = self
            .presentation
            .history
            .get_mut(&section)
            .and_then(Vec::pop)
        {
            self.presentation
                .saved
                .insert(self.destination(), self.location());
            self.restore(location);
            self.presentation.revision = self.presentation.revision.wrapping_add(1);
        } else if self.module == Module::Homebrew && self.diagnostics.tab.is_some() {
            // Diagnostics is a section, not a drill-in: its first page has nothing behind it.
        } else {
            self.presentation.page = Page::Browse;
            if self.module == Module::Homebrew {
                self.focus = Focus::Inventory;
            }
        }
    }
    pub fn navigate(&mut self, destination: Destination) {
        self.presentation.notice = None;
        if destination.section() == self.destination().section() {
            return;
        }
        // Switching sections never adds a Back step; each section keeps its own history.
        self.commit_history();
        self.presentation.areas[self.module.index()] = Some(self.location());
        self.presentation
            .saved
            .insert(self.destination().section(), self.location());
        if let Some(location) = self.presentation.saved.get(&destination.section()).cloned() {
            self.restore(location);
            return;
        }
        self.presentation.page = Page::Browse;
        match destination {
            Destination::Homebrew
            | Destination::Unresolved
            | Destination::Missing
            | Destination::Leaves
            | Destination::Cleanup => {
                self.module = Module::Homebrew;
                self.diagnostics = crate::diagnostics::View::default();
                self.explorer.section = Section::Overview;
                self.focus = Focus::Inventory;
                self.detail_scroll = 0;
                let detail = match destination {
                    Destination::Unresolved => Detail::Unresolved,
                    Destination::Missing => Detail::Missing,
                    Destination::Leaves => Detail::Leaves,
                    Destination::Cleanup => Detail::Cleanup,
                    _ => Detail::Overview,
                };
                if detail != Detail::Overview {
                    self.apply_detail(detail);
                }
            }
            Destination::Containers | Destination::Images | Destination::Volumes => {
                self.module = Module::Docker;
                let d = &mut self.docker;
                d.section = match destination {
                    Destination::Images => crate::docker::Section::Images,
                    Destination::Volumes => crate::docker::Section::Volumes,
                    _ => crate::docker::Section::Containers,
                };
                d.query.clear();
                d.panel.offset = 0;
                d.clamp();
            }
            Destination::Aws => {
                self.module = Module::Cloud;
                self.aws.query.clear();
                self.aws.panel.offset = 0;
                self.aws.clamp();
            }
            d => {
                self.module = Module::Agents;
                self.agents.section = d.agents_section().unwrap_or_default();
                self.codex.sources = d == Destination::Sources;
                self.agents.query.clear();
                self.agents.panel.offset = 0;
                self.agents_clamp();
            }
        }
    }
    /// Only Homebrew Diagnostics has a section list; Codex and Claude use the panel.
    pub fn detail_choices(&self) -> Vec<Detail> {
        use Detail::*;
        if self.module != Module::Homebrew {
            return vec![];
        }
        let mut choices = vec![
            Unresolved,
            Missing,
            Leaves,
            Cleanup,
            FullFinding,
            OriginalOutput,
        ];
        if self.live && self.diagnostics.tab.and_then(|t| t.check()).is_some() {
            choices.push(RunCheck);
        }
        choices
    }
    pub fn detail_rows(&self) -> Vec<Detail> {
        let query = match &self.presentation.page {
            Page::Inspector { query, .. } => query.as_str(),
            _ => "",
        };
        self.detail_choices()
            .into_iter()
            .filter(|d| matches_query(query, d.label()))
            .collect()
    }
    pub fn open_detail(&mut self, detail: Detail) {
        self.remember();
        self.presentation.notice = None;
        if let Some(key) = match detail {
            Detail::RunCheck => Some('c'),
            Detail::OriginalOutput => Some('o'),
            Detail::FullFinding => Some(' '),
            _ => None,
        } {
            if let Page::Inspector { focus, .. } = &mut self.presentation.page {
                *focus = InspectorFocus::Content;
            }
            self.focus_content();
            self.handle_key_inner(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
        } else {
            self.apply_detail(detail);
        }
    }
    fn apply_detail(&mut self, detail: Detail) {
        use Detail::*;
        self.focus_content();
        let selected = self
            .detail_choices()
            .iter()
            .position(|d| *d == detail)
            .unwrap_or(0);
        self.presentation.page = Page::Inspector {
            detail,
            focus: InspectorFocus::Content,
            selected,
            query: String::new(),
        };
        self.diagnostics.tab = match detail {
            Unresolved => Some(Tab::Unresolved),
            Missing => Some(Tab::Missing),
            Leaves => Some(Tab::Leaves),
            Cleanup => Some(Tab::Cleanup),
            _ => None,
        };
        self.diagnostics.group = None;
        self.diagnostics.selected = 0;
        self.diagnostics.scroll = 0;
        self.diagnostics.raw = false;
        self.diagnostics.expanded = false;
        self.detail_scroll = 0;
        self.sync_explorer();
    }
    /// `e`: show the Codex configuration layer that defines the selected row.
    pub(crate) fn follow_source(&mut self) {
        if let Some(layer) = self.agents_source() {
            // The layer is selected in Settings › source files; Esc returns.
            self.agents_jump_source(layer);
            self.presentation.page = Page::Browse;
        } else {
            self.presentation.notice =
                Some("No Codex configuration source is available for this row.".into());
        }
    }
}

// Checkpoints cover native drill-down actions without duplicating their implementation.
#[derive(PartialEq)]
struct Route {
    destination: Destination,
    resource: Option<usize>,
    group: Option<usize>,
    report: bool,
    finding: bool,
    page: Page,
}
pub(crate) struct Checkpoint {
    location: Location,
    revision: u64,
    route: Route,
}
impl App {
    fn route(&self) -> Route {
        Route {
            destination: self.destination(),
            resource: self.selected_node(),
            group: self.diagnostics.group,
            report: self.diagnostics.raw,
            finding: self.diagnostics.expanded,
            page: self.presentation.page.clone(),
        }
    }
    pub(crate) fn navigation_checkpoint(&self, key: KeyEvent) -> Option<Checkpoint> {
        // Only Homebrew Diagnostics opens a report page; Codex and Claude use the panel.
        (self.module == Module::Homebrew
            && !self.active_searching()
            && matches!(
                key.code,
                KeyCode::Enter | KeyCode::Char('1'..='8' | 'o' | 's') | KeyCode::Char(' ')
            ))
        .then(|| Checkpoint {
            location: self.location(),
            revision: self.presentation.revision,
            route: self.route(),
        })
    }
    pub(crate) fn finish_navigation(&mut self, previous: Option<Checkpoint>) {
        if let Some(p) = previous
            && p.revision == self.presentation.revision
            && p.route != self.route()
        {
            self.push_location(p.location);
            let detail = self.native_detail();
            let selected = self
                .detail_choices()
                .iter()
                .position(|d| *d == detail)
                .unwrap_or(0);
            self.presentation.page = Page::Inspector {
                detail,
                focus: InspectorFocus::Content,
                selected,
                query: String::new(),
            };
            self.focus_content();
        }
    }
    fn native_detail(&self) -> Detail {
        match self.destination() {
            Destination::Unresolved => Detail::Unresolved,
            Destination::Missing => Detail::Missing,
            Destination::Leaves => Detail::Leaves,
            Destination::Cleanup => Detail::Cleanup,
            _ => Detail::Overview,
        }
    }
    pub(crate) fn presentation_key(&mut self, key: KeyEvent) -> bool {
        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => self.help = false,
                KeyCode::Char('q') => self.quit = true,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.help_scroll = self.help_scroll.saturating_add(1).min(self.help_max)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_scroll = self.help_scroll.saturating_sub(1)
                }
                KeyCode::PageDown => {
                    self.help_scroll = self.help_scroll.saturating_add(10).min(self.help_max)
                }
                KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
                KeyCode::Home => self.help_scroll = 0,
                KeyCode::End => self.help_scroll = self.help_max,
                _ => (),
            };
            return true;
        }
        if key.code == KeyCode::F(1)
            || (!self.active_searching()
                && !self.presentation.menu_searching
                && key.code == KeyCode::Char('?'))
        {
            self.help = true;
            self.help_scroll = 0;
            return true;
        }
        if matches!(key.code, KeyCode::Tab | KeyCode::F(2)) {
            self.switch_area();
            return true;
        }
        if matches!(key.code, KeyCode::BackTab | KeyCode::F(3)) {
            self.next_section();
            return true;
        }
        if self.active_searching() {
            return false;
        }
        if self.presentation.menu_searching {
            let len = self.detail_rows().len();
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.presentation.menu_searching = false;
            } else if let Page::Inspector {
                query, selected, ..
            } = &mut self.presentation.page
            {
                if matches!(
                    key.code,
                    KeyCode::Up
                        | KeyCode::Down
                        | KeyCode::PageUp
                        | KeyCode::PageDown
                        | KeyCode::Home
                        | KeyCode::End
                ) {
                    *selected = move_selection(*selected, len, key.code);
                } else {
                    edit_query(query, key);
                    *selected = 0;
                }
            }
            return true;
        }
        match key.code {
            KeyCode::Char('/') if !self.inspecting() => {
                self.remember();
                return false;
            }
            KeyCode::Esc | KeyCode::Backspace => {
                self.back();
                return true;
            }
            KeyCode::Left | KeyCode::Right if !self.inspecting() => {
                return true;
            }
            KeyCode::Left if self.inspecting() => {
                self.back();
                return true;
            }
            KeyCode::Char('e') if self.module == Module::Agents => {
                self.remember();
                self.follow_source();
                return true;
            }
            _ => (),
        }
        if self.inspecting() {
            if key.code == KeyCode::Char('/') {
                self.presentation.menu_searching = true;
                if let Page::Inspector { focus, .. } = &mut self.presentation.page {
                    *focus = InspectorFocus::Sections;
                }
                return true;
            }
            if matches!(key.code, KeyCode::Char('[' | ']')) {
                if let Page::Inspector { focus, .. } = &mut self.presentation.page {
                    *focus = match key.code {
                        KeyCode::Char('[') => InspectorFocus::Sections,
                        _ => InspectorFocus::Content,
                    };
                }
                return true;
            }
            if let Page::Inspector {
                focus: InspectorFocus::Sections,
                selected,
                ..
            } = &self.presentation.page
            {
                let selected = *selected;
                let rows = self.detail_rows();
                match key.code {
                    KeyCode::Enter => {
                        if let Some(&detail) = rows.get(selected) {
                            if matches!(self.presentation.page,Page::Inspector{detail:d,..} if d==detail)
                            {
                                if let Page::Inspector { focus, .. } = &mut self.presentation.page {
                                    *focus = InspectorFocus::Content;
                                }
                            } else {
                                self.open_detail(detail);
                            }
                        }
                        return true;
                    }
                    KeyCode::Down
                    | KeyCode::Char('j')
                    | KeyCode::Up
                    | KeyCode::Char('k')
                    | KeyCode::Home
                    | KeyCode::End
                    | KeyCode::PageUp
                    | KeyCode::PageDown => {
                        if let Page::Inspector { selected, .. } = &mut self.presentation.page {
                            *selected = move_selection(*selected, rows.len(), key.code);
                        }
                        return true;
                    }
                    _ => (),
                }
            }
        }
        false
    }
}
fn move_selection(n: usize, len: usize, key: KeyCode) -> usize {
    match key {
        KeyCode::PageDown => n.saturating_add(10).min(len.saturating_sub(1)),
        KeyCode::PageUp => n.saturating_sub(10),
        KeyCode::Home => 0,
        KeyCode::End => len.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => n.saturating_add(1).min(len.saturating_sub(1)),
        _ => n.saturating_sub(1),
    }
}
fn edit_query(query: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Backspace => {
            query.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => query.clear(),
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                && query.len() < 256
                && crate::inventory::safe_text(&c.to_string()) == c.to_string() =>
        {
            query.push(c)
        }
        _ => (),
    }
}
