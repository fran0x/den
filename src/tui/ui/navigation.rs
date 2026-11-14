use super::*;
use crate::{
    app::Module,
    tui::presentation::{InspectorFocus, Page},
};

pub(super) fn inspector(frame: &mut Frame, app: &mut App) {
    let Page::Inspector {
        detail,
        focus,
        selected,
        query,
    } = app.presentation.page.clone()
    else {
        return;
    };
    let label = app.destination().label();
    // Only Homebrew Diagnostics uses this page; Codex and Claude show a scrolling panel.
    let status = if app.live {
        app.collection_message()
    } else {
        app.source_label.into()
    };
    // The status line names the open section: narrow terminals show one pane at a time.
    let line = if app.presentation.menu_searching {
        format!("Find section: {}_", safe_text(&query))
    } else {
        format!(
            "{} · {}",
            detail.label(),
            app.presentation.notice.as_deref().unwrap_or(&status)
        )
    };
    let body = chrome(
        frame,
        label,
        &line,
        app.presentation.menu_searching,
        if app.presentation.menu_searching {
            "Enter:done Esc:done"
        } else if focus == InspectorFocus::Sections {
            "Enter:open ]:content Tab:area Shift+Tab:section"
        } else {
            "[:reports Tab:area Shift+Tab:section"
        },
    );
    let (menu, content) = if body.width >= 100 {
        let panes = Layout::horizontal([
            Constraint::Length(27),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(body);
        (panes[0], reading_area(panes[2]))
    } else if focus == InspectorFocus::Sections {
        (body, Rect::default())
    } else {
        (Rect::default(), reading_area(body))
    };
    if menu.width > 0 {
        let block = pane(" Reports ", focus == InspectorFocus::Sections);
        let inner = block.inner(menu);
        frame.render_widget(block, menu);
        let rows = app.detail_rows();
        let items: Vec<_> = rows
            .iter()
            .map(|d| {
                ListItem::new(fit(
                    &format!("{}{}", if *d == detail { "• " } else { "  " }, d.label()),
                    inner.width.saturating_sub(2) as usize,
                ))
            })
            .collect();
        let mut selection_state = ListState::default().with_selected(
            (!rows.is_empty()).then_some(selected.min(rows.len().saturating_sub(1))),
        );
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol(if focus == InspectorFocus::Sections {
                    "> "
                } else {
                    "  "
                })
                .highlight_style(if focus == InspectorFocus::Sections {
                    selection()
                } else {
                    Style::default()
                }),
            inner,
            &mut selection_state,
        );
        if rows.is_empty() {
            frame.render_widget(
                Paragraph::new("No matching sections. / then Ctrl-U clears search."),
                inner,
            );
        }
    }
    if content.width == 0 {
        return;
    }
    let old_focus = app.active_focus();
    // Render the inactive pane without a second focus marker; the underlying controller
    // remains focused on inspector content when handling native detail operations.
    let rendered_focus = if focus == InspectorFocus::Content {
        Focus::Details
    } else {
        Focus::Inventory
    };
    app.focus = rendered_focus;
    super::render_details(frame, app, content);
    app.focus = old_focus;
}
pub(super) fn help(app: &App) -> String {
    let context = match app.module {
        Module::Agents => super::agents::HELP,
        Module::Cloud => match app.cloud {
            crate::app::CloudSection::Aws => super::cloud::HELP,
            crate::app::CloudSection::Kubernetes => super::cloud::KUBE_HELP,
        },
        Module::Docker => super::docker::HELP,
        Module::Toolchains => super::toolchains::HELP,
        Module::Homebrew => {
            "Homebrew\n\
Packages starts with your Tools: what you installed on purpose, plus casks nothing else needs (Homebrew keeps no request record for casks). a shows All packages, dependencies included; your tools stay bold. Casks are [bracketed] and in their own colour.\n\
Each row says whether you asked for it or it came in as a dependency, and how many other packages need it (required by N). A yellow ↑ marks a newer version, from your last brew update.\n\
The panel shows what the package depends on and what it is required by. Incomplete records stay Unknown: Den never guesses, and no dependents does not mean safe to remove.\n\
In dew, x opens actions: upgrade or uninstall this package, update Homebrew's data, autoremove, cleanup. Choose with ↑↓; Enter runs it after its dry run, and uninstall is always last. den has no actions.\n\
Diagnostics (Shift+Tab): [ or 5-8 picks a report, ] returns to its content, Enter opens a group or jumps to a package (Esc comes back), Space shows the full finding, o Homebrew's original report, c runs the check now (read-only)."
        }
    };
    format!(
        "Den\n\n\
Tab        next area: Homebrew, Agents, Cloud, Docker, Toolchains\n\
Shift+Tab  next section in this area\n\
↑ ↓ j k    move through the list; Home and End jump to the first or last row\n\
PgUp PgDn  scroll the panel\n\
/          search; Enter or Esc finishes and keeps the search\n\
Esc        go back\n\
r          refresh\n\
d          only what changed since Den started; d again shows every row\n\
q          quit (Ctrl-C works anywhere)\n\n\
Every area and section remembers where you were.\n\n\
What changed: after a refresh, + marks a new row and ~ a changed one (the panel says what it was before). d lists only those, then the removed rows, struck through. Den compares each area with how it first read it this session, in memory: nothing is saved, and quitting forgets it. Running or stopped containers and newer Homebrew versions are not changes.\n\n\
{context}\n\n\
F2 is Tab and F3 is Shift+Tab. Esc closes this help.\n\
Den only looks: it starts no commands, scripts or servers it finds, and withheld values stay withheld. Changes happen only in dew, each one chosen and confirmed with Enter."
    )
}
