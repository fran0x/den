//! Loading a saved Den inventory (`--inventory`). Homebrew collection and raw snapshots live in `homebrew`.
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read},
    path::Path,
};

use crate::inventory::Inventory;

pub const MAX_SNAPSHOT_BYTES: u64 = 1_048_576;
const DEMO: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/demo.json"));

pub fn demo() -> io::Result<Inventory> {
    parse(DEMO.as_bytes())
}

pub fn load_inventory(path: &Path) -> io::Result<Inventory> {
    read_snapshot(open_snapshot(path)?)
}

pub(crate) fn open_snapshot(path: &Path) -> io::Result<File> {
    // Reject directories and special files before opening (e.g. a blocking FIFO).
    if !path.metadata()?.is_file() {
        return Err(invalid("the file must be a regular file"));
    }
    let file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(invalid("the file must be a regular file"));
    }
    Ok(file)
}

pub fn read_snapshot(reader: impl Read) -> io::Result<Inventory> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SNAPSHOT_BYTES {
        return Err(invalid("the inventory exceeds the 1 MiB limit"));
    }
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> io::Result<Inventory> {
    let inventory: Inventory = serde_json::from_slice(bytes).map_err(|error| {
        // Do not echo arbitrary snapshot contents in diagnostics.
        invalid(&format!(
            "invalid inventory JSON/schema at line {}, column {}",
            error.line(),
            error.column()
        ))
    })?;
    if inventory.schema_version != 1 {
        return Err(invalid("unsupported schema_version; expected 1"));
    }
    if inventory.resources.len() > 512 {
        return Err(invalid("the inventory exceeds the 512 resource limit"));
    }
    let mut ids = HashSet::new();
    for resource in &inventory.resources {
        if resource.id.trim().is_empty() || resource.name.trim().is_empty() {
            return Err(invalid("resource id and name must not be empty"));
        }
        if !ids.insert(&resource.id) {
            return Err(invalid("resource ids must be unique"));
        }
        if [
            &resource.id,
            &resource.name,
            &resource.description,
            &resource.origin,
            &resource.reason,
        ]
        .into_iter()
        .chain(resource.version.iter())
        .chain(resource.path.iter())
        .any(|value| value.len() > 8192)
        {
            return Err(invalid("resource fields must not exceed 8192 bytes"));
        }
    }
    Ok(inventory)
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
