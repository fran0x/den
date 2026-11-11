use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    diagnostics::{Check, CheckState, Health, Item, Report, Tab, View},
    explorer::{Explorer, Section},
    graph::{Graph, Installation},
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
    /// Codex and Claude Code side by side.
    Agents,
    /// Cloud tools: AWS profiles and Kubernetes contexts.
    Cloud,
    /// Docker containers, images and volumes.
    Docker,
    /// Language toolchains: what wins on PATH, Rust, global packages.
    Toolchains,
}
/// The Cloud area's sections; Shift+Tab switches them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudSection {
    #[default]
    Aws,
    Kubernetes,
}

impl Module {
    /// Position in the top bar and in Tab order.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Package counts for the current search, independent of Tools/All.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HomebrewCounts {
    pub all: usize,
    /// Packages Homebrew recorded as installed on request.
    pub tools: usize,
}

pub struct App {
    pub presentation: crate::presentation::Presentation,
    pub module: Module,
    pub skills: crate::skills::state::State,
    pub codex: crate::codex::state::State,
    pub claude: crate::claude::state::State,
    pub agents: crate::agents::State,
    pub aws: crate::aws::state::State,
    pub kube: crate::kube::state::State,
    pub cloud: CloudSection,
    pub toolchains: crate::toolchains::state::State,
    pub docker: crate::docker::state::State,
    pub inventory: Inventory,
    pub graph: Graph,
    pub explorer: Explorer,
    pub health: Health,
    pub diagnostics: View,
    pub checks: [CheckState; 3],
    pub check_requested: Option<Check>,
    pub inventory_epoch: u64,
    pub source_label: &'static str,
    pub query: String,
    pub filter: Option<ResourceKind>,
    /// Homebrew starts with Tools (recorded as requested); `a` shows all packages.
    pub show_all: bool,
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

impl App {
    pub fn new(inventory: Inventory, source_label: &'static str) -> Self {
        let graph = Graph::build(&inventory);
        let health = Health::build(&graph);
        let mut app = Self {
            presentation: crate::presentation::Presentation::default(),
            module: Module::Homebrew,
            skills: crate::skills::state::State::default(),
            codex: crate::codex::state::State::default(),
            claude: crate::claude::state::State::default(),
            agents: crate::agents::State::default(),
            aws: crate::aws::state::State::default(),
            kube: crate::kube::state::State::default(),
            cloud: CloudSection::default(),
            toolchains: crate::toolchains::state::State::default(),
            docker: crate::docker::state::State::default(),
            graph,
            health,
            diagnostics: View::default(),
            checks: std::array::from_fn(|_| CheckState::default()),
            check_requested: None,
            inventory_epoch: 0,
            explorer: Explorer::default(),
            inventory,
            source_label,
            query: String::new(),
            filter: None,
            show_all: false,
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

    pub(crate) fn sync_explorer(&mut self) {
        let node = self.selected_node();
        if self.explorer.installation.as_ref().map(|s| s.node) != node {
            self.explorer.select(&self.graph, node);
        }
    }

    pub fn jump_to(&mut self, target: Installation) {
        if target.node >= self.inventory.resources.len() {
            return;
        }
        self.remember();
        // Packages have no inspector: select the package in the list; Back returns.
        self.presentation.page = crate::presentation::Page::Browse;
        self.diagnostics = View::default();
        // Related packages may be hidden by the current query or Tools view. Back restores that context.
        self.query.clear();
        self.filter = None;
        self.show_all |= !self.is_tool(&self.inventory.resources[target.node]);
        self.refresh();
        self.selected = self.visible.iter().position(|&node| node == target.node);
        self.explorer.select(&self.graph, Some(target.node));
        if target.keg.is_some() {
            self.explorer.installation = Some(target);
        }
        self.explorer.section = Section::Overview;
        self.detail_scroll = 0;
        self.focus = Focus::Inventory;
        self.sync_explorer();
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

    /// " · 3 outdated", or nothing. Homebrew compares with its last `brew update`.
    pub fn outdated_text(&self) -> String {
        let n = self
            .inventory
            .resources
            .iter()
            .filter(|r| r.homebrew.as_ref().is_some_and(|m| m.update().is_some()))
            .count();
        if n == 0 {
            String::new()
        } else {
            format!(" · {n} outdated")
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
                "Loaded {} packages{} | r:refresh",
                self.inventory.resources.len(),
                self.outdated_text()
            ),
            CollectionStatus::Failed { error, stale } => {
                format!("{}{} | r:retry", if *stale { "STALE: " } else { "" }, error)
            }
        }
    }

    fn matches_scope(&self, r: &Resource) -> bool {
        if self.filter.is_some_and(|kind| r.kind != kind) {
            return false;
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
        ));
        crate::inventory::matches_query(&self.query, &haystack)
    }
    /// A tool is a package Homebrew recorded as installed on request. Missing or
    /// conflicting records are not guessed, so those packages appear only in All.
    pub fn is_tool(&self, resource: &Resource) -> bool {
        resource.intent() == InstallationIntent::Requested
    }
    /// True when the Homebrew inventory is limited to Tools.
    pub fn tools_only(&self) -> bool {
        !self.show_all && self.is_homebrew()
    }
    /// Counts for the search scope, independent of Tools/All.
    pub fn homebrew_counts(&self) -> HomebrewCounts {
        let mut counts = HomebrewCounts::default();
        for resource in self
            .inventory
            .resources
            .iter()
            .filter(|r| self.matches_scope(r))
        {
            counts.all += 1;
            counts.tools += usize::from(self.is_tool(resource));
        }
        counts
    }
    pub fn refresh(&mut self) {
        let previous = self.selected.and_then(|i| self.visible.get(i)).copied();

        self.visible = self
            .inventory
            .resources
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                (self.matches_scope(r) && (!self.tools_only() || self.is_tool(r))).then_some(i)
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
        if !self.inspecting() {
            self.focus_inventory();
        }
        if self.presentation_key(key) {
            return;
        }
        let previous = self.navigation_checkpoint(key);
        self.handle_key_inner(key);
        self.finish_navigation(previous);
    }
    pub(crate) fn handle_key_inner(&mut self, key: KeyEvent) {
        if self.module == Module::Agents {
            self.agents_key(key);
            return;
        }
        if self.module == Module::Cloud {
            let quit = match self.cloud {
                CloudSection::Aws => self.aws.handle(key),
                CloudSection::Kubernetes => self.kube.handle(key),
            };
            if quit {
                self.quit = true;
            }
            return;
        }
        if self.module == Module::Docker {
            if self.docker.handle(key) {
                self.quit = true;
            }
            return;
        }
        if self.module == Module::Toolchains {
            if self.toolchains.handle(key) {
                self.quit = true;
            }
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
            KeyCode::Backspace => self.back(),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::Inventory;
            }
            // Homebrew has no type filter: casks are bracketed in the list instead.
            KeyCode::Char('f') if !self.is_homebrew() => {
                let kinds = &ResourceKind::ALL;
                self.filter = match self.filter {
                    None => Some(kinds[0]),
                    Some(kind) => kinds
                        .iter()
                        .position(|&k| k == kind)
                        .and_then(|i| kinds.get(i + 1).copied()),
                };
                self.refresh();
            }
            KeyCode::Char('a') if self.is_homebrew() && self.focus == Focus::Inventory => {
                self.show_all = !self.show_all;
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
            KeyCode::Char('s' | 'V' | 't') => {}
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
