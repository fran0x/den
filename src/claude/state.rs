//! Claude Code collection state. Browsing is in `agents`.
use super::{Context, Inventory};
use std::io;

#[derive(Default)]
pub struct State {
    pub context: Option<Context>,
    pub inventory: Option<Inventory>,
    pub requested: bool,
    pub running: bool,
    pub error: Option<String>,
}

impl State {
    pub fn configure(&mut self, context: Context) {
        self.context = Some(context);
        self.requested = true;
    }
    pub fn finish(&mut self, result: io::Result<Inventory>) {
        self.running = false;
        match result {
            Ok(inventory) => {
                self.inventory = Some(inventory);
                self.error = None;
            }
            Err(_) => {
                self.error = Some("Claude Code collection failed; previous results retained".into())
            }
        }
    }
}
