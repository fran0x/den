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
            "Homebrew\nPackages start with Tools: packages Homebrew recorded as installed on request.\na switches Tools / All. Tools are bold; casks are [bracketed]. Counts follow search.\nRows show intent and \"required by N\": other installed packages with recorded runtime references.\nPackages without a request record appear only in All; Unknown stays Unknown and is never guessed.\nThe information panel shows Depends on and Required by, up to two levels; PARTIAL marks omissions.\nTransitive and uncertain records stay labeled leaves; no direct links are inferred.\nCurrent declarations do not count toward \"required by\"; no count does not mean unused.\nDiagnostics: [ selects a report (or 5-8); Enter drills down; Space shows full evidence; o shows the original report; c runs the read-only check.\nA package opened from Diagnostics is selected in Packages; Esc returns."
        }
    };
    format!(
        "Den — Area › Section › Inventory\n\nTab switches Homebrew / Agents / Cloud / Docker / Toolchains; each area keeps its place.\nShift+Tab switches sections: Homebrew Packages / Diagnostics; Agents Skills / Instructions / Settings / MCP / Hooks / Plugins; Cloud AWS / Kubernetes; Docker Containers / Images / Volumes; Toolchains On PATH / Rust / Packages.\nThe inventory keeps focus; the information panel follows the selection and shows everything about it. There is no inspector: PgUp/PgDn scroll the panel.\n/ searches the inventory. Enter or Esc finishes editing and keeps the query.\nIn Diagnostics, [ selects a report and ] its content.\nEsc goes back within the current section. Switching areas or sections never adds a Back step.\nHome/End move to the first or last row. ? opens this help; Esc closes it. q or Ctrl-C quits.\n\n{context}\n\nShortcuts: F2 = Tab, F3 = Shift+Tab.\n\nRead-only inspection. No inspected commands, scripts or MCP servers are executed. Withheld configuration values stay withheld.\nEach section keeps up to 64 Back contexts; removed resources are clamped after refresh."
    )
}
