mod common;
mod navigation;
use common::*;
mod codex;
mod diagnostics;
mod skills;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{App, Focus},
    explorer::Section,
    inventory::{HomebrewMetadata, installation_intent, safe_text},
};

pub const HELP: &str = "Den — navigation\n\n\
F2: Homebrew / Codex · F3: next view; each view retains its state.\nEnter from list: full-screen inspector · Esc: Back\nv: Compact / Detailed · z: full-screen toggle from either pane\n\n\
Browse read-only Homebrew inventory, or offline snapshots/demo data.\n\n\
Up/Down or k/j   Move selection / scroll focused pane\n\
PageUp/PageDown  Move a page\n\
Home/End        First / last item or detail line\n\
Tab/Shift-Tab   Switch pane focus\n\
Enter           Inspect selected resource (full screen)\nRight           Focus details\n\
Left            Focus inventory\n\
/               Edit search (live, case insensitive)\n\
Enter/Esc       Finish search, retaining the query\n\
Backspace       Delete last search character\n\
Ctrl-U          Clear query while editing\n\
f               Cycle kind filter; wraps back to All\n\
i               Cycle Homebrew intent: Requested/Dependency/Unknown/All\n\
r               Refresh live Homebrew inventory (snapshots stay offline)\n\
5/6/7/8         Diagnostics: Unresolved / Missing / Leaves / Cleanup\n\
c (diagnostics) Run the selected Homebrew check (live only)\n\
o (diagnostics) Show original sanitized output; Space: full finding\n\
1/2/3/4         Overview / Dependencies / Required by / Dependency paths\n\
s (details)     Switch recorded runtime / current declaration evidence\n\
V (details)     Cycle installed keg (linked keg is preferred initially)\n\
t (relations)   Direct list / all evidence with expandable direct-edge tree\n\
Space           Expand/collapse selected tree branch\n\
Enter           Open selected related package overview\n\
Backspace       Back to previous package and its search/filter context\n\
Esc             Clear query and all filters when browsing\n\
? / F1          Open help (F1 also works during search)\n\
Esc / ? / F1    Close help\n\
q / Ctrl-C      Quit (q is text while editing search)\n\n\
Search matches all words across name, kind, ID, version, path, description, origin and reason.\n\n\
Below 100 columns Tab switches between full-width panes. Minimum useful size: 30 x 10.\n\n\
Trust: unknown. No security checks or package changes. Den invokes brew info once per collection; Homebrew may fetch metadata. Snapshot claims are unverified.\n\n\
Cask versions are Homebrew-recorded versions; applications may self-update. Mixed or insufficient installation-intent records are Unknown.\n\n\
Relationships never change installation intent. Recorded transitive/unknown-directness entries are leaves, not inferred direct edges. Current optional/build/test declarations are labeled and excluded from runtime traversal.\n\n\
Paths show one shortest known direct-edge path per requested installation, within the selected evidence view. Missing versions, ambiguous references and missing metadata can prevent a path. No path does not imply unused or safe to remove.\n\n\
Exploration limits: 100000 edges, 400 visible rows, depth 32, 10000 traversal steps, 20 requested paths and 32 back-history entries. Partial results are labeled.\n\n\
Scroll this help with arrows, j/k, PageUp/PageDown or Home/End.";

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Den: resize to 30 x 10\nq / Ctrl-C: quit"),
            area,
        );
        return;
    }
    if app.presentation.switcher.is_some() && !app.help {
        navigation::switcher(frame, app);
        return;
    }
    if app.help {
        let body = chrome(
            frame,
            "Help",
            "Navigation and evidence · scroll for all controls",
            "Read-only · nothing inspected is executed",
            false,
            "Esc:Back arrows:scroll",
        );
        let block = pane(" Help · Esc:Back ", true);
        let inner = block.inner(reading_area(body));
        let text = navigation::help(app);
        let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
        app.help_max = scroll_max(&paragraph, inner);
        app.help_scroll = app.help_scroll.min(app.help_max);
        frame.render_widget(block, reading_area(body));
        frame.render_widget(paragraph.scroll((app.help_scroll, 0)), inner);
        return;
    }
    if app.inspecting() {
        navigation::inspector(frame, app);
        return;
    }

    if app.module == crate::app::Module::Skills {
        if app.codex.view == crate::codex::state::View::Skills {
            skills::render(frame, &mut app.skills);
        } else {
            codex::render(frame, &mut app.codex);
        }
        return;
    }
    let view = app.diagnostics.tab.map_or_else(
        || format!("Homebrew / {}", app.explorer.section.label()),
        |t| format!("Homebrew / Diagnostics / {}", t.label()),
    );
    let query = format!(
        " /{}{}   Type:{}  Intent:{}",
        if app.searching { " " } else { " Search: " },
        safe_text(&app.query),
        app.filter.map_or("All", |k| k.label()),
        app.intent_filter.map_or("All", |i| i.label())
    );
    let status = if app.live {
        app.collection_message()
    } else {
        format!(
            "{}/{} resources · {}",
            app.visible.len(),
            app.inventory.resources.len(),
            app.source_label
        )
    };
    let body = chrome(
        frame,
        &view,
        &query,
        &status,
        app.searching,
        &actions(
            app.focus,
            false,
            if app.diagnostics.tab.is_some() {
                if app.diagnostics.raw || app.diagnostics.expanded {
                    "Backspace:up"
                } else {
                    "Enter:open"
                }
            } else if app.explorer.section != Section::Overview {
                "Enter:follow"
            } else {
                ""
            },
        ),
    );
    let panes = panes(body, app.focus, false);
    if panes[0].width > 0 {
        render_inventory(frame, app, panes[0]);
    }
    if panes[1].width > 0 {
        render_details(frame, app, panes[1]);
    }
}

fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::TOP)
        .title(format!("{}{}", if focused { ">" } else { " " }, title))
        .border_style(if focused {
            Style::default().fg(ACCENT)
        } else {
            muted()
        })
}

fn render_inventory(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = pane(" Inventory ", app.focus == Focus::Inventory);
    let inner = block.inner(area);
    if app.focus == Focus::Inventory {
        app.page_size = inner.height.max(1) as usize;
    }
    frame.render_widget(block, area);
    if app.visible.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.live && app.inventory.resources.is_empty() {
                app.collection_message()
            } else if app.is_homebrew() && app.inventory.resources.is_empty() {
                "No installed Homebrew packages in snapshot".into()
            } else {
                "No matches. Esc resets filters.".into()
            })
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let items: Vec<_> = app
        .visible
        .iter()
        .map(|&index| {
            let r = &app.inventory.resources[index];
            ListItem::new(columns(
                &format!(
                    "{}{}",
                    r.name,
                    if matches!(&r.homebrew, Some(HomebrewMetadata::Cask { .. })) {
                        " [Cask]"
                    } else {
                        ""
                    }
                ),
                r.version.as_deref().unwrap_or("Unknown"),
                &r.description,
                inner.width,
            ))
        })
        .collect();
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(selection());
    let mut state = ListState::default()
        .with_selected(app.selected)
        .with_offset(app.list_offset);
    frame.render_stateful_widget(list, inner, &mut state);
    app.list_offset = state.offset();
}

