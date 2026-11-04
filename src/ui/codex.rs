use super::*;
use crate::codex::state::{State, View};

pub(super) const HELP: &str = "Codex local evidence\nF3: Skills / Configuration / Instructions / MCP\nConfiguration: s toggles settings/sources; Enter in inspector opens source\nInstructions: Enter in inspector toggles redacted preview; t toggles if-trusted scenario\nMCP: 1–6 Overview / Transport / Authentication / Tools / Sources / Findings\nf selects filter dimension; i cycles value; V selects source; e or Enter in inspector opens source\nBackspace returns to setting, skill or MCP server\nr refreshes configuration, skills and MCP together\n\nRaw TOML is never displayed. CLI, managed policies and live state are unobserved.";

pub(super) fn render(frame: &mut Frame, state: &mut State, detailed: bool, inspector: bool) {
    let view = match state.view {
        View::Mcp => "MCP",
        View::Configuration => "Configuration",
        View::Instructions => "Instructions",
        View::Skills => "Skills",
    };
    let query = format!(
        " /{}{}{}",
        if state.searching { " " } else { " Search: " },
        safe_text(&state.query),
        if state.view == View::Mcp {
            format!("   {}", state.mcp.label())
        } else {
            String::new()
        }
    );
    let body = chrome(
        frame,
        &format!("Codex / {view}"),
        &query,
        &state.status(),
        state.searching,
        detailed,
        &actions(
            state.focus(),
            inspector,
            match state.view {
                View::Instructions => "Enter:preview",
                View::Mcp => "Enter:source",
                View::Configuration if !state.sources => "Enter:source",
                _ => "",
            },
        ),
    );
    let panes = panes(body, state.focus(), inspector);
    state.clamp();
    let entries = state.rows();
    if panes[0].width > 0 {
        let title = match state.view {
            View::Mcp => " MCP inventory ",
            View::Instructions => " Instruction chain ",
            _ if state.sources => " Sources ",
            _ => " Settings ",
        };
        let block = pane(title, !state.focus_details);
        let inner = block.inner(panes[0]);
        frame.render_widget(block, panes[0]);
        let page = inner.height.max(1) as usize;
        let selected = state.selected[state.slot()];
        let start = (selected / page) * page;
        let items: Vec<_> = entries
            .iter()
            .skip(start)
            .take(page)
            .map(|(n, label)| {
                let line = if let Some(i) = &state.inventory {
                    match state.view {
                        View::Mcp => {
                            let server = &i.configuration.mcp.servers[*n];
                            columns(
                                &server.name,
                                server.transport().label(),
                                server.status.label(),
                                inner.width,
                            )
                        }
                        View::Instructions => {
                            let f = &i.instructions.files;
                            let chain = state.chain().map_or(f, |c| &c.files);
                            let f = &chain[*n];
                            columns(
                                &f.path.file_name().unwrap_or_default().to_string_lossy(),
                                if f.global { "Global" } else { "Project" },
                                &format!("{:?}", f.status),
                                inner.width,
                            )
                        }
                        View::Configuration if state.sources => {
                            let l = &i.configuration.layers[*n];
                            columns(
                                &format!("{:?}", l.scope),
                                &l.status,
                                &format!("{:?}", l.activity),
                                inner.width,
                            )
                        }
                        _ => Line::from(fit(label, inner.width.saturating_sub(2) as usize)),
                    }
                } else {
                    Line::from(fit(label, inner.width.saturating_sub(2) as usize))
                };
                ListItem::new(line)
            })
            .collect();
        let mut selection_state =
            ListState::default().with_selected((!entries.is_empty()).then_some(selected - start));
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol("> ")
                .highlight_style(selection()),
            inner,
            &mut selection_state,
        );
        if entries.is_empty() {
            frame.render_widget(Paragraph::new("No matching entries"), inner);
        }
        if !state.focus_details {
            state.page = page;
        }
    }
    if panes[1].width == 0 {
        return;
    }
    let title = if state.view == View::Mcp {
        format!(
            " Section {}: {} ",
            state.mcp.section + 1,
            [
                "Overview",
                "Transport",
                "Authentication",
                "Tools and approvals",
                "Sources",
                "Findings and limitations"
            ][state.mcp.section]
        )
    } else {
        " Overview / local evidence ".into()
    };
    let block = pane(&title, state.focus_details);
    let inner = block.inner(panes[1]);
    frame.render_widget(block, panes[1]);
    let text = if detailed
        || state.sources
        || state.preview
        || (state.view == View::Mcp && state.mcp.section != 0)
    {
        state.text()
    } else {
        overview(state)
    };
    let lines: Vec<_> = text.lines().map(|s| Line::from(safe_text(s))).collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    state.scroll_max = scroll_max(&paragraph, inner);
    state.scroll = state.scroll.min(state.scroll_max);
    if state.focus_details {
        state.page = inner.height.max(1) as usize;
    }
    frame.render_widget(paragraph.scroll((state.scroll, 0)), inner);
}

fn overview(state: &State) -> String {
    let Some(i) = &state.inventory else {
        return state.status();
    };
    let rows = state.rows();
    let Some((n, _)) = rows.get(state.selected[state.slot()]) else {
        return "No matching entries".into();
    };
    match state.view {
        View::Mcp => {
            let s = &i.configuration.mcp.servers[*n];
            format!(
                "{}\n\n{} · {}\nValidation: {}\n{} configuration definitions\nRuntime: not checked\n\n2 Transport   3 Authentication\n4 Tools and approvals   5 Sources\n6 Findings   e Configuration source\nv Technical evidence",
                s.name,
                s.transport().label(),
                s.status.label(),
                s.validation(),
                s.definitions.len()
            )
        }
        View::Configuration => {
            let s = &i.configuration.settings[*n];
            let source = s
                .source
                .and_then(|n| i.configuration.layers.get(n))
                .map_or("Documented default / unresolved".into(), |l| {
                    format!("{:?} configuration", l.scope)
                });
            format!(
                "{}\n\nLocally resolved: {}\nSource: {}\n{} definitions{}\n\nCLI overrides and managed policies unobserved.\nEnter in inspector: source   s: sources\nv: precedence and evidence",
                s.key,
                s.value,
                source,
                s.definitions.len(),
                if s.conditional.is_some() {
                    " · conditional definition present"
                } else {
                    ""
                }
            )
        }
        View::Instructions => {
            let Some(c) = state.chain() else {
                return "No instruction chain".into();
            };
            let f = &c.files[*n];
            format!(
                "{}\n\n{} · {:?}\nIncluded: {} / {} file bytes\n\nChain: {} selected files · {} / {} byte limit\n{}\n\nEnter in inspector: redacted preview\nv: paths and selection evidence\nt: conditional trusted scenario",
                f.path.file_name().unwrap_or_default().to_string_lossy(),
                if f.global { "Global" } else { "Project" },
                f.status,
                f.included,
                f.bytes,
                c.selected(),
                c.included_bytes,
                c.limit,
                if c.uncertain {
                    "Instruction chain conditional / uncertain"
                } else {
                    "Instructions combine in directory order"
                }
            )
        }
        View::Skills => String::new(),
    }
}
