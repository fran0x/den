//! Navigation snapshots contain view state only, never inventories or worker state.
use super::*;
use crate::{
    diagnostics,
    explorer::Explorer,
    inventory::{InstallationIntent, ResourceKind},
    skills,
};
use std::path::PathBuf;

#[derive(Clone)]
pub(super) enum Location {
    Homebrew {
        page: Page,
        id: Option<String>,
        query: String,
        filter: Option<ResourceKind>,
        intent: Option<InstallationIntent>,
        focus: Focus,
        scroll: u16,
        offset: usize,
        explorer: Explorer,
        diagnostics: diagnostics::View,
        epoch: u64,
    },
    Skills {
        page: Page,
        identity: Option<(skills::Scope, PathBuf)>,
        query: String,
        scope: Option<skills::Scope>,
        filter: Option<skills::state::Filter>,
        focus: Focus,
        section: skills::state::Section,
        file: usize,
        file_path: Option<PathBuf>,
        file_open: bool,
        scroll: u16,
    },
    Codex {
        page: Page,
        view: View,
        identity: Option<String>,
        query: String,
        selected: [usize; 4],
        scroll: u16,
        focus: bool,
        sources: bool,
        preview: bool,
        conditional: bool,
        mcp: crate::codex::mcp::state::Controls,
    },
}
impl App {
    pub(super) fn location(&self) -> Location {
        let page = self.presentation.page.clone();
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => {
                let mut explorer = self.explorer.clone();
                explorer.rows.clear();
                explorer.note.clear();
                Location::Homebrew {
                    page,
                    id: self.resource().map(|r| r.id.clone()),
                    query: self.query.clone(),
                    filter: self.filter,
                    intent: self.intent_filter,
                    focus: self.focus,
                    scroll: self.detail_scroll,
                    offset: self.list_offset,
                    explorer,
                    diagnostics: self.diagnostics.clone(),
                    epoch: self.inventory_epoch,
                }
            }
            (_, View::Skills) => Location::Skills {
                page,
                identity: self
                    .skills
                    .selected_skill()
                    .map(|s| (s.scope, s.path.clone())),
                query: self.skills.query.clone(),
                scope: self.skills.scope,
                filter: self.skills.filter,
                focus: self.skills.focus,
                section: self.skills.section,
                file: self.skills.file,
                file_path: self
                    .skills
                    .selected_skill()
                    .and_then(|s| s.files.get(self.skills.file))
                    .map(|f| f.path.clone()),
                file_open: self.skills.file_open,
                scroll: self.skills.scroll,
            },
            _ => Location::Codex {
                page,
                view: self.codex.view,
                identity: self.codex.selected_identity(),
                query: self.codex.query.clone(),
                selected: self.codex.selected,
                scroll: self.codex.scroll,
                focus: self.codex.focus_details,
                sources: self.codex.sources,
                preview: self.codex.preview,
                conditional: self.codex.conditional,
                mcp: self.codex.mcp.clone(),
            },
        }
    }
    pub(super) fn restore(&mut self, location: Location) {
        match location {
            Location::Homebrew {
                page,
                id,
                query,
                filter,
                intent,
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
                self.intent_filter = intent;
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
                    self.diagnostics.tab = diagnostics.tab;
                }
                self.sync_explorer();
                self.focus = focus;
                self.detail_scroll = scroll;
                self.list_offset = offset.min(self.visible.len().saturating_sub(1));
                self.searching = false;
                self.presentation.page = page;
            }
            Location::Skills {
                page,
                identity,
                query,
                scope,
                filter,
                focus,
                section,
                file,
                file_path,
                file_open,
                scroll,
            } => {
                self.module = Module::Skills;
                self.codex.view = View::Skills;
                let s = &mut self.skills;
                s.query = query;
                s.scope = scope;
                s.filter = filter;
                s.refresh_visible();
                s.selected = identity
                    .and_then(|id| {
                        s.visible.iter().position(|&n| {
                            (
                                s.inventory.skills[n].scope,
                                s.inventory.skills[n].path.clone(),
                            ) == id
                        })
                    })
                    .or(s.selected);
                s.file = file_path
                    .and_then(|p| {
                        s.selected_skill()
                            .and_then(|s| s.files.iter().position(|f| f.path == p))
                    })
                    .unwrap_or(file)
                    .min(
                        s.selected_skill()
                            .map_or(0, |s| s.files.len().saturating_sub(1)),
                    );
                s.section = section;
                s.file_open = file_open;
                s.focus = focus;
                s.scroll = scroll;
                s.searching = false;
                self.presentation.page = page;
            }
            Location::Codex {
                page,
                view,
                identity,
                query,
                selected,
                scroll,
                focus,
                sources,
                preview,
                conditional,
                mcp,
            } => {
                self.module = Module::Skills;
                let s = &mut self.codex;
                s.view = view;
                s.query = query;
                s.selected = selected;
                s.sources = sources;
                s.preview = preview;
                s.conditional = conditional;
                s.mcp = mcp;
                if let Some(id) = identity
                    && let Some(n) = s.rows().iter().position(|(n, _)| s.identity(*n) == id)
                {
                    let slot = s.slot();
                    s.selected[slot] = n;
                }
                s.clamp();
                s.scroll = scroll;
                s.focus_details = focus;
                s.searching = false;
                self.presentation.page = page;
            }
        }
    }
}
