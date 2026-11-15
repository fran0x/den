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
/// Where each kind of package's newer version comes from, and what is never checked.
/// Keep in step with `online::Registry` and docs/GUIDE.md.
const UPDATES: &str = "\
Homebrew formulas, casks   formulae.brew.sh; without u, Homebrew's data (last brew update)\n\
Codex, Claude Code         Homebrew, when it installed them; Codex also checks itself\n\
cargo install crates       index.crates.io\n\
npm and bun packages       registry.npmjs.org\n\
uv tools, pipx packages    pypi.org\n\
Rust stable and 1.xx       endoflife.date\n\
node, python3, ruby, go    endoflife.date; Homebrew's ↑ when Homebrew installed them\n\
Docker images              Docker Hub and ghcr.io, for names with a namespace or registry\n\
Skills from npx skills     api.github.com: whether the folder changed since install\n\
Not checked: other Homebrew taps, git and path crates, go install programs, uv Pythons, java, pinned, beta and nightly Rust, single-name images such as postgres, other skills, AWS, Kubernetes, MCP servers and plugins. No ↑ there does not mean up to date.\n\
Only names are sent; installed versions stay here. Answers are kept with the time of the check, and only u checks again.";
/// Everything dew can change, by area: `x` on a row offers the ones that apply to it.
const DEW_ACTIONS: &str = "What dew can change (x on a row, Enter runs the selected action):\n\
Homebrew     upgrade or uninstall a package; update Homebrew's data; remove dependencies nothing needs; clean up old versions\n\
Docker       stop a running container; remove a stopped container, or an image or volume nothing uses; pull the newer build of a tag the online check (u) found\n\
Toolchains   Rust: update or uninstall a toolchain, upgrade or uninstall a cargo install crate. Packages: upgrade or uninstall a global npm package, uv tool or pipx package. PATH: Homebrew's actions for a command it installed\n\
Agents, Cloud  nothing: read-only in dew too\n\
Each action shows its exact command first, and a dry run where the tool has one. Nothing in use is removed, and there is never --force, prune, sudo or a shell. Results go to ~/.local/state/den/actions.log.";
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
Each row says whether you asked for it or it came in as a dependency, and how many other packages need it (required by N). A yellow ↑ marks a newer version, from your last brew update or, once you press u, from formulae.brew.sh.\n\
The panel shows what the package depends on and what it is required by. Incomplete records stay Unknown: Den never guesses, and no dependents does not mean safe to remove.\n\
In dew, x opens actions: upgrade or uninstall this package, update Homebrew's data, autoremove, cleanup. Enter runs the chosen one after its dry run, and uninstall is always last.\n\
Diagnostics (Shift+Tab): reports on unresolved links, missing dependencies, leaves and what cleanup would remove. Enter opens a group or jumps to its package, Space shows the full finding, o shows Homebrew's own report, and c runs the check again (read-only)."
        }
    };
    let changes = if app.actions.allowed() {
        DEW_ACTIONS
    } else {
        "den changes nothing. dew is the same with actions on: x on a row shows what it can change there."
    };
    format!(
        "Den\n\n\
Keys: the bottom line lists the keys that work where you are, in three groups: this view's own │ Den's │ moving around. Also: ↑↓ or j k move, Home and End jump to the first or last row, Esc goes back, F2 and F3 are Tab and Shift+Tab, and Ctrl-C quits anywhere. Every area and section remembers where you were.\n\n\
What changed (d): + marks a new row and ~ a changed one (the panel says what it was before); d lists only those, then the removed rows, struck through. Den compares each area with how it first read it this session, and with the snapshot S saved, if any. Running or stopped containers and newer versions are not changes.\n\n\
Where updates come from (u, after asking; Esc stops):\n\
{UPDATES}\n\n\
{context}\n\n\
{changes}\n\n\
Den only looks: it starts no commands, scripts or servers it finds, and withheld values stay withheld. den writes only its own files, each after you confirm: the snapshot (S) and the online answers (u), in ~/.local/state/den, private to you."
    )
}
