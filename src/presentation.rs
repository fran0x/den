//! Section → resource → inspector. One bounded history owns all Back navigation.
mod location;
use crate::{
    app::{App, Focus, Module},
    codex::state::View,
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
    Dependencies,
    RequiredBy,
    Paths,
    Unresolved,
    Missing,
    Leaves,
    Cleanup,
    Skills,
    Configuration,
    Sources,
    Instructions,
    Mcp,
}
impl Destination {
    pub const ALL: [Self; 13] = [
        Self::Homebrew,
        Self::Dependencies,
        Self::RequiredBy,
        Self::Paths,
        Self::Unresolved,
        Self::Missing,
        Self::Leaves,
        Self::Cleanup,
        Self::Skills,
        Self::Configuration,
        Self::Sources,
        Self::Instructions,
        Self::Mcp,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Homebrew => "Homebrew / Packages",
            Self::Dependencies => "Homebrew / Dependencies",
            Self::RequiredBy => "Homebrew / Required by",
            Self::Paths => "Homebrew / Dependency paths",
            Self::Unresolved => "Homebrew / Diagnostics / Unresolved",
            Self::Missing => "Homebrew / Diagnostics / Missing",
            Self::Leaves => "Homebrew / Diagnostics / Leaves",
            Self::Cleanup => "Homebrew / Diagnostics / Cleanup preview",
            Self::Skills => "Codex / Skills",
            Self::Configuration => "Codex / Configuration",
            Self::Sources => "Codex / Configuration / Sources",
            Self::Instructions => "Codex / Instructions",
            Self::Mcp => "Codex / MCP",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detail {
    Overview,
    Dependencies,
    RequiredBy,
    Paths,
    Unresolved,
    Missing,
    Leaves,
    Cleanup,
    Instructions,
    Files,
    Findings,
    Coverage,
    Transport,
    Authentication,
    Tools,
    Sources,
    Preview,
    Evidence,
    ToggleEvidence,
    NextKeg,
    ToggleTree,
    RunCheck,
    OriginalOutput,
    FullFinding,
    NextSource,
    TrustedScenario,
}
impl Detail {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Dependencies => "Dependencies",
            Self::RequiredBy => "Required by",
            Self::Paths => "Dependency paths",
            Self::Unresolved => "Diagnostics / Unresolved",
            Self::Missing => "Diagnostics / Missing",
            Self::Leaves => "Diagnostics / Leaves",
            Self::Cleanup => "Diagnostics / Cleanup",
            Self::Instructions => "Instructions",
            Self::Files => "Supporting files",
            Self::Findings => "Findings",
            Self::Coverage => "Coverage",
            Self::Transport => "Transport",
            Self::Authentication => "Authentication",
            Self::Tools => "Tools and approvals",
            Self::Sources => "Configuration sources",
            Self::Preview => "Instruction preview",
            Self::Evidence => "Evidence",
            Self::ToggleEvidence => "Recorded / current evidence",
            Self::NextKeg => "Next installed keg",
            Self::ToggleTree => "Direct / tree view",
            Self::RunCheck => "Run selected Homebrew check",
            Self::OriginalOutput => "Original Homebrew report",
            Self::FullFinding => "Full finding / summary",
            Self::NextSource => "Next configuration source",
            Self::TrustedScenario => "Conditional trusted scenario",
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
pub struct Switcher {
    pub query: String,
    pub selected: usize,
    pub searching: bool,
}
impl Switcher {
    pub fn rows(&self) -> Vec<Destination> {
        Destination::ALL
            .into_iter()
            .filter(|d| matches_query(&self.query, d.label()))
            .collect()
    }
}
#[derive(Default)]
pub struct Presentation {
    pub page: Page,
    pub switcher: Option<Switcher>,
    history: Vec<Location>,
    saved: BTreeMap<Destination, Location>,
    pub(super) revision: u64,
    pub menu_searching: bool,
    pub notice: Option<String>,
}
impl App {
    pub fn destination(&self) -> Destination {
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => match self.diagnostics.tab {
                Some(Tab::Unresolved) => Destination::Unresolved,
                Some(Tab::Missing) => Destination::Missing,
                Some(Tab::Leaves) => Destination::Leaves,
                Some(Tab::Cleanup) => Destination::Cleanup,
                None => match self.explorer.section {
                    Section::Overview => Destination::Homebrew,
                    Section::Dependencies => Destination::Dependencies,
                    Section::RequiredBy => Destination::RequiredBy,
                    Section::Paths => Destination::Paths,
                },
            },
            (_, View::Skills) => Destination::Skills,
            (_, View::Configuration) if self.codex.sources => Destination::Sources,
            (_, View::Configuration) => Destination::Configuration,
            (_, View::Instructions) => Destination::Instructions,
            (_, View::Mcp) => Destination::Mcp,
        }
    }
    pub fn inspecting(&self) -> bool {
        matches!(self.presentation.page, Page::Inspector { .. })
    }
    pub fn evidence(&self) -> bool {
        matches!(
            self.presentation.page,
            Page::Inspector {
                detail: Detail::Evidence,
                ..
            }
        )
    }
    pub fn active_focus(&self) -> Focus {
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => self.focus,
            (_, View::Skills) => self.skills.focus,
            _ => self.codex.focus(),
        }
    }
    pub fn active_searching(&self) -> bool {
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => self.searching,
            (_, View::Skills) => self.skills.searching,
            _ => self.codex.searching,
        }
    }
    fn focus_content(&mut self) {
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => self.focus = Focus::Details,
            (_, View::Skills) => self.skills.focus = Focus::Details,
            _ => self.codex.focus_details = true,
        }
    }
    pub(crate) fn remember(&mut self) {
        self.push_location(self.location());
    }
    fn push_location(&mut self, location: Location) {
        if self.presentation.history.len() == 64 {
            self.presentation.history.remove(0);
        }
        self.presentation.history.push(location);
        self.presentation.revision = self.presentation.revision.wrapping_add(1);
    }
    pub fn back(&mut self) {
        self.presentation.notice = None;
        if let Some(location) = self.presentation.history.pop() {
            self.presentation
                .saved
                .insert(self.destination(), self.location());
            self.restore(location);
            self.presentation.revision = self.presentation.revision.wrapping_add(1);
        } else {
            self.presentation.page = Page::Browse;
            match self.module {
                Module::Homebrew => self.focus = Focus::Inventory,
                Module::Skills if self.codex.view == View::Skills => {
                    self.skills.focus = Focus::Inventory
                }
                _ => self.codex.focus_details = false,
            }
        }
    }
    pub fn navigate(&mut self, destination: Destination) {
        self.presentation.switcher = None;
        self.presentation.notice = None;
        if destination == self.destination() {
            return;
        }
        self.presentation
            .saved
            .insert(self.destination(), self.location());
        self.remember();
        if let Some(location) = self.presentation.saved.get(&destination).cloned() {
            self.restore(location);
            return;
        }
        self.presentation.page = Page::Browse;
        match destination {
            Destination::Homebrew
            | Destination::Dependencies
            | Destination::RequiredBy
            | Destination::Paths
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
                    Destination::Dependencies => Detail::Dependencies,
                    Destination::RequiredBy => Detail::RequiredBy,
                    Destination::Paths => Detail::Paths,
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
            Destination::Skills => {
                self.module = Module::Skills;
                self.codex.view = View::Skills;
                self.skills.focus = Focus::Inventory;
            }
            _ => {
                self.module = Module::Skills;
                self.codex.view = match destination {
                    Destination::Instructions => View::Instructions,
                    Destination::Mcp => View::Mcp,
                    _ => View::Configuration,
                };
                self.codex.sources = destination == Destination::Sources;
                self.codex.query.clear();
                self.codex.scroll = 0;
                self.codex.focus_details = false;
                self.codex.preview = false;
                self.codex.clamp();
            }
        }
    }
    pub fn detail_choices(&self) -> Vec<Detail> {
        use Detail::*;
        let mut choices = match (self.module, self.codex.view) {
            (Module::Homebrew, _) => vec![
                Overview,
                Dependencies,
                RequiredBy,
                Paths,
                Unresolved,
                Missing,
                Leaves,
                Cleanup,
                Evidence,
            ],
            (_, View::Skills) => vec![
                Overview,
                Instructions,
                Files,
                Findings,
                Coverage,
                Sources,
                Evidence,
            ],
            (_, View::Mcp) => vec![
                Overview,
                Transport,
                Authentication,
                Tools,
                Sources,
                Findings,
                Evidence,
            ],
            (_, View::Instructions) => vec![Overview, Preview, Evidence],
            _ => vec![Overview, Sources, Evidence],
        };
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) if self.diagnostics.tab.is_some() => {
                choices.extend([FullFinding, OriginalOutput]);
                if self.live && self.diagnostics.tab.and_then(|t| t.check()).is_some() {
                    choices.push(RunCheck);
                }
            }
            (Module::Homebrew, _) => {
                choices.extend([ToggleEvidence, NextKeg]);
                if self.explorer.section != Section::Overview {
                    choices.push(ToggleTree);
                }
            }
            (_, View::Mcp) => choices.push(NextSource),
            (_, View::Instructions) => choices.push(TrustedScenario),
            _ => (),
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
    pub fn open_inspector(&mut self) {
        self.remember();
        self.focus_content();
        self.presentation.page = Page::Inspector {
            detail: Detail::Overview,
            focus: InspectorFocus::Content,
            selected: 0,
            query: String::new(),
        };
        self.apply_overview();
    }
    fn apply_overview(&mut self) {
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => {
                self.diagnostics.tab = None;
                self.explorer.section = Section::Overview;
                self.detail_scroll = 0;
            }
            (_, View::Skills) => {
                self.skills.section = crate::skills::state::Section::Overview;
                self.skills.scroll = 0;
            }
            (_, View::Mcp) => {
                self.codex.mcp.section = 0;
                self.codex.scroll = 0;
            }
            _ => {
                self.codex.preview = false;
                self.codex.scroll = 0;
            }
        }
    }
    pub fn open_detail(&mut self, detail: Detail) {
        self.remember();
        self.presentation.notice = None;
        if let Some(key) = match detail {
            Detail::ToggleEvidence => Some('s'),
            Detail::NextKeg => Some('V'),
            Detail::ToggleTree => Some('t'),
            Detail::RunCheck => Some('c'),
            Detail::OriginalOutput => Some('o'),
            Detail::FullFinding => Some(' '),
            Detail::NextSource => Some('V'),
            Detail::TrustedScenario => Some('t'),
            _ => None,
        } {
            self.handle_key_inner(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE));
            if let Page::Inspector { focus, .. } = &mut self.presentation.page {
                *focus = InspectorFocus::Content;
            }
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
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => {
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
                self.explorer.section = match detail {
                    Dependencies => Section::Dependencies,
                    RequiredBy => Section::RequiredBy,
                    Paths => Section::Paths,
                    _ => Section::Overview,
                };
                self.detail_scroll = 0;
                self.sync_explorer();
            }
            (_, View::Skills) => {
                self.skills.section = match detail {
                    Instructions => crate::skills::state::Section::Instructions,
                    Files => crate::skills::state::Section::Files,
                    Findings => crate::skills::state::Section::Findings,
                    Coverage => crate::skills::state::Section::Coverage,
                    _ => crate::skills::state::Section::Overview,
                };
                self.skills.scroll = 0;
                self.skills.file_open = false;
                if detail == Sources {
                    self.follow_source();
                }
            }
            (_, View::Mcp) => {
                self.codex.mcp.section = match detail {
                    Transport => 1,
                    Authentication => 2,
                    Tools => 3,
                    Sources => 4,
                    Findings => 5,
                    _ => 0,
                };
                self.codex.scroll = 0;
                if detail == Sources {
                    self.follow_source();
                }
            }
            (_, View::Instructions) => {
                self.codex.preview = detail == Preview;
                self.codex.scroll = 0;
            }
            _ => {
                self.codex.scroll = 0;
                if detail == Sources {
                    self.follow_source();
                }
            }
        }
    }
    pub(crate) fn follow_source(&mut self) {
        let source = if self.codex.view == View::Skills {
            self.skills
                .selected
                .and_then(|n| self.skills.visible.get(n))
                .and_then(|n| self.skills.enablement.get(*n))
                .and_then(|e| e.source)
        } else {
            self.codex.selected_source()
        };
        if let Some(source) = source {
            self.codex.jump_source(source);
            self.presentation.page = Page::Inspector {
                detail: Detail::Evidence,
                focus: InspectorFocus::Content,
                selected: 0,
                query: String::new(),
            };
        } else {
            self.presentation.notice =
                Some("No configuration source is available for this resource.".into());
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
    skill_section: crate::skills::state::Section,
    file_open: bool,
    mcp_section: usize,
    preview: bool,
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
            skill_section: self.skills.section,
            file_open: self.skills.file_open,
            mcp_section: self.codex.mcp.section,
            preview: self.codex.preview,
            page: self.presentation.page.clone(),
        }
    }
    pub(crate) fn navigation_checkpoint(&self, key: KeyEvent) -> Option<Checkpoint> {
        (!self.active_searching()
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
        use Detail::*;
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => match self.destination() {
                Destination::Dependencies => Dependencies,
                Destination::RequiredBy => RequiredBy,
                Destination::Paths => Paths,
                Destination::Unresolved => Unresolved,
                Destination::Missing => Missing,
                Destination::Leaves => Leaves,
                Destination::Cleanup => Cleanup,
                _ => Overview,
            },
            (_, View::Skills) => match self.skills.section {
                crate::skills::state::Section::Overview => Overview,
                crate::skills::state::Section::Instructions => Instructions,
                crate::skills::state::Section::Files => Files,
                crate::skills::state::Section::Findings => Findings,
                crate::skills::state::Section::Coverage => Coverage,
            },
            (_, View::Mcp) => [
                Overview,
                Transport,
                Authentication,
                Tools,
                Sources,
                Findings,
            ][self.codex.mcp.section.min(5)],
            (_, View::Instructions) if self.codex.preview => Preview,
            (_, View::Configuration) if self.codex.sources => Evidence,
            _ => Overview,
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
        if self.presentation.switcher.is_some() {
            return self.switcher_key(key);
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
            KeyCode::Char('g') => {
                self.presentation.switcher = Some(Switcher::default());
                return true;
            }
            KeyCode::Esc | KeyCode::Backspace => {
                self.back();
                return true;
            }
            KeyCode::F(2) => {
                self.navigate(if self.module == Module::Homebrew {
                    Destination::Skills
                } else {
                    Destination::Homebrew
                });
                return true;
            }
            KeyCode::F(3) => {
                let next = match self.destination() {
                    Destination::Homebrew => Destination::Dependencies,
                    Destination::Dependencies => Destination::RequiredBy,
                    Destination::RequiredBy => Destination::Paths,
                    Destination::Paths => Destination::Unresolved,
                    Destination::Unresolved => Destination::Missing,
                    Destination::Missing => Destination::Leaves,
                    Destination::Leaves => Destination::Cleanup,
                    Destination::Cleanup => Destination::Homebrew,
                    Destination::Skills => Destination::Configuration,
                    Destination::Configuration | Destination::Sources => Destination::Instructions,
                    Destination::Instructions => Destination::Mcp,
                    Destination::Mcp => Destination::Skills,
                };
                self.navigate(next);
                return true;
            }
            KeyCode::Left if self.inspecting() => {
                self.back();
                return true;
            }
            KeyCode::Char('v') => {
                self.open_detail(Detail::Evidence);
                return true;
            }
            KeyCode::Char('z') => {
                if !self.inspecting() {
                    self.open_inspector();
                }
                return true;
            }
            KeyCode::Enter if !self.inspecting() => {
                self.open_inspector();
                return true;
            }
            KeyCode::Char('e') if self.module == Module::Skills => {
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
            if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                if let Page::Inspector { focus, .. } = &mut self.presentation.page {
                    *focus = match focus {
                        InspectorFocus::Sections => InspectorFocus::Content,
                        InspectorFocus::Content => InspectorFocus::Sections,
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
    fn switcher_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Esc {
            self.presentation.switcher = None;
            return true;
        }
        if key.code == KeyCode::Enter {
            let destination = self
                .presentation
                .switcher
                .as_ref()
                .and_then(|s| s.rows().get(s.selected).copied());
            if let Some(d) = destination {
                self.navigate(d);
            }
            return true;
        }
        if key.code == KeyCode::Char('?')
            && self
                .presentation
                .switcher
                .as_ref()
                .is_some_and(|s| !s.searching)
        {
            self.help = true;
            self.help_scroll = 0;
            return true;
        }
        let Some(s) = &mut self.presentation.switcher else {
            return true;
        };
        if s.searching {
            if matches!(
                key.code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Home
                    | KeyCode::End
            ) {
                s.selected = move_selection(s.selected, s.rows().len(), key.code);
            } else {
                edit_query(&mut s.query, key);
                s.selected = 0;
            }
        } else {
            match key.code {
                KeyCode::Char('/') => s.searching = true,
                KeyCode::Down
                | KeyCode::Char('j')
                | KeyCode::Up
                | KeyCode::Char('k')
                | KeyCode::Home
                | KeyCode::End => s.selected = move_selection(s.selected, s.rows().len(), key.code),
                _ => (),
            }
        }
        true
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
