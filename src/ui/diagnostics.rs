use super::*;
use crate::diagnostics::{Check, Tab, describe};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let tab = app.diagnostics.tab.unwrap_or(Tab::Unresolved);
    let title = format!(" Diagnostics / {} [5-8] ", tab.label());
    let block = pane(&title, app.focus == Focus::Details);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let state = tab.check().map(|c| &app.checks[c.index()]);
    let report = state.and_then(|s| s.report.as_ref());
    let status = if !app.live {
        "Not checked: live commands unavailable in snapshot/demo mode.".into()
    } else if state.is_some_and(|s| s.running) {
        "Checking Homebrew... previous report retained below; q cancels on exit.".into()
    } else if app.checks.iter().any(|s| s.running) {
        "Another Homebrew check is running; wait before starting this check.".into()
    } else if matches!(
        app.collection_status,
        crate::app::CollectionStatus::Loading { .. }
    ) && report.is_none()
    {
        "Not checked: wait for inventory collection to finish.".into()
    } else if let Some(report) = report {
        let age = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(report.checked_at);
        let stale = report.epoch != app.inventory_epoch
            || matches!(
                app.collection_status,
                crate::app::CollectionStatus::Failed { .. }
                    | crate::app::CollectionStatus::Loading { .. }
            );
        format!(
            "{}{} · checked {age}s ago (Unix UTC {}) · exit {:?}\n{}",
            if stale {
                "STALE / inventory changed or uncertain: "
            } else {
                ""
            },
            report.status.label(),
            report.checked_at,
            report.code,
            report.message
        )
    } else {
        "Not checked. c: run on demand (read-only).".into()
    };
    let missing = app.checks[Check::Missing.index()]
        .report
        .as_ref()
        .filter(|r| r.epoch == app.inventory_epoch)
        .and_then(|r| r.count)
        .map_or("Not checked / unavailable".into(), |n| n.to_string());
    let header = if tab == Tab::Unresolved {
        format!(
            "{}{} unresolved references · {} target-keg gaps\nMissing references confirmed by Homebrew: {missing}. Enter: drill down; Space: explanation.",
            if app.health.partial {
                "PARTIAL GRAPH: "
            } else {
                ""
            },
            app.health.unresolved,
            app.health.keg_gaps
        )
    } else {
        status.clone()
    };
    let header = if app.health.partial && tab != Tab::Unresolved {
        format!("PARTIAL GRAPH: observations incomplete.\n{header}")
    } else {
        header
    };
    let items = app.diagnostics.items(&app.health, &app.checks);
    app.diagnostics.selected = app.diagnostics.selected.min(items.len().saturating_sub(1));
    let detail = items.get(app.diagnostics.selected).map(|&item| describe(item, &app.diagnostics, &app.health, &app.inventory, &app.graph, &app.checks, app.inventory_epoch).1).unwrap_or_else(|| {
        if tab == Tab::Unresolved { "Informational: no unresolved references in the available graph. This is not a complete installation check.".into() } else { status.clone() }
    });
    if app.diagnostics.raw || app.diagnostics.expanded || items.is_empty() {
        let content = if app.diagnostics.raw {
            match report {
                Some(r) => format!(
                    "{status}\n\nOriginal Homebrew stdout (sanitized):\n{}\n\nOriginal Homebrew stderr (sanitized):\n{}\n\nHomebrew output is authoritative; structured parsing may be unavailable. No removals are performed.",
                    r.stdout, r.stderr
                ),
                None => format!("{status}\nNo Homebrew report available."),
            }
        } else if items.is_empty() && tab != Tab::Unresolved {
            format!(
                "{header}\n\nc: run check · o: original output · Backspace: return\nNo removals or repairs are performed."
            )
        } else {
            format!("{header}\n\n{detail}\n\nSpace/o/Backspace: return · c: check")
        };
        let mut lines: Vec<_> = content
            .split('\n')
            .take(60000)
            .map(|s| Line::from(safe_text(s)))
            .collect();
        if content.split('\n').count() > 60000 {
            lines.push(Line::from("PARTIAL DISPLAY: report exceeds 60000 lines."));
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        app.detail_max = scroll_max(&paragraph, inner);
        app.diagnostics.scroll = app.diagnostics.scroll.min(app.detail_max);
        app.page_size = inner.height.max(1) as usize;
        frame.render_widget(paragraph.scroll((app.diagnostics.scroll, 0)), inner);
        return;
    }
    let parts = Layout::vertical([
        Constraint::Length(if inner.height >= 7 { 3 } else { 1 }),
        Constraint::Min(1),
        Constraint::Length(if inner.height >= 7 { 3 } else { 0 }),
    ])
    .split(inner);
    let header_lines: Vec<_> = header
        .split('\n')
        .map(|s| Line::from(safe_text(s)))
        .collect();
    frame.render_widget(
        Paragraph::new(header_lines).wrap(Wrap { trim: false }),
        parts[0],
    );
    let page = parts[1].height.max(1) as usize;
    app.page_size = page;
    let start = (app.diagnostics.selected / page) * page;
    // Render only the visible page; all bounded graph findings remain navigable.
    let rows: Vec<_> = items
        .iter()
        .skip(start)
        .take(page)
        .map(|&item| {
            let (label, _) = describe(
                item,
                &app.diagnostics,
                &app.health,
                &app.inventory,
                &app.graph,
                &app.checks,
                app.inventory_epoch,
            );
            ListItem::new(safe_text(&label))
        })
        .collect();
    let list = List::new(rows)
        .highlight_symbol("> ")
        .highlight_style(selection());
    let mut selection = ListState::default().with_selected(Some(app.diagnostics.selected - start));
    frame.render_stateful_widget(list, parts[1], &mut selection);
    let footer = format!(
        "{}/{} · Space: full evidence · Enter: open · o: report\n{detail}",
        app.diagnostics.selected + 1,
        items.len()
    );
    frame.render_widget(
        Paragraph::new(
            footer
                .split('\n')
                .map(|s| Line::from(safe_text(s)))
                .collect::<Vec<_>>(),
        )
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::Reset)),
        parts[2],
    );
}
