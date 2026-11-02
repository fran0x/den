use super::*;
use crate::skills::{
    Validity,
    state::{Section, State},
};

const HELP: &str = "Codex Skills — read-only\n\nF2: Homebrew / Skills (keeps each view's state)\n/: search name and description\nf: scope filter; i: validation/findings filter\nTab: switch panes; arrows/j/k: navigate or scroll\n1: Overview; 2: Instructions; 3: Files; 4: Findings; 5: Coverage\nEnter in Files: inspect the selected file as text\nBackspace: return from a file; otherwise focus inventory\nPageUp/PageDown, Home/End: scroll\nr: refresh configured roots; Esc: reset filters\n?: help; q/Ctrl-C: quit\n\nSkills and scripts are inspected as data, never executed. MCP declarations remain unresolved. F3 switches Codex views; e opens enablement provenance. Discovered does not mean enabled, loaded or invoked.\n\nFilesystem coverage and limits are listed under 5. Legacy and system-cache locations do not establish activation. Plugin/session skills may be absent from this inventory.";

pub(super) fn render(frame: &mut Frame, state: &mut State) {
    let area = frame.area();
    if area.width < 30 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Den: resize to 30 x 10\nF2: module · q:quit"),
            area,
        );
        return;
    }
    let compact = area.height < 14;
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(if compact { 1 } else { 2 }),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(
        Paragraph::new(" DEN / Codex / Skills | F2: Homebrew · F3: Configuration")
            .style(Style::default().fg(Color::Cyan)),
        rows[0],
    );
    let query = format!(
        "{}: {}\nScope: {} · Status: {}",
        if state.searching { "Search*" } else { "Search" },
        safe_text(&state.query),
        state.scope.map_or("All", |s| s.label()),
        state.filter.map_or("All", |s| s.label())
    );
    frame.render_widget(Paragraph::new(query), rows[1]);
    frame.render_widget(
        Paragraph::new(safe_text(&state.status())).style(Style::default().fg(Color::Yellow)),
        rows[2],
    );
    let status = if state.searching {
        "EDIT | Enter:done Esc:done Ctrl-U:clear"
    } else {
        "F2:module F3:Codex e:config 1-5:view /:search f:scope i:status r:refresh ?:help q:quit"
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        rows[4],
    );
    if state.help {
        paragraph(frame, state, rows[3], " Help ", HELP.to_owned());
        return;
    }
    let panes = Layout::default()
        .direction(if area.width >= 80 {
            Direction::Horizontal
        } else {
            Direction::Vertical
        })
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(rows[3]);
    let block = pane(" Skills inventory ", state.focus == Focus::Inventory);
    let inner = block.inner(panes[0]);
    frame.render_widget(block, panes[0]);
    let page = inner.height.max(1) as usize;
    if state.focus == Focus::Inventory {
        state.page = page;
    }
    let start = (state.selected.unwrap_or(0) / page) * page;
    let items: Vec<_> = state
        .visible
        .iter()
        .skip(start)
        .take(page)
        .map(|&i| {
            let s = &state.inventory.skills[i];
            ListItem::new(safe_text(&format!(
                "{} [{}] [{}] {}",
                s.name,
                s.scope.label(),
                state
                    .enablement
                    .get(i)
                    .map_or("Not evaluated", |e| e.label.as_str()),
                s.description
            )))
        })
        .collect();
    let mut selected = ListState::default().with_selected(state.selected.map(|n| n - start));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("> ")
            .highlight_style(Style::default().fg(Color::Cyan)),
        inner,
        &mut selected,
    );
    if state.section == Section::Coverage {
        let mut text = state.inventory.coverage.join("\n\n");
        if let Some(c) = &state.config {
            text = format!("Project context: {}\n\n{text}", c.project.display());
        }
        text.push_str(&format!(
            "\n\n{} discovered; {} valid; {} invalid; {} not evaluated.\n",
            state.inventory.skills.len(),
            state
                .inventory
                .skills
                .iter()
                .filter(|s| s.metadata == Validity::Valid)
                .count(),
            state
                .inventory
                .skills
                .iter()
                .filter(|s| s.metadata == Validity::Invalid)
                .count(),
            state
                .inventory
                .skills
                .iter()
                .filter(|s| s.metadata == Validity::NotEvaluated)
                .count()
        ));
        for scope in crate::skills::Scope::ALL {
            text.push_str(&format!(
                "{}: {}\n",
                scope.label(),
                state
                    .inventory
                    .skills
                    .iter()
                    .filter(|s| s.scope == scope)
                    .count()
            ));
        }
        for location in &state.inventory.locations {
            text.push_str(&format!(
                "\n{} · {}\n{}\n",
                location.scope.label(),
                location.path.display(),
                location.status
            ));
        }
        if state.config.is_none() {
            text = state.status();
        }
        paragraph(frame, state, panes[1], " Discovery coverage ", text);
        return;
    }
    let Some(skill) = state.selected_skill() else {
        paragraph(
            frame,
            state,
            panes[1],
            " Details ",
            "No matching skills. /:search · Esc:reset · 5:coverage".into(),
        );
        return;
    };
    let title = match state.section {
        Section::Overview => " 1 Overview ",
        Section::Instructions => " 2 Instructions (data) ",
        Section::Files => " 3 Files (read-only) ",
        Section::Findings => " 4 Validation findings ",
        Section::Coverage => " Coverage ",
    };
    let text=match state.section {
        Section::Overview=>{
            let mut text=format!("{}\n\nScope: {}\nLocation: {}\nResolved: {}\nDiscovered: yes\nMetadata: {}\nDirectory accessible: {}\nOptional metadata: {}\nDuplicate name: {}\nConfiguration status: {}\nLoaded/invoked: not evaluated\n\nDescription:\n{}\n\n{} files; {} executable files; {} script-directory entries\n\nDeclared MCP dependencies (unresolved; never connected):\n",skill.name,skill.scope.label(),skill.path.display(),skill.resolved.as_ref().map_or("Unresolved".into(),|p|p.display().to_string()),skill.metadata.label(),skill.accessible,skill.optional.label(),skill.duplicate,state.selected.and_then(|n|state.visible.get(n)).and_then(|n|state.enablement.get(*n)).map_or("Not evaluated".into(),|e|format!("{} — {} (e: source)",e.label,e.explanation)),skill.description,skill.files.len(),skill.files.iter().filter(|f|f.executable).count(),skill.files.iter().filter(|f|f.path.starts_with("scripts")).count());
            for dep in &skill.mcp{text.push_str(&format!("{} · {} · {}\n{}\n",dep.value,dep.transport.as_deref().unwrap_or("transport not declared"),dep.url.as_deref().unwrap_or("URL not declared"),dep.description.as_deref().unwrap_or("")));}
            text.push_str("\n2: full instructions · 3: supporting files · 4: findings\nScripts and declarations are not a security assessment.");text
        },
        Section::Instructions=>if skill.instructions.is_empty(){"No readable instruction body; inspect validation findings.".into()}else{skill.instructions.clone()},
        Section::Findings=>if skill.findings.is_empty(){"No findings in the inspected data. See Overview for configuration status; security not assessed.".into()}else{skill.findings.iter().map(|f|format!("{:?}: {}",f.severity,f.message)).collect::<Vec<_>>().join("\n\n")},
        Section::Files if state.file_open=>skill.files.get(state.file).map_or("No file selected".into(),|f|format!("{}\n{} · executable bit: {}\n{}\n\n{}",f.path.display(),f.kind,f.executable,f.note,f.text.as_deref().unwrap_or("No text preview available. Directories are listed as relative paths; use Backspace to choose a child file."))),
        Section::Files=>{
            let block=pane(title,state.focus==Focus::Details);let inner=block.inner(panes[1]);frame.render_widget(block,panes[1]);
            let page=inner.height.max(1) as usize;let start=(state.file/page)*page;
            let items:Vec<_>=skill.files.iter().skip(start).take(page).map(|f|ListItem::new(safe_text(&format!("{} [{}] {}",f.path.display(),f.kind,f.note)))).collect();
            let mut selected=ListState::default().with_selected(Some(state.file-start));frame.render_stateful_widget(List::new(items).highlight_symbol("> ").highlight_style(Style::default().fg(Color::Cyan)),inner,&mut selected);
            if state.focus==Focus::Details{state.page=page;}return;
        },Section::Coverage=>String::new(),
    };
    paragraph(frame, state, panes[1], title, text);
}
fn paragraph(frame: &mut Frame, state: &mut State, area: Rect, title: &str, text: String) {
    let block = pane(title, state.focus == Focus::Details || state.help);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<_> = text
        .split('\n')
        .map(|line| Line::from(safe_text(line)))
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    state.scroll_max = scroll_max(&paragraph, inner);
    state.scroll = state.scroll.min(state.scroll_max);
    if state.focus == Focus::Details || state.help {
        state.page = inner.height.max(1) as usize;
    }
    frame.render_widget(paragraph.scroll((state.scroll, 0)), inner);
}
