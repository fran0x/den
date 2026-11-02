use super::*;
use crate::codex::state::{State, View};
pub(super) fn render(frame: &mut Frame, state: &mut State) {
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Den: resize to 30 x 10\nF2:module F3:Codex q:quit"),
            area,
        );
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(format!(
            " DEN / Codex / {:?} | F2: Homebrew · F3: view",
            state.view
        ))
        .style(Style::default().fg(Color::Cyan)),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(safe_text(&state.status())).style(Style::default().fg(Color::Yellow)),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(format!(
            "Search{}: {}",
            if state.searching { "*" } else { "" },
            safe_text(&state.query)
        )),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(
            "F3:view /:search Tab:focus s:sources Enter:inspect r:refresh ?:help q:quit",
        )
        .style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        rows[4],
    );
    let panes = Layout::default()
        .direction(if area.width >= 80 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(rows[3]);
    let block = pane(
        if state.view == View::Instructions {
            " Instruction chain "
        } else if state.sources {
            " Sources "
        } else {
            " Settings "
        },
        !state.focus_details,
    );
    let inner = block.inner(panes[0]);
    frame.render_widget(block, panes[0]);
    state.clamp();
    let page = inner.height.max(1) as usize;
    let selected = state.selected[state.slot()];
    let start = (selected / page) * page;
    let entries = state.rows();
    let items: Vec<_> = entries
        .iter()
        .skip(start)
        .take(page)
        .map(|(_, label)| ListItem::new(safe_text(label)))
        .collect();
    let mut selection =
        ListState::default().with_selected((!entries.is_empty()).then_some(selected - start));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Cyan)),
        inner,
        &mut selection,
    );
    if !state.focus_details {
        state.page = page;
    }
    let area = if state.help { rows[3] } else { panes[1] };
    let block = pane(
        " Local evidence · Enter: inspect ",
        state.focus_details || state.help,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = state.text();
    let lines: Vec<_> = text.lines().map(|s| Line::from(safe_text(s))).collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    state.scroll_max = scroll_max(&paragraph, inner);
    state.scroll = state.scroll.min(state.scroll_max);
    if state.focus_details || state.help {
        state.page = inner.height.max(1) as usize;
    }
    frame.render_widget(paragraph.scroll((state.scroll, 0)), inner);
}
