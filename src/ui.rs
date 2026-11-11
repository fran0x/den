mod common;
mod theme;
pub use theme::{Theme, set_theme};
mod homebrew;
mod navigation;
use common::*;
mod agents;
mod cloud;
mod diagnostics;
mod docker;
mod skills;
mod toolchains;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{App, Focus},
    inventory::{HomebrewMetadata, safe_text},
};

pub const HELP: &str = "Den — navigation\n\n\
F2: Homebrew / Codex · F3: next view; each view retains its state.\nEnter from list: full-screen inspector · Esc: Back\nv: Compact / Detailed · z: full-screen toggle from either pane\n\n\
Browse read-only Homebrew inventory, or offline snapshots/demo data.\n\n\
Up/Down or k/j   Move selection / scroll focused pane\n\
PageUp/PageDown  Move a page\n\
Home/End        First / last item or detail line\n\
Tab/Shift-Tab   Switch Homebrew/Codex areas; retain context\n\
Enter           Inspect selected resource (full screen)\nRight           Tree expansion in an explicit inspector\n\
[ / ]           Inspector sections / content\n\
/               Edit search (live, case insensitive)\n\
Enter/Esc       Finish search, retaining the query\n\
Backspace       Delete last search character\n\
Ctrl-U          Clear query while editing\n\
f               Cycle kind filter; wraps back to All\n\
i               Cycle Homebrew intent: Requested/Dependency/Unknown/All\n\
u               Toggle Used by others (recorded runtime; independent of intent)\n\
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
Browsing information is passive, below the inventory at narrow widths. Minimum useful size: 30 x 10.\n\n\
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
    if app.help {
        let body = chrome(
            frame,
            "Help",
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
    app.focus_inventory();

    if app.module == crate::app::Module::Agents {
        agents::render(frame, app);
        return;
    }
    if app.module == crate::app::Module::Cloud {
        cloud::render(frame, app);
        return;
    }
    if app.module == crate::app::Module::Toolchains {
        toolchains::render(frame, app);
        return;
    }
    if app.module == crate::app::Module::Docker {
        docker::render(frame, app);
        return;
    }
    // With nothing collected yet (loading or Homebrew unavailable), the list carries the
    // message once; the status line, filter counts and panel stay quiet.
    let nothing = app.live && app.inventory.resources.is_empty();
    let status = if nothing {
        String::new()
    } else if app.live {
        app.collection_message()
    } else {
        format!("{}{}", app.source_label, app.outdated_text())
    };
    let body = chrome(
        frame,
        app.destination().label(),
        &status,
        app.searching,
        if app.searching {
            actions(true)
        } else if app.is_homebrew() {
            "a:tools/all Tab:area Shift+Tab:section"
        } else {
            "f:type Tab:area"
        },
    );
    let panes = panes(body);
    if panes[0].width > 0 {
        render_inventory(frame, app, panes[0]);
    }
    if panes[1].width > 0 && nothing {
        frame.render_widget(pane(" Information ", false), panes[1]);
    } else if panes[1].width > 0 {
        if app.is_homebrew() {
            homebrew::information(frame, app, panes[1]);
        } else {
            information(frame, panes[1], overview(app), ENLARGE_MORE);
        }
    }
}

fn pane(title: &str, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::TOP)
        .title(format!("{}{}", if focused { ">" } else { " " }, title))
        .border_style(if focused { accent() } else { muted() })
}

fn render_inventory(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = pane(" Inventory ", app.focus == Focus::Inventory);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let view = if app.live && app.inventory.resources.is_empty() {
        String::new()
    } else if app.is_homebrew() {
        homebrew::view_text(app)
    } else {
        format!("Type: {}", app.filter.map_or("All", |k| k.label()))
    };
    let inner = filter_line(frame, inner, view, &app.query, app.searching);
    if app.focus == Focus::Inventory {
        app.page_size = inner.height.max(1) as usize;
    }
    if app.visible.is_empty() {
        frame.render_widget(
            Paragraph::new(if app.live && app.inventory.resources.is_empty() {
                app.collection_message()
            } else if app.is_homebrew() && app.inventory.resources.is_empty() {
                "No installed Homebrew packages in snapshot".into()
            } else if app.tools_only() && app.query.is_empty() && app.homebrew_counts().all > 0 {
                "No packages recorded as requested.\na: show all packages".into()
            } else if app.tools_only() {
                "No matching tools.\na: show all · /:search".into()
            } else if app.is_homebrew() {
                "No matches.\n/:search".into()
            } else {
                "No matches.\n/:search · f:type".into()
            })
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let items: Vec<_> = app
        .visible
        .iter()
        .enumerate()
        .map(|(row, &index)| {
            let r = &app.inventory.resources[index];
            let tail = if app.is_homebrew() {
                let users = app.graph.recorded_users(index);
                let intent = r.intent();
                let mut text = intent.label().to_owned();
                if users > 0 {
                    let required = format!("required by {users}");
                    let full = format!("{text} · {required}");
                    // When narrow, keep the fact that explains why the package is here.
                    text = if full.chars().count() <= tail_width(inner.width) {
                        full
                    } else if intent == crate::inventory::InstallationIntent::Requested {
                        text
                    } else {
                        required
                    };
                }
                // An update comes first so it survives narrow widths.
                if let Some(latest) = r.homebrew.as_ref().and_then(|m| m.update()) {
                    text = format!("{UPDATE} {latest} · {text}");
                }
                if !r.description.is_empty() {
                    text.push_str(&format!(" · {}", r.description));
                }
                text
            } else {
                r.description.clone()
            };
            let item = status_item(
                (
                    &r.display_name(),
                    r.version.as_deref().unwrap_or("Unknown"),
                    &tail,
                ),
                inner.width,
                app.selected == Some(row),
            );
            // Tools stand out in bold, so All still shows what was installed on request.
            if app.is_homebrew() && app.is_tool(r) {
                item.style(Style::default().add_modifier(Modifier::BOLD))
            } else {
                item
            }
        })
        .collect();
    let list = List::new(items)
        .highlight_symbol("> ")
        .highlight_style(status_selection());
    let mut state = ListState::default()
        .with_selected(app.selected)
        .with_offset(app.list_offset);
    frame.render_stateful_widget(list, inner, &mut state);
    app.list_offset = state.offset();
}

/// The Homebrew page outside the package list is Diagnostics; packages have no inspector.
fn render_details(frame: &mut Frame, app: &mut App, area: Rect) {
    diagnostics::render(frame, app, area);
}

fn scroll_max(paragraph: &Paragraph<'_>, area: Rect) -> u16 {
    if area.width == 0 || area.height == 0 {
        return 0;
    }
    paragraph
        .line_count(area.width)
        .saturating_sub(area.height as usize)
        .try_into()
        .unwrap_or(u16::MAX)
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
        "{}\n\n{}\n\n{} · {}: {}\nInstallation intent: {}\n\nRelationships: {} outgoing / {} incoming evidence records\nUnknown - not assessed for security\n\n{}",
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
