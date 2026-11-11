use super::*;

pub(super) const ACCENT: Color = Color::Blue;
/// Clipped-panel hints.
pub(super) const ENLARGE_MORE: &str = "… enlarge the terminal to see more";
/// The active area in the top bar.
pub(super) const ACTIVE: Color = Color::Yellow;
pub(super) fn muted() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}
pub(super) fn selection() -> Style {
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
}

/// Top bar, status line and footer around a body. `view` is "Area / Section" for the
/// Homebrew, Agents, Cloud and Docker areas: the active area is highlighted and followed by its section.
pub(super) fn chrome(
    frame: &mut Frame,
    view: &str,
    status: &str,
    searching: bool,
    actions: &str,
) -> Rect {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());
    let accent = Style::default().fg(ACCENT).bold();
    let mut title = vec![Span::styled(" DEN ", accent)];
    match view.split_once(" / ") {
        Some((area, section)) if matches!(area, "Homebrew" | "Agents" | "Cloud" | "Docker") => {
            for name in ["Homebrew", "Agents", "Cloud", "Docker"] {
                title.push(Span::raw(" "));
                if name == area {
                    // A distinct colour for the active area; bold and underline keep it
                    // visible without colour, on the terminal's own background.
                    title.push(Span::styled(
                        format!(" {name} "),
                        Style::default()
                            .fg(ACTIVE)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    ));
                    title.push(Span::styled(format!(" › {section} "), accent));
                } else {
                    title.push(Span::styled(format!(" {name} "), muted()));
                }
            }
        }
        _ => title.push(Span::styled(format!("/ {view}"), accent)),
    }
    frame.render_widget(Paragraph::new(Line::from(title)), rows[0]);
    frame.render_widget(Paragraph::new(safe_text(status)).style(muted()), rows[1]);
    let footer = if searching {
        format!(" Search · {actions}  Ctrl-U:clear  Ctrl-C:quit")
    } else {
        format!(" {actions}  /:search  ?:help  q:quit")
    };
    let footer = if frame.area().width < 65 && !searching {
        format!(
            "{} /:find ?:help q:quit",
            fit(actions, frame.area().width.saturating_sub(21) as usize)
        )
    } else {
        footer
    };
    frame.render_widget(Paragraph::new(footer).style(muted()), rows[3]);
    rows[2]
}

/// The one line above an inventory: view and filters, then search. Returns the list area.
pub(super) fn filter_line(
    frame: &mut Frame,
    area: Rect,
    mut text: String,
    query: &str,
    searching: bool,
) -> Rect {
    if searching || !query.is_empty() {
        if !text.is_empty() {
            text.push_str(" · ");
        }
        text.push_str(&format!(
            "Search: {}{}",
            safe_text(query),
            if searching { "_" } else { "" }
        ));
    }
    if text.is_empty() || area.height < 2 {
        return area;
    }
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
    frame.render_widget(
        Paragraph::new(fit(&text, area.width as usize)).style(if searching {
            Style::default()
        } else {
            muted()
        }),
        rows[0],
    );
    rows[1]
}

/// Browsing always keeps the inventory and its passive information visible.
pub(super) fn panes(area: Rect) -> [Rect; 2] {
    if area.width < 100 {
        let rows = Layout::vertical([Constraint::Percentage(55), Constraint::Min(1)]).split(area);
        return [rows[0], reading_area(rows[1])];
    }
    let widths =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    [
        Rect {
            width: widths[0].width.saturating_sub(2),
            ..widths[0]
        },
        reading_area(widths[1]),
    ]
}
/// The Homebrew panel. `more` is the hint shown when text is clipped; Codex and Claude
/// use `scrolled` instead.
pub(super) fn information(frame: &mut Frame, area: Rect, text: String, more: &str) {
    let block = pane(" Information ", false);
    let inner = block.inner(area);
    let lines: Vec<_> = text
        .lines()
        .map(|line| Line::from(safe_text(line)))
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let clipped = paragraph.line_count(inner.width) > inner.height as usize;
    frame.render_widget(block, area);
    frame.render_widget(paragraph, inner);
    if clipped && inner.height > 0 {
        frame.render_widget(
            Paragraph::new(fit(more, inner.width as usize)).style(muted()),
            Rect {
                y: inner.bottom() - 1,
                height: 1,
                ..inner
            },
        );
    }
}
/// Lines starting with "## " are section headings: bold, without the marker.
pub(super) const HEADING: &str = "## ";

