//! Kubernetes collection and browsing state for Cloud › Kubernetes.
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
        "Kubernetes collection failed; previous results retained".into()
    }
    fn section_name(_: ()) -> &'static str {
        "Kubernetes"
    }
    fn entries(&self) -> Vec<crate::snapshot::Entry> {
        crate::snapshot::kube(self)
    }
}
