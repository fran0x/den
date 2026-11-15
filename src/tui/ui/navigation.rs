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
            "Enter:open ]:content 5-8:report"
        } else {
            "[:reports Enter:open Space:finding o:report c:check now"
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
/// Where `u` looks for each kind of package. Keep in step with `online::Registry` and
/// docs/GUIDE.md.
const UPDATES: &str = "\
Updates (u)\n\
Homebrew          formulae.brew.sh\n\
cargo crates      index.crates.io\n\
npm, bun          registry.npmjs.org\n\
uv, pipx          pypi.org\n\
Rust, runtimes    endoflife.date\n\
Docker images     Docker Hub, ghcr.io (names with a namespace)\n\
npx skills        api.github.com\n\
Not checked       go install, java, uv Pythons, beta and nightly Rust, other registries\n\
Only names are sent.";
/// What dew can change, by area.
const DEW_ACTIONS: &str = "\
dew (x)\n\
Homebrew          upgrade, uninstall, update data, autoremove, cleanup\n\
Docker            stop, remove unused, pull a newer build\n\
Toolchains        update, upgrade, uninstall\n\
Exact command and dry run first. Never --force, prune, sudo or a shell.";
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
Tools: what you asked for, and casks nothing needs. a: all packages.\n\
Unknown: incomplete record. Den never guesses.\n\
Diagnostics: unresolved links, missing dependencies, leaves, cleanup."
        }
    };
    let changes = if app.actions.allowed() {
        DEW_ACTIONS
    } else {
        "den changes nothing; dew does, after x and Enter."
    };
    format!(
        "Keys: bottom line: this area │ all areas (S, u) │ moving around.\n\
Also j k, Home End, Esc back, Ctrl-C quit.\n\
S and u cover the areas read so far; (Docker) in their question means not read yet: open its tab to include it.\n\
Marks: + new · ~ changed · ↑ newer · End of life. d lists changes.\n\n\
{context}\n\n\
{UPDATES}\n\n\
{changes}\n\n\
Den runs nothing it finds and shows no secrets."
    )
}
