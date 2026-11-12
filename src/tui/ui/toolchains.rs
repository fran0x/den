//! The Toolchains area: On PATH, Rust and Packages, with the same layout as Cloud.
use super::*;
use crate::areas::toolchains::{Class, Section};

pub(super) const HELP: &str = "Toolchains: language tools on this machine, read from files\n\
On PATH: for each common command (node, python3, go, java, cargo, …) the copy the shell runs, who provides it (Homebrew, rustup, uv, nvm, mise, the system, …), its version when the install path shows it, and the copies it hides. Problems: a relative PATH entry, a manager folder that is not on PATH, a system copy hiding a managed one, or a macOS stub with nothing behind it (java without a JDK, python3 without the Command Line Tools).\n\
Rust: rustup toolchains (the default, folder overrides, targets and components), then cargo install crates with their version, source and programs.\n\
Packages: global npm packages, uv tools and Pythons, pipx, bun and go install programs.\n\
Nothing is run: versions come from install paths and package metadata, so programs installed straight into a system folder show no version. Registry and git URLs and local source paths are withheld. Newer versions are not checked.\n\
/ searches; PgUp/PgDn scroll the panel; Shift+Tab switches sections; r refreshes.";

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let t = &app.toolchains;
    let section = t.section;
    let message = match (&t.error, &t.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if t.context.is_none() => "Toolchains not read in this mode".into(),
        (None, None) => "Reading toolchains...".into(),
        (None, Some(i)) => {
            let rows = i.rows(section);
            let problems = rows.iter().filter(|r| r.status.is_problem()).count();
            let problems = crate::inventory::count(problems, "problem");
            match section {
                Section::Path => format!(
                    "{} on PATH · {problems}",
                    crate::inventory::count(
                        rows.iter().filter(|r| r.class == Class::Command).count(),
                        "command"
                    )
                ),
                Section::Rust => format!(
                    "{} · default {} · {} · {problems}",
                    crate::inventory::count(
                        rows.iter().filter(|r| r.class == Class::Toolchain).count(),
                        "toolchain"
                    ),
                    i.default_toolchain.as_deref().unwrap_or("not set"),
                    crate::inventory::count(
                        rows.iter().filter(|r| r.class == Class::Crate).count(),
                        "crate"
                    ),
                ),
                Section::Packages => {
                    let mut text = crate::inventory::count(rows.len(), "package");
                    for (manager, n) in &i.managers {
                        text.push_str(&format!(" · {manager} {n}"));
                    }
                    text
                }
            }
        }
    };
    let coverage = t
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.coverage.join("\n"));
    let text = t.selected_row().map_or_else(
        || {
            if !t.query.is_empty() {
                "No matches.\n/:search".into()
            } else if t.inventory.is_some() {
                let empty = match section {
                    Section::Path => "None of the usual language commands is on PATH.",
                    Section::Rust => "Rust not set up here: no rustup or cargo installs.",
                    Section::Packages => "No global packages: npm, uv, pipx, bun or go.",
                };
                format!("{empty}\n\n{HEADING}Coverage\n{coverage}")
            } else {
                message.clone()
            }
        },
        |r| format!("{HEADING}{}\n\n{HEADING}Coverage\n{coverage}", r.detail),
    );
    let (title, header) = match section {
        Section::Path => ("Toolchains / On PATH", ("Command", "From", "Status")),
        Section::Rust => ("Toolchains / Rust", ("Name", "Kind", "Status")),
        Section::Packages => (
            "Toolchains / Packages",
            ("Package", "Installed with", "Status"),
        ),
    };
    let view = ListView {
        title,
        pane: match section {
            Section::Path => " On PATH ",
            Section::Rust => " Rust ",
            Section::Packages => " Packages ",
        },
        header,
        // Until rows exist, the list carries the message; the status line stays quiet.
        status: if t.inventory.is_some() && !t.rows().is_empty() {
            message
        } else {
            String::new()
        },
        rows: t
            .rows()
            .iter()
            .map(|r| (r.name.clone(), r.kind.clone(), r.status.to_string()))
            .collect(),
        text,
        selected: t.selected(),
        query: t.query.clone(),
        searching: t.searching,
    };
    let t = &mut app.toolchains;
    list_view(frame, view, &mut t.page, &mut t.panel);
}
