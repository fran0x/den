use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    diagnostics::{Check, CheckState, Health, Item, Report, Tab, View},
    explorer::{Explorer, Section},
    graph::{Evidence, Graph, Installation},
    homebrew::CollectionError,
    inventory::{InstallationIntent, Inventory, Resource, ResourceKind, safe_text},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionStatus {
    Snapshot,
    Loading { refreshing: bool },
    Loaded,
    Failed { error: CollectionError, stale: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Inventory,
    Details,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Module {
    Homebrew,
    Skills,
}

pub struct App {
    pub module: Module,
    pub skills: crate::skills::state::State,
    pub codex: crate::codex::state::State,
    pub inventory: Inventory,
    pub graph: Graph,
    pub explorer: Explorer,
    pub health: Health,
    pub diagnostics: View,
    pub checks: [CheckState; 3],
    pub check_requested: Option<Check>,
    pub inventory_epoch: u64,
    history: Vec<Location>,
    pub source_label: &'static str,
    pub query: String,
    pub filter: Option<ResourceKind>,
    pub intent_filter: Option<InstallationIntent>,
    pub live: bool,
    pub refresh_requested: bool,
    pub collection_status: CollectionStatus,
    has_valid_inventory: bool,
    pub focus: Focus,
    pub searching: bool,
    pub help: bool,
    pub quit: bool,
    pub selected: Option<usize>,
    pub visible: Vec<usize>,
    pub list_offset: usize,
    pub detail_scroll: u16,
    pub detail_max: u16,
    pub help_scroll: u16,
    pub help_max: u16,
    pub page_size: usize,
}

struct Location {
    id: String,
    query: String,
    filter: Option<ResourceKind>,
    intent: Option<InstallationIntent>,
    explorer: Explorer,
    diagnostics: View,
}

impl App {
    pub fn new(inventory: Inventory, source_label: &'static str) -> Self {
        let graph = Graph::build(&inventory);
        let health = Health::build(&graph);
        let mut app = Self {
            module: Module::Homebrew,
            skills: crate::skills::state::State::default(),
            codex: crate::codex::state::State::default(),
            graph,
            health,
            diagnostics: View::default(),
            checks: std::array::from_fn(|_| CheckState::default()),
            check_requested: None,
            inventory_epoch: 0,
            explorer: Explorer::default(),
            history: vec![],
            inventory,
            source_label,
            query: String::new(),
            filter: None,
            intent_filter: None,
            live: false,
            refresh_requested: false,
            collection_status: CollectionStatus::Snapshot,
            has_valid_inventory: true,
            focus: Focus::Inventory,
            searching: false,
            help: false,
            quit: false,
            selected: None,
            visible: Vec::new(),
            list_offset: 0,
            detail_scroll: 0,
            detail_max: 0,
            help_scroll: 0,
            help_max: 0,
            page_size: 1,
        };
        app.refresh();
        app
    }

    pub fn resource(&self) -> Option<&Resource> {
        self.selected
            .and_then(|i| self.visible.get(i))
            .map(|&i| &self.inventory.resources[i])
    }

    pub fn selected_node(&self) -> Option<usize> {
        self.selected.and_then(|i| self.visible.get(i)).copied()
    }

    fn sync_explorer(&mut self) {
        let node = self.selected_node();
        if self.explorer.installation.as_ref().map(|s| s.node) != node {
            self.explorer.select(&self.graph, node);
        }
        self.explorer.rebuild(&self.graph, &self.inventory);
    }

    pub fn jump_to(&mut self, target: Installation) {
        if target.node >= self.inventory.resources.len() {
            return;
        }
        {
            let id = self.resource().map(|r| r.id.clone()).unwrap_or_default();
            let mut explorer = self.explorer.clone();
            explorer.rows.clear();
            explorer.note.clear();
            if self.history.len() >= 32 {
                self.history.remove(0);
            }
            self.history.push(Location {
                id,
                query: self.query.clone(),
                filter: self.filter,
                intent: self.intent_filter,
                explorer,
                diagnostics: self.diagnostics.clone(),
            });
        }
        self.diagnostics = View::default();
        // Related packages may be hidden by the current query. Back restores that context.
        self.query.clear();
        self.filter = None;
        self.intent_filter = None;
        self.refresh();
        self.selected = self.visible.iter().position(|&node| node == target.node);
        self.explorer.select(&self.graph, Some(target.node));
        if target.keg.is_some() {
            self.explorer.installation = Some(target);
        }
        self.explorer.section = Section::Overview;
        self.detail_scroll = 0;
        self.focus = Focus::Details;
        self.sync_explorer();
    }

    fn go_back(&mut self) {
        if let Some(location) = self.history.pop() {
            self.query = location.query;
            self.filter = location.filter;
            self.intent_filter = location.intent;
            self.refresh();
            self.selected = self
                .visible
                .iter()
                .position(|&i| self.inventory.resources[i].id == location.id)
                .or(self.selected);
            self.explorer = location.explorer;
            self.diagnostics = location.diagnostics;
            self.focus = Focus::Details;
            self.sync_explorer();
        }
    }

    pub fn live_homebrew() -> Self {
        let mut app = Self::new(
            Inventory {
                schema_version: 1,
                resources: vec![],
            },
            "Homebrew",
        );
        app.live = true;
        app.has_valid_inventory = false;
        app.refresh_requested = true;
        app
    }

    pub fn is_homebrew(&self) -> bool {
        self.live
            || self.source_label == "Homebrew snapshot"
            || self
                .inventory
                .resources
                .iter()
                .any(|r| r.homebrew.is_some())
    }

    pub fn begin_collection(&mut self) {
        self.refresh_requested = false;
        self.collection_status = CollectionStatus::Loading {
            refreshing: self.has_valid_inventory,
        };
    }

    pub fn finish_collection(&mut self, result: Result<Inventory, CollectionError>) {
        match result {
            Ok(inventory) => {
                let old_id = self.resource().map(|r| r.id.clone());
                let old_scroll = self.detail_scroll;
                let old_offset = self.list_offset;
                let graph = Graph::build(&inventory);
                let health = Health::build(&graph);
                self.inventory = inventory;
                self.health = health;
                self.inventory_epoch = self.inventory_epoch.saturating_add(1);
                self.diagnostics.group = None;
                self.diagnostics.selected = 0;
                self.diagnostics.scroll = 0;
                self.graph = graph;
                self.history.clear();
                self.explorer.installation = None;
                self.selected = None;
                self.refresh();
                if let Some(index) = self
                    .visible
                    .iter()
                    .position(|&i| Some(&self.inventory.resources[i].id) == old_id.as_ref())
                {
                    self.selected = Some(index);
                    self.detail_scroll = old_scroll;
                    self.list_offset = old_offset;
                }
                self.has_valid_inventory = true;
                self.collection_status = CollectionStatus::Loaded;
                self.sync_explorer();
            }
            Err(error) => {
                self.collection_status = CollectionStatus::Failed {
                    error,
                    stale: self.has_valid_inventory,
                }
            }
        }
    }

    pub fn collection_message(&self) -> String {
        match &self.collection_status {
            CollectionStatus::Snapshot => "Snapshot loaded; no collection commands run".into(),
            CollectionStatus::Loading { refreshing: false } => {
                "Loading Homebrew inventory... q:quit".into()
            }
            CollectionStatus::Loading { refreshing: true } => {
                "Refreshing Homebrew inventory... showing previous data".into()
            }
            CollectionStatus::Loaded if self.inventory.resources.is_empty() => {
                "No installed Homebrew packages | r:refresh".into()
            }
            CollectionStatus::Loaded => format!(
                "Loaded {} packages | r:refresh",
                self.inventory.resources.len()
            ),
            CollectionStatus::Failed { error, stale } => {
                format!("{}{} | r:retry", if *stale { "STALE: " } else { "" }, error)
            }
        }
    }

    pub fn refresh(&mut self) {
        let previous = self.selected.and_then(|i| self.visible.get(i)).copied();

        self.visible = self
            .inventory
            .resources
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                if self.filter.is_some_and(|kind| r.kind != kind) {
                    return None;
                }
                if self.intent_filter.is_some_and(|intent| {
                    r.homebrew
                        .as_ref()
                        .map_or(InstallationIntent::Unknown, |m| m.intent())
                        != intent
                }) {
                    return None;
                }
                let display_names = match &r.homebrew {
                    Some(crate::inventory::HomebrewMetadata::Cask { display_names, .. }) => {
                        display_names.join(" ")
                    }
                    _ => String::new(),
                };
                let haystack = safe_text(&format!(
                    "{} {} {} {} {} {} {} {} {}",
                    r.id,
                    r.name,
                    r.kind.label(),
                    r.version.as_deref().unwrap_or(""),
                    r.path.as_deref().unwrap_or(""),
                    r.description,
                    r.origin,
                    r.reason,
                    display_names
                ))
                .to_lowercase();
                crate::inventory::matches_query(&self.query, &haystack).then_some(i)
            })
            .collect();
        self.selected = previous
            .and_then(|old| self.visible.iter().position(|&i| i == old))
            .or_else(|| (!self.visible.is_empty()).then_some(0));
        self.list_offset = 0;
        self.detail_scroll = 0;
        self.detail_max = 0;
        self.sync_explorer();
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if key.code == KeyCode::F(2) {
            self.module = if self.module == Module::Homebrew {
                Module::Skills
            } else {
                Module::Homebrew
            };
            return;
        }
        if self.module == Module::Skills {
            use crate::codex::state::View;
            if key.code == KeyCode::F(3) {
                self.codex.cycle_view();
                return;
            }
            if self.codex.view != View::Skills {
                if self.codex.handle(key) {
                    self.quit = true;
                }
                return;
            }
            if !self.skills.searching && key.code == KeyCode::Char('e') {
                if let Some(index) = self
                    .skills
                    .selected
                    .and_then(|n| self.skills.visible.get(n))
                    && let Some(source) = self.skills.enablement.get(*index).and_then(|e| e.source)
                {
                    self.codex.jump_source(source);
                    self.codex.return_to_skill = true;
                }
                return;
            }
            if !self.skills.searching
                && key.code == KeyCode::Char('r')
                && self.codex.context.is_some()
            {
                if !self.codex.running {
                    self.codex.requested = true;
                }
                return;
            }
            if self.skills.handle(key) {
                self.quit = true;
            }
            return;
        }
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
                    self.help_scroll = self.help_scroll.saturating_add(5).min(self.help_max)
                }
                KeyCode::PageUp => self.help_scroll = self.help_scroll.saturating_sub(5),
                KeyCode::Home => self.help_scroll = 0,
                KeyCode::End => self.help_scroll = self.help_max,
                _ => {}
            }
            return;
        }
        if key.code == KeyCode::F(1) || (!self.searching && key.code == KeyCode::Char('?')) {
            self.help = true;
            self.help_scroll = 0;
            return;
        }
        if self.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.refresh();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear();
                    self.refresh();
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && self.query.len() < 256
                        && safe_text(&ch.to_string()) == ch.to_string() =>
                {
                    self.query.push(ch);
                    self.refresh();
                }
                _ => {}
            }
            return;
        }
        if self.handle_diagnostic_key(key.code) {
            return;
        }
        match key.code {
            KeyCode::Char(ch @ '1'..='4') if self.is_homebrew() => {
                self.diagnostics.tab = None;
                self.explorer.section = match ch {
                    '1' => Section::Overview,
                    '2' => Section::Dependencies,
                    '3' => Section::RequiredBy,
                    _ => Section::Paths,
                };
                self.explorer.clear_navigation();
                self.focus = Focus::Details;
                self.detail_scroll = 0;
                self.sync_explorer();
            }
            KeyCode::Char('s') if self.focus == Focus::Details && self.is_homebrew() => {
                self.explorer.evidence = match self.explorer.evidence {
                    Evidence::Recorded => Evidence::Current,
                    Evidence::Current => Evidence::Recorded,
                };
                self.explorer.clear_navigation();
                self.sync_explorer();
            }
            KeyCode::Char('v') if self.focus == Focus::Details && self.is_homebrew() => {
                self.explorer.cycle_keg(&self.graph);
                self.sync_explorer();
            }
            KeyCode::Char('t')
                if self.focus == Focus::Details && self.explorer.section != Section::Overview =>
            {
                self.explorer.tree = !self.explorer.tree;
                self.explorer.clear_navigation();
                self.sync_explorer();
            }
            KeyCode::Char(' ')
                if self.focus == Focus::Details && self.explorer.section != Section::Overview =>
            {
                self.explorer.toggle_expansion();
                self.sync_explorer();
            }
            KeyCode::Enter
                if self.focus == Focus::Details && self.explorer.section != Section::Overview =>
            {
                if let Some(target) = self
                    .explorer
                    .rows
                    .get(self.explorer.selected)
                    .and_then(|r| r.target.clone())
                {
                    self.jump_to(target);
                }
            }
            KeyCode::Backspace => self.go_back(),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::Inventory;
            }
            KeyCode::Esc => {
                self.query.clear();
                self.filter = None;
                self.intent_filter = None;
                self.refresh();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::Inventory => Focus::Details,
                    Focus::Details => Focus::Inventory,
                }
            }
            KeyCode::Enter | KeyCode::Right => self.focus = Focus::Details,
            KeyCode::Left => self.focus = Focus::Inventory,
            KeyCode::Char('f') => {
                let kinds: &[ResourceKind] = if self.is_homebrew() {
                    &[ResourceKind::Formula, ResourceKind::Cask]
                } else {
                    &ResourceKind::ALL
                };
                self.filter = match self.filter {
                    None => Some(kinds[0]),
                    Some(kind) => kinds
                        .iter()
                        .position(|&k| k == kind)
                        .and_then(|i| kinds.get(i + 1).copied()),
                };
                self.refresh();
            }
            KeyCode::Char('i') if self.is_homebrew() => {
                self.intent_filter = match self.intent_filter {
                    None => Some(InstallationIntent::Requested),
                    Some(InstallationIntent::Requested) => Some(InstallationIntent::Dependency),
                    Some(InstallationIntent::Dependency) => Some(InstallationIntent::Unknown),
                    Some(InstallationIntent::Unknown) => None,
                };
                self.refresh();
            }
            KeyCode::Char('r')
                if self.live
                    && !matches!(self.collection_status, CollectionStatus::Loading { .. }) =>
            {
                self.refresh_requested = true;
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(self.page_size as isize),
            KeyCode::PageUp => self.move_by(-(self.page_size as isize)),
            KeyCode::Home => self.move_by(-isize::MAX),
            KeyCode::End => self.move_by(isize::MAX),
            _ => {}
        }
    }

    pub fn begin_check(&mut self) -> Option<Check> {
        if !self.live || self.checks.iter().any(|c| c.running) {
            return None;
        }
        let check = self.check_requested.take()?;
        self.checks[check.index()].running = true;
        Some(check)
    }

    pub fn finish_check(
        &mut self,
        check: Check,
        result: Result<crate::homebrew::CommandOutput, CollectionError>,
        epoch: u64,
    ) {
        self.checks[check.index()] = CheckState {
            running: false,
            report: Some(Report::interpret(
                check,
                result,
                &self.inventory,
                &self.graph,
                epoch,
            )),
        };
    }

    fn handle_diagnostic_key(&mut self, key: KeyCode) -> bool {
        if let KeyCode::Char(ch @ '5'..='8') = key
            && self.is_homebrew()
        {
            self.diagnostics = View {
                tab: Some(match ch {
                    '5' => Tab::Unresolved,
                    '6' => Tab::Missing,
                    '7' => Tab::Leaves,
                    _ => Tab::Cleanup,
                }),
                ..View::default()
            };
            self.focus = Focus::Details;
            return true;
        }
        let Some(tab) = self
            .diagnostics
            .tab
            .filter(|_| self.focus == Focus::Details)
        else {
            return false;
        };
        match key {
            KeyCode::Char('c') => {
                if self.live
                    && !self.checks.iter().any(|c| c.running)
                    && !matches!(self.collection_status, CollectionStatus::Loading { .. })
                {
                    self.check_requested = tab.check();
                }
            }
            KeyCode::Char('o') => {
                self.diagnostics.raw = !self.diagnostics.raw;
                self.diagnostics.expanded = false;
                self.diagnostics.scroll = 0;
            }
            KeyCode::Char(' ') => {
                self.diagnostics.expanded = !self.diagnostics.expanded;
                self.diagnostics.raw = false;
                self.diagnostics.scroll = 0;
            }
            KeyCode::Enter if !self.diagnostics.raw && !self.diagnostics.expanded => {
                if let Some(item) = self
                    .diagnostics
                    .items(&self.health, &self.checks)
                    .get(self.diagnostics.selected)
                    .copied()
                {
                    match item {
                        Item::Group(group) => {
                            self.diagnostics.group = Some(group);
                            self.diagnostics.selected = 0;
                        }
                        Item::Edge(edge) => {
                            self.jump_to(self.graph.preferred(self.graph.edges[edge].from));
                        }
                        Item::Package(node) => {
                            self.jump_to(self.graph.preferred(node));
                        }
                        Item::Entry(entry) => {
                            let id = tab
                                .check()
                                .and_then(|c| self.checks[c.index()].report.as_ref())
                                .and_then(|r| r.entries.get(entry))
                                .and_then(|e| e.package_id.as_deref());
                            if let Some(node) = id.and_then(|id| {
                                self.inventory.resources.iter().position(|r| r.id == id)
                            }) {
                                self.jump_to(self.graph.preferred(node));
                            }
                        }
                    }
                }
            }
            KeyCode::Enter => {}
            KeyCode::Backspace => {
                if self.diagnostics.raw || self.diagnostics.expanded {
                    self.diagnostics.raw = false;
                    self.diagnostics.expanded = false;
                } else if let Some(group) = self.diagnostics.group.take() {
                    self.diagnostics.selected = group;
                } else {
                    self.diagnostics.tab = None;
                }
                self.diagnostics.scroll = 0;
            }
            // Existing relationship controls must not change hidden explorer state.
            KeyCode::Char('s' | 'v' | 't') => {}
            _ => return false,
        }
        true
    }

    fn move_by(&mut self, delta: isize) {
        if self.focus == Focus::Details && self.diagnostics.tab.is_some() {
            if self.diagnostics.raw
                || self.diagnostics.expanded
                || self
                    .diagnostics
                    .items(&self.health, &self.checks)
                    .is_empty()
            {
                self.diagnostics.scroll = (self.diagnostics.scroll as usize)
                    .saturating_add_signed(delta)
                    .min(self.detail_max as usize) as u16;
            } else {
                let count = self.diagnostics.items(&self.health, &self.checks).len();
                self.diagnostics.selected = self
                    .diagnostics
                    .selected
                    .saturating_add_signed(delta)
                    .min(count.saturating_sub(1));
            }
            return;
        }
        if self.focus == Focus::Details {
            if self.explorer.section != Section::Overview && !self.explorer.rows.is_empty() {
                self.explorer.move_by(delta);
                return;
            }
            self.detail_scroll = (self.detail_scroll as usize)
                .saturating_add_signed(delta)
                .min(self.detail_max as usize) as u16;
        } else if let Some(selected) = self.selected {
            self.selected = Some(
                selected
                    .saturating_add_signed(delta)
                    .min(self.visible.len() - 1),
            );
            if self.selected != Some(selected) {
                self.detail_scroll = 0;
                self.detail_max = 0;
                self.sync_explorer();
            }
        }
    }
}
