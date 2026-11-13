//! The Toolchains area: PATH, Rust and Packages, with the same layout as Cloud.
use super::*;
use crate::areas::toolchains::{Class, Row, Section};

pub(super) const HELP: &str = "Toolchains: language tools on this machine, read from files\n\
PATH: for each common command (node, python3, go, java, cargo, …) the copy the shell runs, who provides it (Homebrew, rustup, uv, nvm, mise, the system, …), its version when the install path shows it, and the copies it hides. Problems: a relative PATH entry, a manager folder that is not on PATH, a system copy hiding a managed one, or a macOS stub with nothing behind it (java without a JDK, python3 without the Command Line Tools).\n\
Rust: rustup toolchains (the default, folder overrides, targets and components), then cargo install crates with their version, source and programs.\n\
Packages: global npm packages, uv tools and Pythons, pipx, bun and go install programs.\n\
Nothing is run: versions come from install paths and package metadata, so programs installed straight into a system folder show no version. Registry and git URLs and local source paths are withheld. Commands Homebrew installed show its newer version (↑) from the Homebrew data Den already has; nothing else is checked for updates.\n\
/ searches; PgUp/PgDn scroll the panel; Shift+Tab switches sections; r refreshes.";

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let t = &app.toolchains;
    let section = t.section;
    let message = match (&t.error, &t.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if t.context.is_none() => "Toolchains not read in this mode".into(),
        (None, None) => format!("{} Reading toolchains…", spinner()),
        (None, Some(i)) => {
            let rows = i.rows(section);
            let problems = rows.iter().filter(|r| r.status.is_problem()).count();
            let problems = crate::inventory::count(problems, "problem");
            match section {
                Section::Path => {
                    let updates = rows.iter().filter(|r| update(app, r).is_some()).count();
                    format!(
                        "{} on PATH · {problems}{}",
                        crate::inventory::count(
                            rows.iter().filter(|r| r.class == Class::Command).count(),
                            "command"
                        ),
                        if updates > 0 {
                            format!(" · {}", crate::inventory::count(updates, "update"))
                        } else {
                            String::new()
                        }
                    )
                }
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
        |r| {
            let update = update(app, r).map_or_else(String::new, |v| {
                format!("\n\n{UPDATE} {v} available from Homebrew, as of its last data refresh")
            });
            format!(
                "{HEADING}{}{update}\n\n{HEADING}Coverage\n{coverage}",
                r.detail
            )
        },
    );
    let (title, header) = match section {
        Section::Path => ("Toolchains / PATH", ("Command", "From", "Status")),
        Section::Rust => ("Toolchains / Rust", ("Name", "Kind", "Status")),
        Section::Packages => (
            "Toolchains / Packages",
            ("Package", "Installed with", "Status"),
        ),
    };
    let view = ListView {
        title,
        pane: match section {
            Section::Path => " PATH ",
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
            .map(|r| {
                let status = match update(app, r) {
                    Some(v) => format!("{UPDATE} {v} · {}", r.status),
                    None => r.status.to_string(),
                };
                (r.name.clone(), r.kind.clone(), status)
            })
            .collect(),
        text,
        selected: t.selected(),
        query: t.query.clone(),
        searching: t.searching,
        current: None,
    };
    let t = &mut app.toolchains;
    list_view(frame, view, &mut t.page, &mut t.panel);
}

/// Homebrew's newer version of a command it installed, from the Homebrew data Den
/// already has (none in offline modes).
fn update(app: &App, row: &Row) -> Option<String> {
    let package = row.homebrew.as_ref()?;
    crate::updates::homebrew(&app.inventory, &package.name, package.cask)
        .and_then(crate::updates::from_homebrew)
}
