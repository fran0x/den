//! The Docker area: Containers, Images and Volumes, with the same layout as Cloud.
use super::*;
use crate::areas::docker::Section;

pub(super) const HELP: &str = "Docker: containers, images, volumes\n\
Read with docker ls and system df, every 5 s while open.\n\
Logins, environment and logs are not shown.";

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let d = &app.docker;
    let section = d.section;
    let status = match (&d.error, &d.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if d.context.is_none() => "Docker not collected in this mode".into(),
        (None, None) => format!("{} Running docker … (read-only listing)", spinner()),
        (None, Some(i)) => {
            let disk = |kind: &str| {
                i.disk
                    .iter()
                    .find(|(t, ..)| t == kind)
                    .map(|(_, size, reclaim)| format!(" · {size} · {reclaim} reclaimable"))
                    .unwrap_or_default()
            };
            let mut text = match section {
                Section::Containers => format!(
                    "{} · {} running{}",
                    crate::inventory::count(i.containers.len(), "container"),
                    i.running,
                    disk("Containers")
                ),
                Section::Images => format!(
                    "{}{}{}",
                    crate::inventory::count(i.images.len(), "image"),
                    disk("Images"),
                    i.disk
                        .iter()
                        .find(|(t, ..)| t == "Build Cache")
                        .map(|(_, size, _)| format!(" · build cache {size}"))
                        .unwrap_or_default()
                ),
                Section::Volumes => format!(
                    "{}{}",
                    crate::inventory::count(i.volumes.len(), "volume"),
                    disk("Local Volumes")
                ),
            };
            if section == Section::Images {
                let lookups: Vec<_> = i.images.iter().filter_map(|r| r.online.clone()).collect();
                text.push_str(&online_status(app, &lookups));
            }
            // Lines Docker printed that were not JSON: rows are missing, so say so.
            if i.unreadable > 0 {
                text.push_str(&format!(
                    " · {} unreadable",
                    crate::inventory::count(i.unreadable, "line")
                ));
            }
            text
        }
    };
    // Until Docker answers, the list carries the message; the status line stays quiet.
    let message = status;
    let status = if d.inventory.is_some() {
        message.clone()
    } else {
        String::new()
    };
    let tracked = tracked(d);
    let now = std::time::SystemTime::now();
    let name = section.label();
    let rows: Vec<_> = d
        .rows()
        .iter()
        .map(|r| {
            let mark = d.changes.mark(name, &r.key);
            let status = match r.online.as_ref().and_then(|l| app.online.newer(l)) {
                Some(newer) => format!("{UPDATE} {newer} · {}", r.status),
                None => r.status.clone(),
            };
            (format!("{mark}{}", r.name), r.state.clone(), status)
        })
        .collect();
    let setup = d
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.setup.join("\n"));
    let text = d.selected_row().map_or_else(
        || {
            if let Some(empty) = &tracked.empty {
                empty.clone()
            } else if !d.query.is_empty() {
                "No matches.\n/:search".into()
            } else if d.inventory.is_some() {
                format!("No {} found.", section.label().to_lowercase())
            } else {
                message.clone()
            }
        },
        |r| {
            let online = r
                .online
                .as_ref()
                .map_or_else(String::new, |l| app.online.panel(l, now));
            format!(
                "{HEADING}{}{online}{}\n\n{HEADING}Docker setup\n{}\nRead with docker container ls, image ls, volume ls and system df; nothing else is run.",
                r.detail,
                d.changes.panel(name, &r.key, now),
                if setup.is_empty() { "No ~/.docker/config.json details" } else { &setup }
            )
        },
    );
    let (title, pane, header) = match section {
        Section::Containers => (
            "Docker / Containers",
            " Containers ",
            ("Container", "State", "Image"),
        ),
        Section::Images => ("Docker / Images", " Images ", ("Image", "Size", "Used by")),
        Section::Volumes => (
            "Docker / Volumes",
            " Volumes ",
            ("Volume", "Driver", "Used by"),
        ),
    };
    let view = ListView {
        title,
        pane,
        header,
        status: if d.inventory.is_some() {
            format!("{status}{}", tracked.status)
        } else {
            status
        },
        rows,
        text,
        selected: d.selected(),
        query: d.query.clone(),
        searching: d.searching,
        current: None,
        keys: app.action_keys(),
        filter: tracked.filter,
        removed: tracked.removed,
    };
    let d = &mut app.docker;
    list_view(frame, view, &mut d.page, &mut d.panel);
}