fn render_details(frame: &mut Frame, app: &mut App, area: Rect) {
    if app.diagnostics.tab.is_some() {
        diagnostics::render(frame, app, area);
        return;
    }
    if app.is_homebrew() && app.explorer.section != Section::Overview {
        render_relationships(frame, app, area);
        return;
    }
    let title = if app.evidence() {
        " Evidence "
    } else if app.is_homebrew() {
        " Details / 1 Overview "
    } else {
        " Details "
    };
    let block = pane(title, app.focus == Focus::Details);
    let inner = block.inner(area);
    if app.focus == Focus::Details {
        app.page_size = inner.height.max(1) as usize;
    }
    let text = if !app.evidence() {
        overview(app)
            .lines()
            .map(|s| Line::from(safe_text(s)))
            .collect()
    } else if let Some(r) = app.resource() {
        let mut lines = vec![
            Line::styled(
                safe_text(&r.name),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Line::from(format!("Kind: {}", r.kind.label())),
            Line::from(format!(
                "{}: {}",
                if matches!(&r.homebrew, Some(HomebrewMetadata::Cask { .. })) {
                    "Homebrew-recorded version"
                } else {
                    "Installed version(s)"
                },
                safe_text(r.version.as_deref().unwrap_or("Unknown"))
            )),
            Line::from(format!("ID: {}", safe_text(&r.id))),
            Line::from(format!(
                "Path: {}",
                safe_text(r.path.as_deref().unwrap_or("Unknown"))
            )),
            Line::from(""),
            Line::from("What's installed?"),
        ];
        if let Some(metadata) = &r.homebrew {
            lines.extend(homebrew_details(metadata));
        }
        if r.description.is_empty() {
            lines.push(Line::from("Description: Unknown"));
        }
        lines.extend(
            r.description
                .split('\n')
                .map(|line| Line::from(safe_text(line))),
        );
        lines.extend([
            Line::from(""),
            Line::from("Why is it here?"),
            Line::from(safe_text(&r.reason)),
            Line::from(format!("Origin: {}", safe_text(&r.origin))),
            Line::from(""),
            Line::from("Can I trust it?"),
            Line::from("Unknown - not assessed. Snapshot claims are unverified."),
        ]);
        lines
    } else {
        vec![Line::from("Select a resource to inspect its details.")]
    };
    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    app.detail_max = scroll_max(&paragraph, inner);
    app.detail_scroll = app.detail_scroll.min(app.detail_max);
    frame.render_widget(block, area);
    frame.render_widget(paragraph.scroll((app.detail_scroll, 0)), inner);
}

fn homebrew_details(metadata: &HomebrewMetadata) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(format!(
        "Installation intent: {}",
        metadata.intent().label()
    ))];
    let mut field = |label: &str, value: Option<&str>| {
        lines.push(Line::from(format!(
            "{label}: {}",
            safe_text(value.filter(|s| !s.is_empty()).unwrap_or("Unknown"))
        )));
    };
    match metadata {
        HomebrewMetadata::Formula {
            full_name,
            tap,
            homepage,
            linked_keg,
            installed,
            dependencies,
            ..
        } => {
            field("Full name", full_name.as_deref());
            field("Tap", tap.as_deref());
            field("Homepage", homepage.as_deref());
            field("Linked keg", linked_keg.as_deref());
            for keg in installed {
                lines.push(Line::from(format!(
                    "Keg {}: {}",
                    safe_text(&keg.version),
                    installation_intent(keg.installed_on_request, keg.installed_as_dependency)
                        .label()
                )));
                lines.push(Line::from(format!(
                    "  on_request: {}; as_dependency: {}",
                    flag(keg.installed_on_request),
                    flag(keg.installed_as_dependency)
                )));
                lines.push(Line::from(format!(
                    "  Recorded runtime dependencies: {}",
                    keg.runtime_dependencies
                        .as_ref()
                        .map_or("Unknown".into(), |d| d.len().to_string())
                )));
            }
            lines.push(Line::from(format!(
                "Declared dependency metadata: {} retained fields (2: explore)",
                dependencies.len()
            )));
        }
        HomebrewMetadata::Cask {
            full_token,
            display_names,
            tap,
            homepage,
            installed_on_request,
            installed_as_dependency,
            ..
        } => {
            field("Full token", full_token.as_deref());
            field(
                "Display name",
                (!display_names.is_empty())
                    .then(|| display_names.join(", "))
                    .as_deref(),
            );
            field("Tap", tap.as_deref());
            field("Homepage", homepage.as_deref());
            lines.push(Line::from(format!(
                "on_request: {}; as_dependency: {}",
                flag(*installed_on_request),
                flag(*installed_as_dependency)
            )));
            lines.push(Line::from(
                "Casks may self-update; the recorded version may differ from the application.",
            ));
        }
    }
    lines.push(Line::from(""));
    lines
}

