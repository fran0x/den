//! Collection and browsing state shared by the list areas: Cloud (AWS, Kubernetes), Docker
//! and Toolchains. Each area's inventory implements [`Listing`]; the state keeps the
//! selection per section, the search and the panel scroll, and survives refreshes by
//! reselecting the same row key.
use crate::{
    inventory::{matches_query, safe_text},
    snapshot::{Entry, Tracking},
    tui::presentation::PanelScroll,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{io, time::Instant};

/// Most sections in one area; selections are kept per section.
pub const MAX_SECTIONS: usize = 3;

/// An area's sections. Single-section areas use `()`.
pub trait Section: Copy + Default + PartialEq {
    /// Position in the per-section selection, below [`MAX_SECTIONS`].
    fn slot(self) -> usize;
}
impl Section for () {
    fn slot(self) -> usize {
        0
    }
}

/// What a collected inventory offers the shared list.
pub trait Listing {
    type Row;
    type Section: Section;
    fn rows(&self, section: Self::Section) -> &[Self::Row];
    /// Stable identity, so a refresh keeps the same row selected.
    fn key(row: &Self::Row) -> &str;
    /// What a search matches against.
    fn search_text(row: &Self::Row) -> String;
    /// The status-line message for a failed collection.
    fn failure(error: &io::Error) -> String;
    /// The section's name in snapshot entries.
    fn section_name(section: Self::Section) -> &'static str;
    /// Every row, as a snapshot keeps it.
    fn entries(&self) -> Vec<Entry>;
}

pub struct State<C, I: Listing> {
    pub context: Option<C>,
    pub inventory: Option<I>,
    pub requested: bool,
    pub running: bool,
    pub error: Option<String>,
    pub section: I::Section,
    /// Selected position in the visible rows, per section.
    pub selected: [usize; MAX_SECTIONS],
    pub query: String,
    pub searching: bool,
    pub panel: PanelScroll,
    pub page: usize,
    /// When the last collection finished (Docker refreshes from here).
    pub finished_at: Option<Instant>,
    /// What changed since Den first read this list.
    pub changes: Tracking,
}
impl<C, I: Listing> Default for State<C, I> {
    fn default() -> Self {
        Self {
            context: None,
            inventory: None,
            requested: false,
            running: false,
            error: None,
            section: I::Section::default(),
            selected: [0; MAX_SECTIONS],
            query: String::new(),
            searching: false,
            panel: PanelScroll::default(),
            page: 0,
            finished_at: None,
            changes: Tracking::default(),
        }
    }
}

impl<C, I: Listing> State<C, I> {
    pub fn configure(&mut self, context: C) {
        self.context = Some(context);
        self.requested = true;
    }
    /// Keeps the same row selected, matched by its key. A failure keeps the previous
    /// results and says why.
    pub fn finish(&mut self, result: io::Result<I>) {
        self.running = false;
        self.finished_at = Some(Instant::now());
        let key = self.selected_row().map(|r| I::key(r).to_owned());
        match result {
            Ok(inventory) => {
                self.changes
                    .observe(inventory.entries(), std::time::SystemTime::now());
                self.inventory = Some(inventory);
                self.error = None;
                if let Some(key) = key {
                    self.select_key(&key);
                }
                self.clamp();
            }
            Err(e) => self.error = Some(I::failure(&e)),
        }
    }
    pub fn slot(&self) -> usize {
        self.section.slot()
    }
    /// The selected position in the current section.
    pub fn selected(&self) -> usize {
        self.selected[self.slot()]
    }
    /// The rows shown in the current section, after the search (and with `d`, changed
    /// rows only).
    pub fn rows(&self) -> Vec<&I::Row> {
        let Some(i) = &self.inventory else {
            return vec![];
        };
        let section = I::section_name(self.section);
        i.rows(self.section)
            .iter()
            .filter(|r| {
                self.changes.keeps(section, I::key(r))
                    && matches_query(&self.query, &safe_text(&I::search_text(r)))
            })
            .collect()
    }
    /// Rows removed since Den started, shown after the rows with `d`, after the search.
    pub fn removed(&self) -> Vec<&Entry> {
        self.changes
            .removed(I::section_name(self.section))
            .into_iter()
            .filter(|e| matches_query(&self.query, &safe_text(&e.name)))
            .collect()
    }
    /// The current section's name in snapshot entries.
    pub fn section_name(&self) -> &'static str {
        I::section_name(self.section)
    }
    pub fn selected_row(&self) -> Option<&I::Row> {
        self.rows().get(self.selected()).copied()
    }
    pub fn select_key(&mut self, key: &str) {
        if let Some(n) = self.rows().iter().position(|r| I::key(r) == key) {
            let slot = self.slot();
            self.selected[slot] = n;
        }
    }
    pub fn clamp(&mut self) {
        let len = self.rows().len();
        let slot = self.slot();
        self.selected[slot] = self.selected[slot].min(len.saturating_sub(1));
    }
    /// Returns true when the key asks to quit.
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
            KeyCode::Char('d') => {
                self.changes.next_view();
                let slot = self.slot();
                self.selected[slot] = 0;
                self.panel.offset = 0;
            }
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
