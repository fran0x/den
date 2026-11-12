//! AWS collection and browsing state for Cloud › AWS.
use super::{Context, Inventory, Row};
use std::io;

pub type State = crate::listing::State<Context, Inventory>;

impl crate::listing::Listing for Inventory {
    type Row = Row;
    type Section = ();
    fn rows(&self, _: ()) -> &[Row] {
        &self.rows
    }
    fn key(row: &Row) -> &str {
        &row.key
    }
    fn search_text(r: &Row) -> String {
        format!("{} {} {}", r.name, r.auth, r.status)
    }
    fn failure(_: &io::Error) -> String {
        "AWS collection failed; previous results retained".into()
    }
}