/// The information panel for Codex and Claude: every section stacked, scrolled with
/// PgUp/PgDn. When text is clipped the last row says where you are.
pub(super) fn scrolled(
    frame: &mut Frame,
    area: Rect,
    text: &str,
    scroll: &mut crate::presentation::PanelScroll,
) {
    let block = pane(" Information ", false);
    let mut inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<_> = text
        .lines()
        .map(|line| match line.strip_prefix(HEADING) {
            Some(heading) => Line::styled(safe_text(heading), Style::default().bold()),
            None => Line::from(safe_text(line)),
        })
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let overflow = paragraph.line_count(inner.width) > inner.height as usize;
    let hint = if overflow && inner.height > 1 {
        inner.height -= 1;
        Some(Rect {
            y: inner.bottom(),
            height: 1,
            ..inner
        })
    } else {
        None
    };
    scroll.max = super::scroll_max(&paragraph, inner);
    scroll.offset = scroll.offset.min(scroll.max);
    scroll.page = inner.height.saturating_sub(1).max(1);
    frame.render_widget(paragraph.scroll((scroll.offset, 0)), inner);
    if let Some(hint) = hint {
        let text = if scroll.offset == 0 {
            "… PgDn for more"
        } else if scroll.offset >= scroll.max {
            "End · PgUp to go back"
        } else {
            "… PgUp/PgDn to scroll"
        };
        frame.render_widget(
            Paragraph::new(fit(text, hint.width as usize)).style(muted()),
            hint,
        );
    }
}

pub(super) fn reading_area(mut area: Rect) -> Rect {
    area.width = area.width.min(92);
    area
}

/// "56 B", "2.1 KB".
pub(super) fn size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

/// Fit using terminal cell widths (including wide Unicode), never byte truncation.
pub(super) fn fit(value: &str, width: usize) -> String {
    let value = safe_text(value);
    if width == 0 {
        return String::new();
    }
    if Span::raw(&value).width() <= width {
        return format!("{}{}", value, " ".repeat(width - Span::raw(&value).width()));
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        let n = Span::raw(ch.to_string()).width();
        if used + n >= width {
            break;
        }
        out.push(ch);
        used += n;
    }
    out.push('…');
    out.push_str(&" ".repeat(width.saturating_sub(used + 1)));
    out
}
/// Column widths for `columns`: (name, middle, tail).
fn column_widths(width: u16) -> (usize, usize, usize) {
    let width = width.saturating_sub(2) as usize;
    let name_width = (width * 2 / 5).clamp(8, 28).min(width);
    let remaining = width.saturating_sub(name_width + 1);
    let mid_width = if remaining > 15 {
        (remaining / 2).min(12)
    } else {
        0
    };
    let tail = remaining.saturating_sub(if mid_width > 0 { mid_width + 1 } else { 0 });
    (name_width, mid_width, tail)
}
/// Cells available to the last column of `columns` at this width.
pub(super) fn tail_width(width: u16) -> usize {
    column_widths(width).2
}
pub(super) fn columns(name: &str, middle: &str, tail: &str, width: u16) -> Line<'static> {
    let (name_width, mid_width, tail_width) = column_widths(width);
    Line::from(vec![
        Span::raw(fit(name, name_width)),
        Span::raw(" "),
        Span::styled(
            if mid_width > 0 {
                format!("{} ", fit(middle, mid_width))
            } else {
                String::new()
            },
            muted(),
        ),
        Span::raw(fit(tail, tail_width)),
    ])
}

/// Short action hints describe the focused view, not every available binding.
pub(super) fn actions(searching: bool) -> &'static str {
    if searching {
        "Enter:done Esc:done"
    } else {
        "Tab:area Shift+Tab:section"
    }
}
