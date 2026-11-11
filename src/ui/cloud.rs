//! The Cloud area: AWS profiles and SSO sessions, with the same layout as Agents.
use super::*;

pub(super) const HELP: &str = "Cloud › AWS: profiles from ~/.aws/config and ~/.aws/credentials\n\
One row per profile, then each SSO session. Auth says how the profile gets credentials: Role, SSO, Process, Static keys or None set. Status is Ready (with the region) or Problem (with what is missing).\n\
The panel shows the region, account and role, the role chain and where the profile is defined. AWS_PROFILE marks the active profile.\n\
Privacy: from the credentials file only profile and key names are kept; values are discarded as they are read. SSO start URLs, endpoints, external IDs and credential_process commands are withheld. SSO and CLI caches are never read. A credentials file other users can read is a Problem.\n\
Nothing is called: no AWS API, SSO login or credential process, so whether credentials work is not checked.\n\
/ searches; PgUp/PgDn scroll the panel; r refreshes.";

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    let a = &app.aws;
    let status = match (&a.error, &a.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if a.context.is_none() => "AWS not configured in this mode".into(),
        (None, None) => "Collecting AWS profiles...".into(),
        (None, Some(i)) => {
            let profiles = i
                .rows
                .iter()
                .filter(|r| r.kind == crate::aws::Kind::Profile)
                .count();
            let problems = i
                .rows
                .iter()
                .filter(|r| r.status.starts_with("Problem"))
                .count();
            format!(
                "{} · {} · {} · active: {}",
                crate::inventory::count(profiles, "profile"),
                crate::inventory::count(i.sso_sessions, "SSO session"),
                crate::inventory::count(problems, "problem"),
                i.active.as_deref().map_or_else(
                    || "default (AWS_PROFILE not set)".into(),
                    |p| format!("{} (AWS_PROFILE)", safe_text(p))
                )
            )
        }
    };
    let body = chrome(
        frame,
        "Cloud / AWS",
        &status,
        a.searching,
        if a.searching {
            actions(true)
        } else {
            "Tab:area"
        },
    );
    let rows: Vec<_> = a
        .rows()
        .iter()
        .map(|r| (r.name.clone(), r.auth.clone(), r.status.clone()))
        .collect();
    let text = a.selected_row().map_or_else(
        || {
            if !a.query.is_empty() {
                "No matches.\n/:search".into()
            } else if let Some(i) = &a.inventory {
                format!(
                    "No AWS profiles found.\n\n{HEADING}Coverage\n{}",
                    i.coverage.join("\n")
                )
            } else {
                status.clone()
            }
        },
        |r| {
            let coverage = a
                .inventory
                .as_ref()
                .map_or(String::new(), |i| i.coverage.join("\n"));
            format!("{HEADING}{}\n\n{HEADING}Coverage\n{coverage}", r.detail)
        },
    );
    let selected = a.selected.min(rows.len().saturating_sub(1));
    let query = a.query.clone();
    let searching = a.searching;
    let panes = panes(body);
    if panes[0].width > 0 {
        let block = pane(" AWS ", true);
        let inner = block.inner(panes[0]);
        frame.render_widget(block, panes[0]);
        let inner = filter_line(frame, inner, String::new(), &query, searching);
        let inner = if inner.height > 2 {
            frame.render_widget(
                Paragraph::new(columns("Profile", "Auth", "Status", inner.width)).style(muted()),
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
        app.aws.page = page;
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
        scrolled(frame, panes[1], &text, &mut app.aws.panel);
    }
}
