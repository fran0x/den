//! Toolchains collection and browsing state: PATH, Rust and Packages sections.
use super::{Context, Inventory, Row, Section};
use crate::inventory::safe_text;
use std::io;

pub type State = crate::listing::State<Context, Inventory>;

impl crate::listing::Section for Section {
    fn slot(self) -> usize {
        self as usize
    }
}

impl crate::listing::Listing for Inventory {
    type Row = Row;
    type Section = Section;
    fn rows(&self, section: Section) -> &[Row] {
        Inventory::rows(self, section)
    }
    fn key(row: &Row) -> &str {
        &row.key
    }
    fn search_text(r: &Row) -> String {
        format!("{} {} {}", r.name, r.kind, r.status)
    }
    fn failure(error: &io::Error) -> String {
        safe_text(&error.to_string())
    }
    fn section_name(section: Section) -> &'static str {
        section.label()
    }
    fn entries(&self) -> Vec<crate::snapshot::Entry> {
        crate::snapshot::toolchains(self)
    }
}
