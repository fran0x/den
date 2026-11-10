use super::*;

pub(super) const ACCENT: Color = Color::Blue;
pub(super) fn muted() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}
pub(super) fn selection() -> Style {
    Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
}

pub(super) fn chrome(
    frame: &mut Frame,
    view: &str,
    query: &str,
    status: &str,
    searching: bool,
    actions: &str,
) -> Rect {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());
    let title = if view.starts_with("Homebrew") {
        format!(" DEN [Homebrew] Codex · Tab:area · {view}")
    } else if view.starts_with("Codex") {
        format!(" DEN Homebrew [Codex] · Tab:area · {view}")
    } else {
        format!(" DEN / {view}")
    };
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(ACCENT).bold()),
        rows[0],
    );
    frame.render_widget(Paragraph::new(safe_text(query)), rows[1]);
    frame.render_widget(Paragraph::new(safe_text(status)).style(muted()), rows[2]);
    let footer = if searching {
        format!(" Search · {actions}  Ctrl-U:clear  Ctrl-C:quit")
    } else {
        format!(" {}  g:sections  /:search  ?:help  q:quit", actions)
    };
    let footer = if frame.area().width < 65 && !searching {
        format!(
            "{} /:find ?:help q:quit",
            fit(actions, frame.area().width.saturating_sub(21) as usize)
        )
    } else {
        footer
    };
    frame.render_widget(Paragraph::new(footer).style(muted()), rows[4]);
    rows[3]
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
pub(super) fn information(frame: &mut Frame, area: Rect, text: String) {
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
            Paragraph::new(fit("… Enter:inspect for more", inner.width as usize)).style(muted()),
            Rect {
                y: inner.bottom() - 1,
                height: 1,
                ..inner
            },
        );
    }
}
pub(super) fn reading_area(mut area: Rect) -> Rect {
    area.width = area.width.min(92);
    area
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
        "Enter:inspect Tab:area Esc:Back"
    }
}
