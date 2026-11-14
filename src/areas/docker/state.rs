//! Docker collection and browsing state: Containers, Images and Volumes sections, with a
//! refresh while the area stays open.
use super::{Context, Inventory, Row, Section};
use crate::inventory::safe_text;
use std::{
    io,
    time::{Duration, Instant},
};

pub type State = crate::listing::State<Context, Inventory>;

/// While the Docker area is open, live listings are rerun this often after the last run.
pub const REFRESH_EVERY: Duration = Duration::from_secs(5);

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Containers => "Containers",
            Self::Images => "Images",
            Self::Volumes => "Volumes",
        }
    }
}
impl crate::listing::Section for Section {
    fn slot(self) -> usize {
        self as usize
    }
}

impl crate::listing::Listing for Inventory {
    type Row = Row;
    type Section = Section;
    fn rows(&self, section: Section) -> &[Row] {
        match section {
            Section::Containers => &self.containers,
            Section::Images => &self.images,
            Section::Volumes => &self.volumes,
        }
    }
    fn key(row: &Row) -> &str {
        &row.key
    }
    fn search_text(r: &Row) -> String {
        format!("{} {} {}", r.name, r.state, r.status)
    }
    fn failure(error: &io::Error) -> String {
        safe_text(&error.to_string())
    }
    fn section_name(section: Section) -> &'static str {
        section.label()
    }
    fn entries(&self) -> Vec<crate::snapshot::Entry> {
        crate::snapshot::docker(self)
    }
}

impl State {
    /// Time until the next automatic refresh, or `None` when there is none: fixtures
    /// stay as saved, and nothing is scheduled before the first run or during one.
    pub fn next_refresh(&self, now: Instant) -> Option<Duration> {
        let live = self.context.as_ref()?.fixture.is_none();
        let finished = self
            .finished_at
            .filter(|_| live && !self.running && !self.requested)?;
        Some((finished + REFRESH_EVERY).saturating_duration_since(now))
    }
    /// Asks for a refresh once it is due.
    pub fn tick(&mut self, now: Instant) {
        if self.next_refresh(now) == Some(Duration::ZERO) {
            self.requested = true;
        }
    }
}
