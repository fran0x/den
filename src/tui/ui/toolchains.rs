//! The Toolchains area: PATH, Rust and Packages, with the same layout as Cloud.
use super::*;
use crate::areas::toolchains::{Class, Row, Section};

pub(super) const HELP: &str = "Toolchains: language tools on this machine, read from files\n\
PATH: for each common command (node, python3, go, java, cargo, …) the copy the shell runs, who provides it (Homebrew, rustup, uv, nvm, mise, the system, …), its version when the install path shows it, and the copies it hides. Problems: a relative PATH entry, a manager folder that is not on PATH, a system copy hiding a managed one, or a macOS stub with nothing behind it (java without a JDK, python3 without the Command Line Tools).\n\
Rust: rustup toolchains (the default, folder overrides, targets and components), then cargo install crates with their version, source and programs.\n\
Packages: global npm packages, uv tools and Pythons, pipx, bun and go install programs.\n\
Nothing is run: versions come from install paths and package metadata, so programs installed straight into a system folder show no version. Registry and git URLs and local source paths are withheld. Commands Homebrew installed show its newer version (↑) from the Homebrew data Den already has.\n\
u checks online, after asking: it sends the names of crates.io crates and npm, bun, uv and pipx packages to index.crates.io, registry.npmjs.org and pypi.org, and the names of Rust's stable and line toolchains and of node, python3, ruby and go to endoflife.date (versions stay here). It marks the newer ones ↑, and a release line whose support has ended End of life. The answers are kept with the time of the check; only u checks again. Esc stops a check.\n\
In dew, x on a Rust toolchain offers rustup update or uninstall; on a cargo install crate, cargo install --locked (crates.io only) or uninstall; on a global npm, uv tool or pipx package, upgrade or uninstall; on a PATH command Homebrew installed, that package's Homebrew actions. Each tool runs as the copy that wins on PATH.\n\
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
                    let ended = rows.iter().filter(|r| ended(app, r).is_some()).count();
                    format!(
                        "{} on PATH · {problems}{}{}{}",
                        crate::inventory::count(
                            rows.iter().filter(|r| r.class == Class::Command).count(),
                            "command"
                        ),
                        if updates > 0 {
                            format!(" · {}", crate::inventory::count(updates, "update"))
                        } else {
                            String::new()
                        },
                        if ended > 0 {
                            format!(" · {ended} end of life")
                        } else {
                            String::new()
                        },
                        online(app, rows)
                    )
                }
                Section::Rust => format!(
                    "{} · default {} · {} · {problems}{}",
                    crate::inventory::count(
                        rows.iter().filter(|r| r.class == Class::Toolchain).count(),
                        "toolchain"
                    ),
                    i.default_toolchain.as_deref().unwrap_or("not set"),
                    crate::inventory::count(
                        rows.iter().filter(|r| r.class == Class::Crate).count(),
                        "crate"
                    ),
                    online(app, rows),
                ),
                Section::Packages => {
                    let mut text = crate::inventory::count(rows.len(), "package");
                    for (manager, n) in &i.managers {
                        text.push_str(&format!(" · {manager} {n}"));
                    }
                    text.push_str(&online(app, rows));
                    text
                }
            }
        }
    };
    let coverage = t
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.coverage.join("\n"));
    let tracked = tracked(t);
    let now = std::time::SystemTime::now();
    let name = section.label();
    let text = t.selected_row().map_or_else(
        || {
            if let Some(empty) = &tracked.empty {
                format!("{empty}\n\n{HEADING}Coverage\n{coverage}")
            } else if !t.query.is_empty() {
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
            let resource = r
                .homebrew
                .as_ref()
                .and_then(|p| crate::updates::homebrew(&app.inventory, &p.name, p.cask));
            let homebrew = resource.and_then(crate::online::homebrew);
            // An online answer is newer than Homebrew's own data, so it replaces that line.
            let mut update = resource
                .and_then(crate::updates::from_homebrew)
                .filter(|_| !homebrew.as_ref().is_some_and(|l| app.online.knows(l)))
                .map_or_else(String::new, |v| {
                    format!("\n\n{UPDATE} {v} available from Homebrew, as of its last data refresh")
                });
            // Homebrew's answer, then the row's own (a registry or its release line).
            for lookup in homebrew.iter().chain(&r.online) {
                update.push_str(&app.online.panel(lookup, now));
            }
            format!(
                "{HEADING}{}{update}{}\n\n{HEADING}Coverage\n{coverage}",
                r.detail,
                t.changes.panel(name, &r.key, now)
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
        status: if t.inventory.is_some() && !(t.rows().is_empty() && !t.changes.only()) {
            format!("{message}{}", tracked.status)
        } else {
            String::new()
        },
        rows: t
            .rows()
            .iter()
            .map(|r| {
                let mut status = match update(app, r) {
                    Some(v) => format!("{UPDATE} {v} · {}", r.status),
                    None => r.status.to_string(),
                };
                if ended(app, r).is_some() {
                    status = format!("End of life · {status}");
                }
                let mark = t.changes.mark(name, &r.key);
                (format!("{mark}{}", r.name), r.kind.clone(), status)
            })
            .collect(),
        text,
        selected: t.selected(),
        query: t.query.clone(),
        searching: t.searching,
        current: None,
        actions: app.actions.allowed(),
        filter: tracked.filter,
        removed: tracked.removed,
        changed: tracked.changed,
    };
    let t = &mut app.toolchains;
    list_view(frame, view, &mut t.page, &mut t.panel);
}

/// A newer version: for a command Homebrew installed, from the Homebrew data Den already
/// has (none in offline modes); for a package, from the last online check (`u`).
fn update(app: &App, row: &Row) -> Option<String> {
    if let Some(package) = &row.homebrew {
        return crate::updates::homebrew(&app.inventory, &package.name, package.cask)
            .and_then(|r| app.homebrew_update(r));
    }
    row.online
        .as_ref()
        .and_then(|l| app.online.newer(l))
        .map(str::to_owned)
}

/// The row's release line, when the last online check found its support has ended.
fn ended<'a>(app: &'a App, row: &Row) -> Option<&'a crate::online::Release> {
    row.online.as_ref().and_then(|l| app.online.ended(l))
}

/// The status line's online part for a section's rows. A command Homebrew installed
/// takes its newer version from Homebrew (`brew upgrade` may move to a newer line), so
/// its release line's newest patch is not counted.
fn online(app: &App, rows: &[Row]) -> String {
    let lookups: Vec<_> = rows
        .iter()
        .filter(|r| r.homebrew.is_none())
        .filter_map(|r| r.online.clone())
        .collect();
    online_status(app, &lookups)
}
