//! The actions dialog, drawn over the current screen: the menu, the confirmation with its
//! preview, the running action and its result.
use super::theme::theme;
use super::*;
use crate::actions::{Action, Dialog, Preview};
use ratatui::widgets::Clear;

pub(super) fn render(frame: &mut Frame, app: &App) {
    let Some(dialog) = &app.actions.dialog else {
        return;
    };
    let (title, body, footer) = match dialog {
        Dialog::Menu { items, selected } => menu(app, items, *selected),
        Dialog::Confirm { item, preview } => {
            confirm(app, &item.action, item.blocked.as_deref(), preview)
        }
        Dialog::Running {
            action,
            started,
            stopping,
        } => (
            action.title(),
            vec![
                Line::raw(format!(
                    "Running: {} · {} s",
                    action.command_line(),
                    started.elapsed().as_secs()
                )),
                Line::raw(""),
                Line::styled(
                    if *stopping {
                        format!(
                            "Stopping: Homebrew has up to {} s to clean up",
                            crate::actions::GRACE.as_secs()
                        )
                    } else {
                        "Den waits for it to finish, then reloads Homebrew.".into()
                    },
                    muted(),
                ),
            ],
            "Esc or Ctrl-C: stop it".into(),
        ),
        Dialog::Done {
            action,
            summary,
            output,
            log,
        } => {
            let style = if summary == "Done" {
                theme().ok()
            } else {
                theme().bad()
            };
            let mut body = vec![
                Line::styled(summary.clone(), style),
                Line::raw(action.command_line()),
                Line::styled(
                    match log {
                        Ok(path) => format!("Logged to {}", home(path)),
                        Err(why) => format!("Log: {why}"),
                    },
                    muted(),
                ),
            ];
            if !output.is_empty() {
                body.push(Line::raw(""));
                body.extend(output.lines().map(|l| Line::raw(l.to_owned())));
            }
            (
                format!("{} · {summary}", action.title()),
                body,
                "Enter or Esc: close".into(),
            )
        }
    };
    let footer = match &app.actions.notice {
        Some(notice) => notice.clone(),
        None => footer,
    };
    // Fit the content as wrapped: borders, the body, a blank line and the footer.
    let area = frame.area();
    let width = area.width.saturating_sub(4).min(100);
    let paragraph = Paragraph::new(body).wrap(Wrap { trim: false });
    let lines = paragraph.line_count(width.saturating_sub(2));
    let wanted = u16::try_from(lines).unwrap_or(u16::MAX).saturating_add(4);
    let height = wanted.clamp(8, 28).min(area.height.saturating_sub(2));
    let rect = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {} ", safe_text(&title)))
        .border_style(if app.actions.on() {
            theme().bad()
        } else {
            heading()
        });
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [body_area, _, footer_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);
    // Long output: keep the end in view, where the result is.
    let scroll = if matches!(dialog, Dialog::Done { .. }) {
        scroll_max(&paragraph, body_area)
    } else {
        0
    };
    frame.render_widget(paragraph.scroll((scroll, 0)), body_area);
    frame.render_widget(
        Paragraph::new(safe_text(&footer)).style(theme().key()),
        footer_area,
    );
}

fn mode_line(app: &App) -> Line<'static> {
    if app.actions.on() {
        Line::styled(
            "WRITE mode is on: each action shows what it will do and runs only on y.",
            theme().bad(),
        )
    } else if app.actions.allowed() {
        Line::styled(
            "Writes are allowed but off: press W to turn them on. c copies a command.",
            theme().warn(),
        )
    } else {
        Line::styled(
            "Read-only: c copies a command. To run them, use dew instead of den.",
            muted(),
        )
    }
}

fn menu(
    app: &App,
    items: &[crate::actions::Item],
    selected: usize,
) -> (String, Vec<Line<'static>>, String) {
    let mut body = vec![mode_line(app), Line::raw("")];
    let column = items
        .iter()
        .map(|i| i.action.title().chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    for (i, item) in items.iter().enumerate() {
        let marker = if i == selected { "> " } else { "  " };
        let detail = match &item.blocked {
            Some(why) => format!("not available: {why}"),
            None => item.action.command_line(),
        };
        let style = if item.blocked.is_some() {
            muted()
        } else if i == selected {
            theme().selected_text()
        } else {
            Style::default()
        };
        body.push(Line::from(vec![
            Span::styled(format!("{marker}{:<column$}", item.action.title()), style),
            Span::styled(detail, muted()),
        ]));
    }
    (
        "Actions".into(),
        body,
        "Enter: details · c: copy the command · Esc: close".into(),
    )
}

fn confirm(
    app: &App,
    action: &Action,
    blocked: Option<&str>,
    preview: &Preview,
) -> (String, Vec<Line<'static>>, String) {
    let mut body = vec![mode_line(app), Line::raw("")];
    body.push(Line::from(vec![
        Span::styled("Runs: ", muted()),
        Span::raw(action.command_line()),
    ]));
    if let Some(note) = note(action) {
        body.push(Line::styled(note.to_owned(), muted()));
    }
    if let Some(why) = blocked {
        body.push(Line::raw(""));
        body.push(Line::styled(format!("Not available: {why}"), theme().bad()));
    }
    if let Some(line) = action.preview_line()
        && blocked.is_none()
    {
        body.push(Line::raw(""));
        body.push(Line::styled(format!("Preview: {line}"), heading()));
        let text = match preview {
            Preview::Waiting => "Checking…".to_owned(),
            Preview::Skipped => "Not run: writes are off.".to_owned(),
            Preview::Ready(text) | Preview::Failed(text) => text.clone(),
            Preview::None => String::new(),
        };
        let style = if matches!(preview, Preview::Failed(_)) {
            theme().bad()
        } else {
            Style::default()
        };
        body.extend(text.lines().map(|l| Line::styled(l.to_owned(), style)));
    }
    let footer = if blocked.is_some() {
        "c: copy the command · Esc: back"
    } else if app.actions.on() {
        "y: run it · c: copy the command · Esc: cancel"
    } else {
        "c: copy the command · Esc: cancel"
    };
    (action.title(), body, footer.into())
}

/// What a command without a dry run will and won't do.
fn note(action: &Action) -> Option<&'static str> {
    match action {
        Action::BrewUpdate => {
            Some("Downloads Homebrew's latest package data. No installed package changes.")
        }
        Action::BrewUninstall(_) => Some(
            "Homebrew has no dry run for this. Den found nothing installed that needs it, \
             and Homebrew refuses anyway if something does.",
        ),
        _ => None,
    }
}

fn home(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}
