//! The Problems overview (`!`): one list of what needs attention across every area,
//! problems first, then updates. Enter jumps to the entry.
use super::*;
use crate::problems::{Entry, Kind};

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let entries = crate::problems::collect(app);
    let problems = entries.iter().filter(|e| e.kind == Kind::Problem).count();
    let updates = entries.len() - problems;
    let mut status = format!(
        "{} · {}",
        crate::inventory::count(problems, "problem"),
        crate::inventory::count(updates, "update")
    );
    if let Some(reading) = still_reading(app) {
        status.push_str(&format!(" · still reading {reading}"));
    }
    if app.docker.context.is_some() && app.docker.inventory.is_none() {
        status.push_str(" · Docker: open it to check");
    }
    let body = chrome(
        frame,
        "Problems",
        &status,
        false,
        "Enter:go to it Esc:close",
    );
    let block = pane(" Problems ", true);
    let inner = block.inner(body);
    frame.render_widget(block, body);
    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new("Nothing needs attention in what Den has read.")
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let selected = app
        .problems
        .as_ref()
        .map_or(0, |v| v.selected)
        .min(entries.len() - 1);
    if let Some(view) = &mut app.problems {
        view.selected = selected;
    }
    app.page_size = inner.height.max(1) as usize;
    // A heading above each place, then its entries; only entries can be selected.
    let mut items = vec![];
    let mut selected_item = 0;
    let mut place = None;
    for (i, entry) in entries.iter().enumerate() {
        if place != Some(&entry.place) {
            place = Some(&entry.place);
            let heading_line = format!(
                "{}{}",
                if items.is_empty() { "" } else { "\n" },
                entry.place
            );
            items.push(ListItem::new(
                heading_line
                    .lines()
                    .map(|l| Line::styled(l.to_owned(), heading()))
                    .collect::<Vec<_>>(),
            ));
        }
        if i == selected {
            selected_item = items.len();
        }
        items.push(ListItem::new(row(entry, inner.width, i == selected)));
    }
    let mut state = ListState::default().with_selected(Some(selected_item));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("> ")
            .highlight_style(status_selection()),
        inner,
        &mut state,
    );
}

/// The name, then the detail with its status coloured, across the full width.
fn row(entry: &Entry, width: u16, selected: bool) -> Line<'static> {
    let width = width.saturating_sub(2) as usize;
    let name_width = (width / 3).clamp(8, 32);
    let tail = tail(entry);
    let mut spans = vec![Span::raw(fit(&entry.name, name_width)), Span::raw(" ")];
    spans.extend(status_spans(
        &fit(&tail, width.saturating_sub(name_width + 1)),
        Style::default(),
    ));
    if selected {
        for span in &mut spans {
            if !span.content.starts_with("Problem") && !span.content.starts_with('↑') {
                span.style = theme::theme().selected_text();
            }
        }
    }
    Line::from(spans)
}

/// "Problem · what is wrong", or the update notice ("↑ 2.0 available …").
fn tail(entry: &Entry) -> String {
    match entry.kind {
        Kind::Problem => format!("Problem · {}", entry.detail),
        Kind::Update => entry.detail.clone(),
    }
}

/// Areas still being read, so the list may grow.
fn still_reading(app: &App) -> Option<String> {
    let reading: Vec<&str> = [
        (
            "Agents",
            app.codex.running || app.claude.running || app.skills.running,
        ),
        ("Cloud", app.aws.running || app.kube.running),
        ("Toolchains", app.toolchains.running),
    ]
    .into_iter()
    .filter_map(|(area, running)| running.then_some(area))
    .collect();
    (!reading.is_empty()).then(|| reading.join(", "))
}
