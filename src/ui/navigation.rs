use super::*;
use crate::{
    app::Module,
    codex::state::View,
    presentation::{Detail, InspectorFocus, Page},
};

pub(super) fn switcher(frame: &mut Frame, app: &mut App) {
    let Some(s) = &app.presentation.switcher else {
        return;
    };
    let body = chrome(
        frame,
        "Sections",
        &format!(
            " Search{}: {}",
            if s.searching { "*" } else { "" },
            safe_text(&s.query)
        ),
        "Choose a destination; the previous context is retained.",
        s.searching,
        "Enter:open Esc:cancel",
    );
    let block = pane(" Sections · / search · arrows select ", true);
    let inner = block.inner(body);
    frame.render_widget(block, body);
    let rows = s.rows();
    let items: Vec<_> = rows.iter().map(|d| ListItem::new(d.label())).collect();
    let mut selected = ListState::default()
        .with_selected((!rows.is_empty()).then_some(s.selected.min(rows.len().saturating_sub(1))));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("> ")
            .highlight_style(selection()),
        inner,
        &mut selected,
    );
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No matching sections. Ctrl-U clears search; Esc returns."),
            inner,
        );
    }
}
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
    let status = match (app.module, app.codex.view) {
        (Module::Homebrew, _) => {
            if app.live {
                app.collection_message()
            } else {
                app.source_label.into()
            }
        }
        (_, View::Skills) => app.skills.status(),
        _ => app.codex.status(),
    };
    let body = chrome(
        frame,
        label,
        &format!(
            " Inspector / {}{}",
            detail.label(),
            if app.presentation.menu_searching {
                format!(" · Find section: {}", safe_text(&query))
            } else {
                String::new()
            }
        ),
        app.presentation.notice.as_deref().unwrap_or(&status),
        app.presentation.menu_searching,
        if app.presentation.menu_searching {
            "Enter:done Esc:done"
        } else if focus == InspectorFocus::Sections {
            "Enter:open Tab:content Esc:Back"
        } else if app.tree_content_focused()
            && app
                .explorer
                .rows
                .get(app.explorer.selected)
                .is_some_and(|row| row.target.is_some())
        {
            "Enter:inspect Tab:sections Esc:Back"
        } else {
            "Tab:sections Esc:Back"
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
        let block = pane(" Inspector sections ", focus == InspectorFocus::Sections);
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
    match (app.module, app.codex.view) {
        (Module::Homebrew, _) => {
            app.focus = rendered_focus;
            super::render_details(frame, app, content);
            app.focus = old_focus;
        }
        (_, View::Skills) => {
            app.skills.focus = rendered_focus;
            skills::render_details(frame, &mut app.skills, content, detail == Detail::Evidence);
            app.skills.focus = old_focus;
        }
        _ => {
            app.codex.focus_details = rendered_focus == Focus::Details;
            codex::render_details(frame, &mut app.codex, content, detail == Detail::Evidence);
            app.codex.focus_details = old_focus == Focus::Details;
        }
    }
}
pub(super) fn help(app: &App) -> String {
    let context = match (app.module, app.codex.view) {
        (Module::Homebrew, _) => {
            "Homebrew\nInspector sections: Overview, Dependencies, Required by, Paths, Diagnostics, Evidence.\nDependencies and Required by start as trees: [+] collapsed, [-] expanded, ! cycle.\nRight expands a branch, then enters its first child. Left collapses, then selects the parent.\nAt the top level Left stays in the tree. Space toggles a branch.\nEnter inspects a package; Esc restores the original tree, selection, scroll and filters.\ns selects recorded/current evidence; V selects a keg; t switches tree/direct view.\nTransitive and uncertain records remain labeled leaves; no direct links are inferred.\nDiagnostics: Enter drills down; Space shows full evidence; o shows original output; c runs the read-only check.\nf/i filter package type/intent in the inventory."
        }
        (_, View::Skills) => skills::HELP,
        _ => codex::HELP,
    };
    format!(
        "Den — Section → Resource → Details\n\ng opens Sections. / searches the switcher; arrows or j/k select; Enter opens; Esc cancels.\n/ searches the resource inventory. Enter finishes search, then Enter inspects the resource.\nThe inspector starts at Overview. Tab focuses its section list; arrows/j/k choose; Enter opens.\nTab switches inspector sections/content. / finds an inspector section.\nEsc returns to the previous context, including related resources and evidence.\nSearch, filters, selection, scroll and dependency expansion are restored when available.\nPage keys and Home/End navigate the focused list or text.\n? opens this context-aware help; Esc closes it. q/Ctrl-C quits.\n\n{context}\n\nCompatibility: F2/F3 and numbered sections still work. v opens Evidence; there are no display modes.\n\nRead-only inspection. Overview is concise; Evidence/Coverage retains provenance and modeling limits.\nNo inspected commands, scripts or MCP servers are executed. Withheld configuration values stay withheld.\nHistory is bounded to 64 contexts; removed resources are clamped after refresh."
    )
}
