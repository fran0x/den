//! The Docker area: Containers, Images and Volumes, with the same layout as Cloud.
use super::*;
use crate::docker::Section;

pub(super) const HELP: &str = "Docker: containers, images and volumes\n\
Shift+Tab switches Containers / Images / Volumes. Containers show their state and image (and Compose project); images and volumes say which containers use them, or Unused. The status line shows disk use and what docker system df could reclaim.\n\
Den runs only: docker container ls --all, docker image ls, docker volume ls and docker system df, each with a 10-second limit, and only once this area is opened. Nothing is started, stopped, pulled, inspected or removed.\n\
Not shown: container commands, environment, labels (except the Compose project) and logs. From ~/.docker/config.json only the context, registry names and credential helper are kept; saved logins are discarded. DOCKER_HOST is noted, not shown.\n\
/ searches; PgUp/PgDn scroll the panel; r refreshes.";

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let d = &app.docker;
    let section = d.section;
    let status = match (&d.error, &d.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if d.context.is_none() => "Docker not collected in this mode".into(),
        (None, None) => "Running docker ... (read-only listing)".into(),
        (None, Some(i)) => {
            let disk = |kind: &str| {
                i.disk
                    .iter()
                    .find(|(t, ..)| t == kind)
                    .map(|(_, size, reclaim)| format!(" · {size} · {reclaim} reclaimable"))
                    .unwrap_or_default()
            };
            match section {
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
            }
        }
    };
    let label = format!("Docker / {}", section.label());
    let body = chrome(
        frame,
        &label,
        &status,
        d.searching,
        if d.searching {
            actions(true)
        } else {
            "Tab:area Shift+Tab:section"
        },
    );
    let rows: Vec<_> = d
        .rows()
        .iter()
        .map(|r| (r.name.clone(), r.state.clone(), r.status.clone()))
        .collect();
    let setup = d
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.setup.join("\n"));
    let text = d.selected_row().map_or_else(
        || {
            if !d.query.is_empty() {
                "No matches.\n/:search".into()
            } else if d.inventory.is_some() {
                format!("No {} found.", section.label().to_lowercase())
            } else {
                status.clone()
            }
        },
        |r| {
            format!(
                "{HEADING}{}\n\n{HEADING}Docker setup\n{}\nRead with docker container ls, image ls, volume ls and system df; nothing else is run.",
                r.detail,
                if setup.is_empty() { "No ~/.docker/config.json details" } else { &setup }
            )
        },
    );
    let header = match section {
        Section::Containers => ("Container", "State", "Image"),
        Section::Images => ("Image", "Size", "Used by"),
        Section::Volumes => ("Volume", "Driver", "Used by"),
    };
    let selected = d.selected[d.slot()].min(rows.len().saturating_sub(1));
    let query = d.query.clone();
    let searching = d.searching;
    let panes = panes(body);
    if panes[0].width > 0 {
        let block = pane(&format!(" {} ", section.label()), true);
        let inner = block.inner(panes[0]);
        frame.render_widget(block, panes[0]);
        let inner = filter_line(frame, inner, String::new(), &query, searching);
        let inner = if inner.height > 2 {
            let (a, b, c) = header;
            frame.render_widget(
                Paragraph::new(columns(a, b, c, inner.width)).style(muted()),
                Rect {
                    x: inner.x + 2,
                    width: inner.width.saturating_sub(2),
                    height: 1,
                    ..inner
                },
            );
            Rect {
                y: inner.y + 1,
                height: inner.height - 1,
                ..inner
            }
        } else {
            inner
        };
        let page = inner.height.max(1) as usize;
        app.docker.page = page;
        if rows.is_empty() {
            frame.render_widget(Paragraph::new(text.lines().next().unwrap_or("")), inner);
        } else {
            let start = (selected / page) * page;
            let items: Vec<_> = rows
                .iter()
                .skip(start)
                .take(page)
                .map(|(n, x, y)| ListItem::new(columns(n, x, y, inner.width)))
                .collect();
            let mut state = ListState::default().with_selected(Some(selected - start));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_symbol("> ")
                    .highlight_style(selection()),
                inner,
                &mut state,
            );
        }
    }
    if panes[1].width > 0 {
        scrolled(frame, panes[1], &text, &mut app.docker.panel);
    }
}
