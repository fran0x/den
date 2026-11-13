//! The actions dialog, drawn over the current screen: the package, the actions and the
//! selected one's dry run; then the running action and its result.
use super::theme::theme;
use super::*;
use crate::actions::{Action, Dialog, Preview};
use ratatui::widgets::Clear;

pub(super) fn render(frame: &mut Frame, app: &App) {
    let Some(dialog) = &app.actions.dialog else {
        return;
    };
    let (title, body, footer) = match dialog {
        Dialog::Menu {
            package,
            items,
            selected,
            preview,
        } => menu(package.as_deref(), items, *selected, preview),
        Dialog::Running {
            action,
            started,
            stopping,
        } => (
            action.title(),
            vec![
                Line::raw(format!(
                    "{} Running: {} · {} s",
                    spinner(),
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
        .border_style(border(dialog));
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

/// Blue while choosing or running, green when it worked, red only when it failed or was
/// stopped. The WRITE tag in the top bar says writes are possible.
fn border(dialog: &Dialog) -> Style {
    match dialog {
        Dialog::Done { summary, .. } if summary == "Done" => theme().ok(),
        Dialog::Done { .. } => theme().bad(),
        Dialog::Menu { .. } | Dialog::Running { .. } => heading(),
    }
}

fn menu(
    package: Option<&str>,
    items: &[crate::actions::Item],
    selected: usize,
    preview: &Preview,
) -> (String, Vec<Line<'static>>, String) {
    let title = package.map_or_else(|| "Actions".to_owned(), |p| format!("Actions · {p}"));
    let mut body = vec![];
    let column = items
        .iter()
        .map(|i| i.action.title().chars().count())
        .max()
        .unwrap_or(0)
        + 2;
    for (i, item) in items.iter().enumerate() {
        let marker = if selected == i { "> " } else { "  " };
        let style = if item.blocked.is_some() {
            muted()
        } else if selected == i {
            theme().selected_text().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        body.push(Line::from(vec![
            Span::styled(format!("{marker}{:<column$}", item.action.title()), style),
            Span::styled(item.action.command_line(), muted()),
        ]));
    }
    let Some(item) = items.get(selected) else {
        body.push(Line::raw(""));
        body.push(Line::styled(
            "Nothing here can be changed without force: it is in use or paused.",
            muted(),
        ));
        return (title, body, "Nothing to do here · Esc: close".into());
    };
    body.push(Line::raw(""));
    if let Some(why) = &item.blocked {
        body.push(Line::styled(
            format!("Not available: {why}"),
            theme().warn(),
        ));
    } else if let Some(note) = item.note.as_deref().or_else(|| note(&item.action)) {
        body.push(Line::styled(note.to_owned(), muted()));
    }
    if let Some(line) = item.action.preview_line()
        && item.blocked.is_none()
    {
        body.push(Line::styled(format!("Dry run: {line}"), heading()));
        let (text, style) = match preview {
            Preview::Waiting => (format!("{} Checking…", spinner()), muted()),
            Preview::Ready(text) => (text.clone(), Style::default()),
            Preview::Failed(text) => (text.clone(), theme().bad()),
            Preview::None => (String::new(), Style::default()),
        };
        body.extend(text.lines().map(|l| Line::styled(l.to_owned(), style)));
    }
    let footer = if item.blocked.is_some() {
        "↑↓: choose · Esc: close"
    } else if *preview == Preview::Waiting {
        "Checking… · ↑↓: choose · Esc: close"
    } else {
        "Enter: run it · ↑↓: choose · Esc: close"
    };
    (title, body, footer.into())
}

/// What a command without a dry run will and won't do.
fn note(action: &Action) -> Option<&'static str> {
    match action {
        Action::BrewUpdate => {
            Some("Downloads Homebrew's latest package data. No installed package changes.")
        }
        Action::BrewUninstall(_) => Some(
            "Homebrew has no dry run for uninstall. Den found nothing installed that needs \
             it, and Homebrew refuses anyway if something does.",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_only_means_it_failed() {
        let done = |summary: &str| Dialog::Done {
            action: crate::actions::Action::BrewUpdate,
            summary: summary.into(),
            output: String::new(),
            log: Err(String::new()),
        };
        let menu = Dialog::Menu {
            package: None,
            items: vec![],
            selected: 0,
            preview: Preview::None,
        };
        assert_eq!(border(&menu), heading());
        assert_eq!(border(&done("Done")), theme().ok());
        assert_eq!(border(&done("Failed: exit 1")), theme().bad());
        assert_eq!(border(&done("Stopped before it finished")), theme().bad());
        assert_ne!(heading(), theme().bad());
    }
}
