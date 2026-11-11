//! Kubernetes collection and browsing state for Cloud › Kubernetes.
use super::{Context, Inventory, Row};
use crate::{
    inventory::{matches_query, safe_text},
    presentation::PanelScroll,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io;

#[derive(Default)]
pub struct State {
    pub context: Option<Context>,
    pub inventory: Option<Inventory>,
    pub requested: bool,
    pub running: bool,
    pub error: Option<String>,
    pub selected: usize,
    pub query: String,
    pub searching: bool,
    pub panel: PanelScroll,
    pub page: usize,
}

impl State {
    pub fn configure(&mut self, context: Context) {
        self.context = Some(context);
        self.requested = true;
    }
    /// Keeps the same row selected, matched by its key.
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
            Err(_) => {
                self.error = Some("Kubernetes collection failed; previous results retained".into())
            }
        }
    }
    /// The rows shown, after the search.
    pub fn rows(&self) -> Vec<&Row> {
        self.inventory
            .as_ref()
            .map(|i| {
                i.rows
                    .iter()
                    .filter(|r| {
                        matches_query(
                            &self.query,
                            &safe_text(&format!("{} {} {}", r.name, r.auth, r.status)),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn selected_row(&self) -> Option<&Row> {
        self.rows().get(self.selected).copied()
    }
    pub fn select_key(&mut self, key: &str) {
        if let Some(n) = self.rows().iter().position(|r| r.key == key) {
            self.selected = n;
        }
    }
    pub fn clamp(&mut self) {
        self.selected = self.selected.min(self.rows().len().saturating_sub(1));
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
            self.selected = 0;
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
        let before = self.selected;
        self.selected = before
            .saturating_add_signed(delta)
            .min(self.rows().len().saturating_sub(1));
        if self.selected != before {
            self.panel.offset = 0;
        }
    }
}
