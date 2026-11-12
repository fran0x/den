//! Navigation snapshots contain view state only, never inventories or worker state.
use super::*;
use crate::{
    areas::homebrew::diagnostics, areas::homebrew::explorer::Explorer, inventory::ResourceKind,
};

#[derive(Clone)]
pub(super) enum Location {
    Homebrew {
        page: Page,
        id: Option<String>,
        query: String,
        filter: Option<ResourceKind>,
        show_all: bool,
        focus: Focus,
        scroll: u16,
        offset: usize,
        explorer: Explorer,
        diagnostics: diagnostics::View,
        epoch: u64,
    },
    /// Agents view state. Codex's source-files toggle, if-trusted scenario and MCP source
    /// choice live in the Codex state, so they are saved here too.
    Docker {
        section: crate::areas::docker::Section,
        key: Option<String>,
        selected: [usize; 3],
        query: String,
        scroll: u16,
    },
    Toolchains {
        section: crate::areas::toolchains::Section,
        key: Option<String>,
        selected: [usize; 3],
        query: String,
        scroll: u16,
    },
    Cloud {
        section: crate::app::CloudSection,
        key: Option<String>,
        selected: [usize; crate::listing::MAX_SECTIONS],
        query: String,
        scroll: u16,
    },
    Agents {
        section: crate::areas::agents::Section,
        key: Option<String>,
        selected: [usize; 6],
        query: String,
        scroll: u16,
        sources: bool,
        conditional: bool,
        mcp_source: usize,
    },
}
impl Location {
    fn module(&self) -> Module {
        match self {
            Self::Homebrew { .. } => Module::Homebrew,
            Self::Agents { .. } => Module::Agents,
            Self::Cloud { .. } => Module::Cloud,
            Self::Docker { .. } => Module::Docker,
            Self::Toolchains { .. } => Module::Toolchains,
        }
    }
}
impl App {
    pub(super) fn location(&self) -> Location {
        let page = self.presentation.page.clone();
        match self.module {
            Module::Toolchains => Location::Toolchains {
                section: self.toolchains.section,
                key: self.toolchains.selected_row().map(|r| r.key.clone()),
                selected: self.toolchains.selected,
                query: self.toolchains.query.clone(),
                scroll: self.toolchains.panel.offset,
            },
            Module::Docker => Location::Docker {
                section: self.docker.section,
                key: self.docker.selected_row().map(|r| r.key.clone()),
                selected: self.docker.selected,
                query: self.docker.query.clone(),
                scroll: self.docker.panel.offset,
            },
            Module::Cloud => match self.cloud {
                crate::app::CloudSection::Aws => Location::Cloud {
                    section: self.cloud,
                    key: self.aws.selected_row().map(|r| r.key.clone()),
                    selected: self.aws.selected,
                    query: self.aws.query.clone(),
                    scroll: self.aws.panel.offset,
                },
                crate::app::CloudSection::Kubernetes => Location::Cloud {
                    section: self.cloud,
                    key: self.kube.selected_row().map(|r| r.key.clone()),
                    selected: self.kube.selected,
                    query: self.kube.query.clone(),
                    scroll: self.kube.panel.offset,
                },
            },
            Module::Agents => Location::Agents {
                section: self.agents.section,
                key: self.agent_row().map(|r| r.key),
                selected: self.agents.selected,
                query: self.agents.query.clone(),
                scroll: self.agents.panel.offset,
                sources: self.codex.sources,
                conditional: self.codex.conditional,
                mcp_source: self.codex.mcp_source,
            },
            Module::Homebrew => {
                let mut explorer = self.explorer.clone();
                explorer.rows.clear();
                explorer.note.clear();
                Location::Homebrew {
                    page,
                    id: self.resource().map(|r| r.id.clone()),
                    query: self.query.clone(),
                    filter: self.filter,
                    show_all: self.show_all,
                    focus: self.focus,
                    scroll: self.detail_scroll,
                    offset: self.list_offset,
                    explorer,
                    diagnostics: self.diagnostics.clone(),
                    epoch: self.inventory_epoch,
                }
            }
        }
    }
    pub(super) fn restore(&mut self, location: Location) {
        if location.module() != self.module {
            self.presentation.areas[self.module.index()] = Some(self.location());
        }
        match location {
            Location::Docker {
                section,
                key,
                selected,
                query,
                scroll,
            } => {
                self.module = Module::Docker;
                let d = &mut self.docker;
                d.section = section;
                d.selected = selected;
                d.query = query;
                d.searching = false;
                if let Some(key) = key {
                    d.select_key(&key);
                }
                d.clamp();
                d.panel.offset = scroll;
                self.presentation.page = Page::Browse;
            }
            Location::Toolchains {
                section,
                key,
                selected,
                query,
                scroll,
            } => {
                self.module = Module::Toolchains;
                let t = &mut self.toolchains;
                t.section = section;
                t.selected = selected;
                t.query = query;
                t.searching = false;
                if let Some(key) = key {
                    t.select_key(&key);
                }
                t.clamp();
                t.panel.offset = scroll;
                self.presentation.page = Page::Browse;
            }
            Location::Cloud {
                section,
                key,
                selected,
                query,
                scroll,
            } => {
                self.module = Module::Cloud;
                self.cloud = section;
                match section {
                    crate::app::CloudSection::Aws => {
                        let a = &mut self.aws;
                        a.selected = selected;
                        a.query = query;
                        a.searching = false;
                        if let Some(key) = key {
                            a.select_key(&key);
                        }
                        a.clamp();
                        a.panel.offset = scroll;
                    }
                    crate::app::CloudSection::Kubernetes => {
                        let k = &mut self.kube;
                        k.selected = selected;
                        k.query = query;
                        k.searching = false;
                        if let Some(key) = key {
                            k.select_key(&key);
                        }
                        k.clamp();
                        k.panel.offset = scroll;
                    }
                }
                self.presentation.page = Page::Browse;
            }
            Location::Agents {
                section,
                key,
                selected,
                query,
                scroll,
                sources,
                conditional,
                mcp_source,
            } => {
                self.module = Module::Agents;
                self.codex.sources = sources;
                self.codex.conditional = conditional;
                self.codex.mcp_source = mcp_source;
                let a = &mut self.agents;
                a.section = section;
                a.selected = selected;
                a.query = query;
                a.searching = false;
                if let Some(key) = key {
                    self.agents_select(&key);
                }
                self.agents_clamp();
                self.agents.panel.offset = scroll;
                self.presentation.page = Page::Browse;
            }
            Location::Homebrew {
                page,
                id,
                query,
                filter,
                show_all,
                focus,
                scroll,
                offset,
                explorer,
                diagnostics,
                epoch,
            } => {
                self.module = Module::Homebrew;
                self.query = query;
                self.filter = filter;
                self.show_all = show_all;
                self.refresh();
                self.selected = id
                    .and_then(|id| {
                        self.visible
                            .iter()
                            .position(|&n| self.inventory.resources[n].id == id)
                    })
                    .or(self.selected);
                if epoch == self.inventory_epoch {
                    self.explorer = explorer;
                    self.diagnostics = diagnostics;
                } else {
                    self.explorer.section = explorer.section;
                    self.explorer.tree = explorer.tree;
                    self.explorer.evidence = explorer.evidence;
                    self.diagnostics.tab = diagnostics.tab;
                }
                self.sync_explorer();
                self.focus = focus;
                self.detail_scroll = scroll;
                self.list_offset = offset.min(self.visible.len().saturating_sub(1));
                self.searching = false;
                self.presentation.page = page;
            }
        }
        if !self.inspecting() {
            self.focus_inventory();
        }
    }
}
