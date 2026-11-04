//! Presentation-only state shared by the existing explorers. No collection decisions.
use crate::{
    app::{App, Focus, Module},
    codex::state::View,
};
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Default)]
pub struct Presentation {
    pub detailed: bool,
    inspectors: [bool; 5],
    return_focus: [bool; 5],
    alternate_scroll: [u16; 5],
}
impl App {
    fn presentation_slot(&self) -> usize {
        match (self.module, self.codex.view) {
            (Module::Homebrew, _) => 0,
            (_, View::Skills) => 1,
            (_, View::Configuration) => 2,
            (_, View::Instructions) => 3,
            (_, View::Mcp) => 4,
        }
    }
    pub fn inspecting(&self) -> bool {
        self.presentation.inspectors[self.presentation_slot()]
    }
    pub fn active_focus(&self) -> Focus {
        match self.presentation_slot() {
            0 => self.focus,
            1 => self.skills.focus,
            _ => self.codex.focus(),
        }
    }
    fn set_active_focus(&mut self, details: bool) {
        let focus = if details {
            Focus::Details
        } else {
            Focus::Inventory
        };
        match self.presentation_slot() {
            0 => self.focus = focus,
            1 => self.skills.focus = focus,
            _ => self.codex.focus_details = details,
        }
    }
    pub fn active_searching(&self) -> bool {
        match self.presentation_slot() {
            0 => self.searching,
            1 => self.skills.searching,
            _ => self.codex.searching,
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
            }
            return true;
        }
        if key.code == KeyCode::F(1) || (!self.active_searching() && key.code == KeyCode::Char('?'))
        {
            self.help = true;
            self.help_scroll = 0;
            return true;
        }
        if self.active_searching() {
            return false;
        }
        let slot = self.presentation_slot();
        match key.code {
            KeyCode::Char('v') => {
                self.presentation.detailed = !self.presentation.detailed;
                let scroll = match slot {
                    0 => &mut self.detail_scroll,
                    1 => &mut self.skills.scroll,
                    _ => &mut self.codex.scroll,
                };
                std::mem::swap(scroll, &mut self.presentation.alternate_scroll[slot]);
                true
            }
            KeyCode::Enter if !self.inspecting() && self.active_focus() == Focus::Inventory => {
                self.presentation.return_focus[slot] = false;
                self.presentation.inspectors[slot] = true;
                self.set_active_focus(true);
                true
            }
            // A full-screen inspector can also be opened from a focused detail pane.
            KeyCode::Char('z') => {
                if self.inspecting() {
                    self.close_inspector();
                } else {
                    self.presentation.return_focus[slot] = self.active_focus() == Focus::Details;
                    self.presentation.inspectors[slot] = true;
                    self.set_active_focus(true);
                }
                true
            }
            KeyCode::Esc if self.module == Module::Skills && self.codex.has_source_return() => {
                self.codex
                    .handle(KeyEvent::new(KeyCode::Backspace, key.modifiers));
                true
            }
            KeyCode::Esc if self.inspecting() => {
                self.close_inspector();
                true
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left if self.inspecting() => {
                self.close_inspector();
                self.set_active_focus(false);
                true
            }
            KeyCode::Esc if self.active_focus() == Focus::Details => {
                self.set_active_focus(false);
                true
            }
            KeyCode::Right if self.active_focus() == Focus::Inventory => {
                self.set_active_focus(true);
                true
            }
            KeyCode::BackTab
                if self.module == Module::Skills && self.codex.view != View::Skills =>
            {
                self.set_active_focus(self.active_focus() == Focus::Inventory);
                true
            }
            KeyCode::Char('/') => {
                if self.inspecting() {
                    self.close_inspector();
                }
                self.set_active_focus(false);
                false
            }
            KeyCode::F(3) if self.module == Module::Homebrew => {
                let next = if let Some(tab) = self.diagnostics.tab {
                    match tab {
                        crate::diagnostics::Tab::Unresolved => '6',
                        crate::diagnostics::Tab::Missing => '7',
                        crate::diagnostics::Tab::Leaves => '8',
                        crate::diagnostics::Tab::Cleanup => '1',
                    }
                } else {
                    match self.explorer.section {
                        crate::explorer::Section::Overview => '2',
                        crate::explorer::Section::Dependencies => '3',
                        crate::explorer::Section::RequiredBy => '4',
                        crate::explorer::Section::Paths => '5',
                    }
                };
                self.handle_key(KeyEvent::new(KeyCode::Char(next), key.modifiers));
                true
            }
            _ => false,
        }
    }
    pub(crate) fn inspect_source(&mut self) {
        if self.codex.has_source_return() {
            let slot = self.presentation_slot();
            if !self.presentation.inspectors[slot] {
                self.presentation.return_focus[slot] = false;
                self.presentation.inspectors[slot] = true;
            }
            self.set_active_focus(true);
        }
    }
    // Native Back actions may restore inventory focus. Never leave it hidden.
    pub(crate) fn settle_inspector(&mut self) {
        if self.active_focus() == Focus::Inventory {
            let slot = self.presentation_slot();
            self.presentation.inspectors[slot] = false;
        }
    }
    fn close_inspector(&mut self) {
        let slot = self.presentation_slot();
        self.presentation.inspectors[slot] = false;
        self.set_active_focus(self.presentation.return_focus[slot]);
    }
}