fn render_relationships(frame: &mut Frame, app: &mut App, area: Rect) {
    let title = format!(
        " Details / {} [{}] ",
        app.explorer.section.label(),
        if app.explorer.tree {
            "all / tree"
        } else {
            "direct"
        }
    );
    let block = pane(&title, app.focus == Focus::Details);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let note = format!(
        "{}{}",
        if app.explorer.limited {
            "PARTIAL RESULTS: "
        } else {
            ""
        },
        safe_text(&app.explorer.note)
    );
    if app.explorer.rows.is_empty() {
        let paragraph = Paragraph::new(note).wrap(Wrap { trim: false });
        app.detail_max = scroll_max(&paragraph, inner);
        app.detail_scroll = app.detail_scroll.min(app.detail_max);
        frame.render_widget(paragraph.scroll((app.detail_scroll, 0)), inner);
        return;
    }
    let detail = app
        .explorer
        .rows
        .get(app.explorer.selected)
        .map(|r| safe_text(&r.detail))
        .unwrap_or_default();
    let footer = Paragraph::new(detail)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::Reset));
    let footer_height = if inner.height >= 6 {
        footer
            .line_count(inner.width)
            .min(6)
            .min((inner.height / 2) as usize) as u16
    } else {
        0
    };
    let header_height = if inner.height >= 4 { 2 } else { 0 };
    let rows = Layout::vertical([
        Constraint::Length(header_height),
        Constraint::Min(1),
        Constraint::Length(footer_height),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(note).wrap(Wrap { trim: true }), rows[0]);
    if app.focus == Focus::Details {
        app.page_size = rows[1].height.max(1) as usize;
    }
    let items: Vec<_> = app
        .explorer
        .rows
        .iter()
        .map(|row| ListItem::new(safe_text(&row.label)))
        .collect();
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(selection());
    let mut state = ListState::default()
        .with_selected(Some(app.explorer.selected))
        .with_offset(app.explorer.offset);
    frame.render_stateful_widget(list, rows[1], &mut state);
    app.explorer.offset = state.offset();
    frame.render_widget(footer, rows[2]);
}

fn flag(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "Unknown",
    }
}

fn scroll_max(paragraph: &Paragraph<'_>, area: Rect) -> u16 {
    if area.width == 0 || area.height == 0 {
        return 0;
    }
    paragraph
        .line_count(area.width)
        .saturating_sub(area.height as usize)
        .min(u16::MAX as usize) as u16
}

fn overview(app: &App) -> String {
    let Some(r) = app.resource() else {
        return "Select a resource to inspect its details.".into();
    };
    let intent = r
        .homebrew
        .as_ref()
        .map_or("Unknown", |m| m.intent().label());
    let version_label = if matches!(&r.homebrew, Some(HomebrewMetadata::Cask { .. })) {
        "Homebrew-recorded version"
    } else {
        "Installed"
    };
    let node = app.selected_node().unwrap_or(0);
    let edges = app.graph.forward.get(node).map_or(0, Vec::len);
    let reverse = app.graph.reverse.get(node).map_or(0, Vec::len);
    format!(
        "{}\n\n{}\n\n{} · {}: {}\nInstallation intent: {}\n\nRelationships: {} outgoing / {} incoming evidence records\nUnknown - not assessed for security\n\nEnter: inspect · Tab: choose inspector section\n{}",
        r.name,
        if r.description.is_empty() {
            "No description"
        } else {
            &r.description
        },
        r.kind.label(),
        version_label,
        r.version.as_deref().unwrap_or("Unknown"),
        intent,
        edges,
        reverse,
        if matches!(&r.homebrew, Some(HomebrewMetadata::Cask { .. })) {
            "Casks may self-update; recorded version may differ."
        } else {
            ""
        }
    )
}
