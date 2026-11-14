use super::*;

use super::theme::theme;

/// Clipped-panel hints.
pub(super) const ENLARGE_MORE: &str = "… enlarge the terminal to see more";
pub(super) fn muted() -> Style {
    theme().muted()
}
pub(super) fn selection() -> Style {
    theme().selection()
}
pub(super) fn accent() -> Style {
    theme().accent()
}
/// Section headings in panels.
pub(super) fn heading() -> Style {
    accent().add_modifier(Modifier::BOLD)
}
/// "Tab:area Shift+Tab:section": key names stand out from their muted labels.
/// Keeps every character, so widths and screen text are unchanged.
pub(super) fn hints(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", muted()));
        }
        match word.split_once(':') {
            Some((key, label)) if !key.is_empty() => {
                spans.push(Span::styled(key.to_owned(), theme().key()));
                spans.push(Span::styled(format!(":{label}"), muted()));
            }
            _ => spans.push(Span::styled(word.to_owned(), muted())),
        }
    }
    Line::from(spans)
}

// Write mode, for the top bar: set at the start of each frame, on the thread that draws it.
thread_local! {
    static WRITE_MODE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn set_write_mode(on: bool) {
    WRITE_MODE.with(|w| w.set(on));
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
    let accent = heading();
    let mut title = vec![Span::styled(" DEN ", accent)];
    if WRITE_MODE.with(std::cell::Cell::get) {
        title.push(Span::styled(
            " WRITE ",
            theme()
                .bad()
                .add_modifier(Modifier::REVERSED | Modifier::BOLD),
        ));
    }
    match view.split_once(" / ") {
        Some((area, section))
            if matches!(
                area,
                "Homebrew" | "Agents" | "Cloud" | "Docker" | "Toolchains"
            ) =>
        {
            for name in ["Homebrew", "Agents", "Cloud", "Docker", "Toolchains"] {
                title.push(Span::raw(" "));
                if name == area {
                    title.push(Span::styled(format!(" {name} "), theme().active()));
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
    frame.render_widget(Paragraph::new(hints(&footer)), rows[3]);
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
    scroll: &mut crate::tui::presentation::PanelScroll,
) {
    let block = pane(" Information ", false);
    let mut inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<_> = text
        .lines()
        .map(|line| match line.strip_prefix(HEADING) {
            Some(text) => Line::styled(safe_text(text), heading()),
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
    cells(name, middle, tail, width, false)
}
/// `columns` with status words coloured: Ready and Running green, Needs approval
/// yellow, Problem red. Used where cells hold states (Agents, Cloud, Docker).
pub(super) fn status_columns(name: &str, middle: &str, tail: &str, width: u16) -> Line<'static> {
    cells(name, middle, tail, width, true)
}
/// A `status_columns` list item. The selected row styles its own text, and the list
/// highlights it with `status_selection`, so status colours stay visible when selected.
pub(super) fn status_item(
    cells: (&str, &str, &str),
    width: u16,
    selected: bool,
) -> ListItem<'static> {
    ListItem::new(status_line(cells, width, selected))
}
/// A `status_item` whose name stands out in `style` (a cask, the active profile), also
/// when the row is selected.
pub(super) fn marked_item(
    cells: (&str, &str, &str),
    width: u16,
    selected: bool,
    style: Style,
) -> ListItem<'static> {
    let mut line = status_line(cells, width, selected);
    if let Some(name) = line.spans.first_mut() {
        name.style = name.style.patch(style);
    }
    ListItem::new(line)
}
fn status_line(
    (name, middle, tail): (&str, &str, &str),
    width: u16,
    selected: bool,
) -> Line<'static> {
    let mut line = status_columns(name, middle, tail, width);
    if selected {
        for span in &mut line.spans {
            if !is_status(&span.content) {
                span.style = theme().selected_text();
            }
        }
    }
    line
}
pub(super) fn status_selection() -> Style {
    theme().selection_background()
}
fn is_status(text: &str) -> bool {
    STATUS_WORDS.contains(&text) || text.starts_with(UPDATE)
}
fn cells(name: &str, middle: &str, tail: &str, width: u16, status: bool) -> Line<'static> {
    let (name_width, mid_width, tail_width) = column_widths(width);
    let cell = |text: String, base: Style| {
        if status {
            status_spans(&text, base)
        } else {
            vec![Span::styled(text, base)]
        }
    };
    let mut spans = vec![Span::raw(fit(name, name_width)), Span::raw(" ")];
    if mid_width > 0 {
        spans.extend(cell(fit(middle, mid_width), muted()));
        spans.push(Span::raw(" "));
    }
    spans.extend(cell(fit(tail, tail_width), Style::default()));
    Line::from(spans)
}

const STATUS_WORDS: [&str; 5] = ["Ready", "Running", "Needs approval", "Problem", UPDATE];
/// Starts an update notice, "↑ 15.0": coloured with its version, to the end of its part.
pub(super) const UPDATE: &str = "↑";

/// Colour a status word where a state starts: at the start of the cell or of a
/// "·" part, optionally after a tool name ("Codex Ready · Claude Needs approval").
/// Words inside free text, such as descriptions, keep the base style.
pub(super) fn status_spans(text: &str, base: Style) -> Vec<Span<'static>> {
    let starts_state = |before: &str| {
        let before = before
            .strip_suffix("Codex ")
            .or_else(|| before.strip_suffix("Claude "))
            .unwrap_or(before);
        before.is_empty() || before.ends_with("· ")
    };
    let mut spans = Vec::new();
    let mut done = 0;
    let mut at = 0;
    while at < text.len() {
        let rest = &text[at..];
        let hit = STATUS_WORDS
            .iter()
            .find(|w| rest.starts_with(**w) && starts_state(&text[..at]));
        if let Some(word) = hit {
            if done < at {
                spans.push(Span::styled(text[done..at].to_owned(), base));
            }
            let style = match *word {
                "Needs approval" | UPDATE => theme().warn(),
                "Problem" => theme().bad(),
                _ => theme().ok(),
            };
            let len = if *word == UPDATE {
                rest.find(" ·").unwrap_or(rest.len())
            } else {
                word.len()
            };
            spans.push(Span::styled(rest[..len].to_owned(), style));
            at += len;
            done = at;
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    if done < text.len() {
        spans.push(Span::styled(text[done..].to_owned(), base));
    }
    spans
}

/// Short action hints describe the focused view, not every available binding.
pub(super) fn actions(searching: bool) -> &'static str {
    if searching {
        "Enter:done Esc:done"
    } else {
        "Tab:area Shift+Tab:section"
    }
}

/// What a list-and-panel section (Cloud, Toolchains) hands to `list_view`.
pub(super) struct ListView {
    pub(super) title: &'static str,
    pub(super) pane: &'static str,
    pub(super) header: (&'static str, &'static str, &'static str),
    pub(super) status: String,
    pub(super) rows: Vec<(String, String, String)>,
    /// The panel text, or (with no rows) the message, a blank line and the panel text.
    pub(super) text: String,
    pub(super) selected: usize,
    pub(super) query: String,
    pub(super) searching: bool,
    /// The row in use (the active profile, the current context), shown highlighted.
    pub(super) current: Option<usize>,
    /// dew offers actions here: the footer shows `x:actions`.
    pub(super) actions: bool,
}

/// A searchable three-column list with a header, and a scrolling information panel.
pub(super) fn list_view(
    frame: &mut Frame,
    view: ListView,
    page_size: &mut usize,
    panel: &mut crate::tui::presentation::PanelScroll,
) {
    let ListView {
        title,
        pane: pane_title,
        header,
        status,
        rows,
        text,
        selected,
        query,
        searching,
        current,
        actions: offers_actions,
    } = view;
    let body = chrome(
        frame,
        title,
        &status,
        searching,
        if searching {
            actions(true)
        } else if offers_actions {
            "x:actions Tab:area Shift+Tab:section"
        } else {
            "Tab:area Shift+Tab:section"
        },
    );
    let selected = selected.min(rows.len().saturating_sub(1));
    let panes = panes(body);
    if panes[0].width > 0 {
        let block = pane(pane_title, true);
        let inner = block.inner(panes[0]);
        frame.render_widget(block, panes[0]);
        let inner = filter_line(frame, inner, String::new(), &query, searching);
        let inner = if inner.height > 2 && !rows.is_empty() {
            frame.render_widget(
                Paragraph::new(columns(header.0, header.1, header.2, inner.width)).style(muted()),
                Rect {
                    x: inner.x + 2,
                    width: inner.width.saturating_sub(2),
                    height: 1,
                    ..inner
                },
            );
            Rect {
                y: inner.y + 1,
                height: inner.height - 1,
                ..inner
            }
        } else {
            inner
        };
        let page = inner.height.max(1) as usize;
        *page_size = page;
        if rows.is_empty() {
            // The reason, once; the panel shows only what follows it.
            let first = text
                .split_once("\n\n")
                .map_or(text.as_str(), |(first, _)| first);
            frame.render_widget(
                Paragraph::new(first.to_owned()).wrap(Wrap { trim: true }),
                inner,
            );
        } else {
            let start = (selected / page) * page;
            let items: Vec<_> = rows
                .iter()
                .skip(start)
                .take(page)
                .enumerate()
                .map(|(i, (n, x, y))| {
                    let cells = (n.as_str(), x.as_str(), y.as_str());
                    if current == Some(start + i) {
                        marked_item(cells, inner.width, start + i == selected, theme().current())
                    } else {
                        status_item(cells, inner.width, start + i == selected)
                    }
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(selected - start));
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_symbol("> ")
                    .highlight_style(status_selection()),
                inner,
                &mut state,
            );
        }
    }
    if panes[1].width > 0 {
        let text = if rows.is_empty() {
            text.split_once("\n\n").map_or("", |(_, more)| more)
        } else {
            &text
        };
        scrolled(frame, panes[1], text, panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<(String, Style)> {
        status_spans(text, Style::default())
            .into_iter()
            .map(|s| (s.content.into_owned(), s.style))
            .collect()
    }

    #[test]
    fn only_states_are_coloured() {
        let t = theme();
        assert_eq!(
            words("Codex Ready · Claude Needs approval"),
            vec![
                ("Codex ".into(), Style::default()),
                ("Ready".into(), t.ok()),
                (" · Claude ".into(), Style::default()),
                ("Needs approval".into(), t.warn()),
            ]
        );
        assert_eq!(
            words("Problem · Not installed")[0],
            ("Problem".into(), t.bad())
        );
        // Inside free text, and truncated, words stay plain.
        assert_eq!(words("Off · not Ready yet").len(), 1);
        assert_eq!(words("Rea…").len(), 1);
        assert_eq!(
            words("↑ 15.0 · Requested"),
            vec![
                ("↑ 15.0".into(), t.warn()),
                (" · Requested".into(), Style::default())
            ]
        );
    }
}

/// Spinner frames, as Homebrew draws them while it works.
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// The frame for now: one step every 80 ms while Den redraws during work.
pub(super) fn spinner() -> char {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    frame(millis)
}

fn frame(millis: u128) -> char {
    SPINNER[(millis / 80 % SPINNER.len() as u128) as usize]
}

#[cfg(test)]
mod spinner_tests {
    use super::*;

    #[test]
    fn the_spinner_turns_every_80_ms() {
        assert_eq!(frame(0), '⠋');
        assert_eq!(frame(79), '⠋');
        assert_eq!(frame(80), '⠙');
        assert_eq!(frame(80 * 10), '⠋');
        let frames: std::collections::HashSet<_> = (0..10).map(|i| frame(i * 80)).collect();
        assert_eq!(frames.len(), 10);
    }
}
