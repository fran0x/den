mod actions;
mod common;
mod problems;
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

pub fn render(frame: &mut Frame, app: &mut App) {
    set_write_mode(app.actions.allowed());
    set_problems(crate::problems::problems(app));
    screen(frame, app);
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        return;
    }
    if app.actions.dialog.is_some() {
        actions::render(frame, app);
    } else if let Some(notice) = &app.actions.notice {
        // Replaces the status line until the next key.
        frame.render_widget(
            Paragraph::new(safe_text(notice)).style(theme::theme().warn()),
            Rect {
                y: area.y + 1,
                height: 1,
                ..area
            },
        );
    }
}

fn screen(frame: &mut Frame, app: &mut App) {
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
    if app.problems.is_some() {
        problems::render(frame, app);
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
        } else if app.is_homebrew() && app.actions.allowed() {
            "a:tools/all x:actions Tab:area Shift+Tab:section"
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
                let mut text = r
                    .homebrew
                    .as_ref()
                    .map_or(intent.label(), |m| m.intent_label())
                    .to_owned();
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
            let name = r.display_name();
            let cells = (
                name.as_str(),
                r.version.as_deref().unwrap_or("Unknown"),
                tail.as_str(),
            );
            let selected = app.selected == Some(row);
            // Casks stand apart from formulae in their own colour.
            let item = if r.kind == crate::inventory::ResourceKind::Cask {
                marked_item(cells, inner.width, selected, theme::theme().cask())
            } else {
                status_item(cells, inner.width, selected)
            };
            // Tools stand out in bold, so All still shows what was installed on request.
            if app.is_homebrew() && app.is_tool(index) {
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
    let intent = r.homebrew.as_ref().map_or("Unknown", |m| m.intent_label());
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
