//! Docker collection and browsing state: Containers, Images and Volumes sections.
use super::{Context, Inventory, Row, Section};
use crate::{
    inventory::{matches_query, safe_text},
    presentation::PanelScroll,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io;

pub struct State {
    pub context: Option<Context>,
    pub inventory: Option<Inventory>,
    pub requested: bool,
    pub running: bool,
    pub error: Option<String>,
    pub section: Section,
    /// Selected position in the visible rows, per section.
    pub selected: [usize; 3],
    pub query: String,
    pub searching: bool,
    pub panel: PanelScroll,
    pub page: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            context: None,
            inventory: None,
            requested: false,
            running: false,
            error: None,
            section: Section::Containers,
            selected: [0; 3],
            query: String::new(),
            searching: false,
            panel: PanelScroll::default(),
            page: 0,
        }
    }
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Containers => "Containers",
            Self::Images => "Images",
            Self::Volumes => "Volumes",
        }
    }
}

impl State {
    pub fn configure(&mut self, context: Context) {
        self.context = Some(context);
        self.requested = true;
    }
    /// Keeps the same row selected, matched by its key. A failure keeps the previous
    /// results and says why.
    pub fn finish(&mut self, result: io::Result<Inventory>) {
        self.running = false;
        let key = self.selected_row().map(|r| r.key.clone());
        match result {
            Ok(inventory) => {
                self.inventory = Some(inventory);
                self.error = None;
                if let Some(key) = key {
                    self.select_key(&key);
                }
                self.clamp();
            }
            Err(e) => self.error = Some(safe_text(&e.to_string())),
        }
    }
    pub fn slot(&self) -> usize {
        self.section as usize
    }
    /// The rows shown in the current section, after the search.
    pub fn rows(&self) -> Vec<&Row> {
        let Some(i) = &self.inventory else {
            return vec![];
        };
        let rows = match self.section {
            Section::Containers => &i.containers,
            Section::Images => &i.images,
            Section::Volumes => &i.volumes,
        };
        rows.iter()
            .filter(|r| {
                matches_query(
                    &self.query,
                    &safe_text(&format!("{} {} {}", r.name, r.state, r.status)),
                )
            })
            .collect()
    }
    pub fn selected_row(&self) -> Option<&Row> {
        self.rows().get(self.selected[self.slot()]).copied()
    }
    pub fn select_key(&mut self, key: &str) {
        if let Some(n) = self.rows().iter().position(|r| r.key == key) {
            let slot = self.slot();
            self.selected[slot] = n;
        }
    }
    pub fn clamp(&mut self) {
        let len = self.rows().len();
        let slot = self.slot();
        self.selected[slot] = self.selected[slot].min(len.saturating_sub(1));
    }
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        if self.searching {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear()
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && self.query.len() < 256
                        && safe_text(&ch.to_string()) == ch.to_string() =>
                {
                    self.query.push(ch)
                }
                _ => {}
            }
            let slot = self.slot();
            self.selected[slot] = 0;
            self.panel.offset = 0;
            self.clamp();
            return false;
        }
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('r') if self.context.is_some() && !self.running => self.requested = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Home => self.move_by(-isize::MAX),
            KeyCode::End => self.move_by(isize::MAX),
            KeyCode::PageDown => self.panel.by(1),
            KeyCode::PageUp => self.panel.by(-1),
            _ => {}
        }
        false
    }
    fn move_by(&mut self, delta: isize) {
        let len = self.rows().len();
        let slot = self.slot();
        let before = self.selected[slot];
        self.selected[slot] = before
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
        if self.selected[slot] != before {
            self.panel.offset = 0;
        }
    }
}
