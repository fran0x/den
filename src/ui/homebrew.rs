//! Passive Homebrew summary. Projection and rendering never mutate navigation state.
use super::*;
use crate::{explorer::Explorer, graph::Evidence};

/// The Homebrew filter line: the active view is bracketed; counts follow search.
pub(super) fn view_text(app: &App) -> String {
    let counts = app.homebrew_counts();
    let (tools, all) = if app.show_all {
        (
            format!("Tools {}", counts.tools),
            format!("[All {}]", counts.all),
        )
    } else {
        (
            format!("[Tools {}]", counts.tools),
            format!("All {}", counts.all),
        )
    };
    format!(
        "{tools} · {all}{}",
        // "required by N" comes from the bounded graph; say so when it was cut short.
        if app.graph.truncated {
            " · required-by partial"
        } else {
            ""
        }
    )
}

pub(super) fn information(frame: &mut Frame, app: &App, area: Rect) {
    let Some(resource) = app.resource() else {
        super::information(
            frame,
            area,
            "Select a resource to inspect its details.".into(),
            ENLARGE_MORE,
        );
        return;
    };
    let block = pane(" Information ", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(node) = app.selected_node() else {
        return;
    };
    let root = app
        .explorer
        .installation
        .as_ref()
        .filter(|root| root.node == node)
        .cloned()
        .unwrap_or_else(|| app.graph.preferred(node));
    let evidence = app.explorer.evidence;
    let context = match evidence {
        Evidence::Recorded if root.keg.is_none() && app.graph.versions(node).is_empty() => {
            "Recorded runtime · metadata unavailable".into()
        }
        Evidence::Recorded => root
            .keg
            .as_ref()
            .map_or("Recorded runtime · linked keg unknown".into(), |keg| {
                format!("Recorded runtime · keg {keg}")
            }),
        Evidence::Current => "Current declarations · not historical".into(),
    };
    // Very small terminals cannot fit metadata plus both directions. Keep the two
    // relationship labels together rather than silently hiding the reverse tree.
    if inner.height < 7 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(fit("Depends on / Required by", inner.width as usize)),
                Line::from(fit(&context, inner.width as usize)),
                Line::from("… enlarge the terminal for trees"),
            ]),
            inner,
        );
        return;
    }
    let cask = matches!(resource.homebrew, Some(HomebrewMetadata::Cask { .. }));
    let mut summary = vec![
        format!(
            "{} · {}: {}",
            resource.name,
            if cask {
                "Recorded version"
            } else {
                "Installed"
            },
            resource.version.as_deref().unwrap_or("Unknown")
        ),
        format!(
            "Installation intent: {}",
            resource
                .homebrew
                .as_ref()
                .map_or("Unknown", |m| m.intent().label())
        ),
    ];
    if inner.height >= 12 {
        summary.push(if resource.description.is_empty() {
            "No description".into()
        } else {
            resource.description.clone()
        });
    }
    summary.push(context);
    let regions = Layout::vertical([
        Constraint::Length(summary.len() as u16),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(
            summary
                .iter()
                .map(|line| Line::from(fit(line, inner.width as usize)))
                .collect::<Vec<_>>(),
        ),
        regions[0],
    );
    let trees =
        Layout::vertical([Constraint::Percentage(50), Constraint::Min(1)]).split(regions[1]);
    let mut partial = inner.height < 12 && !resource.description.is_empty()
        || summary
            .iter()
            .any(|line| Span::raw(safe_text(line)).width() > inner.width as usize);
    let mut y = regions[1].y;
    for (reverse, space) in [(false, trees[0]), (true, trees[1])] {
        let area = Rect { y, ..space };
        let preview = Explorer::preview(
            &app.graph,
            &app.inventory,
            root.clone(),
            evidence,
            reverse,
            area.height.saturating_sub(1).min(8) as usize,
        );
        partial |= preview.limited;
        let heading = if reverse { "Required by" } else { "Depends on" };
        let mut lines = vec![Line::styled(heading, Style::default().bold())];
        if preview.rows.is_empty() {
            let empty = if evidence == Evidence::Recorded
                && root.keg.is_none()
                && !app.graph.versions(node).is_empty()
            {
                "Linked keg unknown; linkage unavailable"
            } else if !reverse && !app.graph.available(&root, evidence) {
                "Dependency metadata unavailable"
            } else if evidence == Evidence::Current {
                "No matching declarations"
            } else {
                "No matching recorded relationships"
            };
            lines.push(Line::from(fit(empty, area.width as usize)));
        } else {
            for row in &preview.rows {
                let label = safe_text(&row.label);
                partial |= Span::raw(&label).width() > area.width as usize;
                lines.push(Line::from(fit(&label, area.width as usize)));
            }
        }
        let used = (lines.len() as u16).min(area.height);
        frame.render_widget(Paragraph::new(lines), area);
        // Reserve each direction's budget, but do not leave a large gap between short trees.
        y += used + u16::from(used < area.height);
    }
    frame.render_widget(
        Paragraph::new(fit(
            if partial {
                "PARTIAL · enlarge the terminal to see more"
            } else if cask {
                "Casks may self-update · security not assessed"
            } else {
                "Security not assessed"
            },
            regions[2].width as usize,
        ))
        .style(muted()),
        regions[2],
    );
}
