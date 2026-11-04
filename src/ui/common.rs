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
    detailed: bool,
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
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" DEN / {view}"), Style::default().fg(ACCENT).bold()),
            Span::styled("   F2:Homebrew/Codex  F3:view", muted()),
        ])),
        rows[0],
    );
    frame.render_widget(Paragraph::new(safe_text(query)), rows[1]);
    frame.render_widget(Paragraph::new(safe_text(status)).style(muted()), rows[2]);
    let footer = if searching {
        " Search · Enter:done  Esc:done  Ctrl-U:clear".to_owned()
    } else {
        format!(
            " {} · {}  /:search  v:mode  ?:help  q:quit",
            if detailed { "Detailed" } else { "Compact" },
            actions
        )
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

/// At narrow sizes only the focused pane is shown. Full inspectors use a readable column.
pub(super) fn panes(area: Rect, focus: Focus, inspector: bool) -> [Rect; 2] {
    if inspector {
        return [Rect::default(), reading_area(area)];
    }
    if area.width < 100 {
        return if focus == Focus::Inventory {
            [area, Rect::default()]
        } else {
            [Rect::default(), reading_area(area)]
        };
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
pub(super) fn columns(name: &str, middle: &str, tail: &str, width: u16) -> Line<'static> {
    let width = width.saturating_sub(2) as usize;
    let name_width = (width * 2 / 5).clamp(8, 28).min(width);
    let remaining = width.saturating_sub(name_width + 1);
    let mid_width = if remaining > 15 {
        (remaining / 2).min(12)
    } else {
        0
    };
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
        Span::raw(fit(
            tail,
            remaining.saturating_sub(if mid_width > 0 { mid_width + 1 } else { 0 }),
        )),
    ])
}

/// Short action hints describe the focused view, not every available binding.
pub(super) fn actions(focus: Focus, inspector: bool, detail: &str) -> String {
    if focus == Focus::Inventory {
        return "Enter:inspect Tab:pane".into();
    }
    if inspector {
        format!(
            "Esc:Back {}",
            if detail.is_empty() {
                "arrows:scroll"
            } else {
                detail
            }
        )
    } else if detail.is_empty() {
        "z:expand Tab:list".into()
    } else {
        format!("{detail} z:expand")
    }
}
