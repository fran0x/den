//! The skill panel shared by both tools in the Agents area.
use super::*;
use crate::skills::{Validity, state::State};

/// One skill's panel: summary, then Findings, Instructions, Supporting files (listed,
/// not opened) and Evidence.
pub(super) fn skill_panel(skill: &crate::skills::Skill, summary: &[String], agent: &str) -> String {
    let mut text = format!("{HEADING}{}\n{}\n", skill.name, skill.description);
    for line in summary {
        text.push_str(&format!("\n{line}"));
    }
    text.push_str(&format!(
        "\nNot shown: whether {agent} actually loaded or used it"
    ));
    text.push_str(&format!(
        "\n\n{HEADING}Findings ({})\n",
        skill.findings.len()
    ));
    text.push_str(&if skill.findings.is_empty() {
        "None in the inspected data; security not assessed.".into()
    } else {
        skill
            .findings
            .iter()
            .map(|f| format!("{:?}: {}", f.severity, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    });
    text.push_str(&format!(
        "\n\n{HEADING}Instructions (data, never followed)\n"
    ));
    text.push_str(if skill.instructions.is_empty() {
        "No readable instruction body; see Findings."
    } else {
        &skill.instructions
    });
    text.push_str(&format!(
        "\n\n{HEADING}Supporting files ({})\n",
        skill.files.len()
    ));
    for f in &skill.files {
        text.push_str(&format!("{} [{}] {}\n", f.path.display(), f.kind, f.note));
    }
    text.push_str(&format!(
        "\n{HEADING}Evidence\nLocation: {}\nResolved: {}\nMetadata: {} · optional metadata: {}\nDirectory accessible: {} · duplicate name: {}\n{} executable files; {} script-directory entries\ne: configuration source, when one applies",
        skill.path.display(),
        skill
            .resolved
            .as_ref()
            .map_or("Unresolved".into(), |p| p.display().to_string()),
        skill.metadata.label(),
        skill.optional.label(),
        skill.accessible,
        skill.duplicate,
        skill.files.iter().filter(|f| f.executable).count(),
        skill
            .files
            .iter()
            .filter(|f| f.path.starts_with("scripts"))
            .count()
    ));
    if !skill.mcp.is_empty() {
        text.push_str("\n\nDeclared MCP dependencies (unresolved; never connected):");
        for dep in &skill.mcp {
            text.push_str(&format!(
                "\n{} · {} · {}{}",
                dep.value,
                dep.transport.as_deref().unwrap_or("transport not declared"),
                dep.url.as_deref().unwrap_or("URL not declared"),
                dep.description
                    .as_deref()
                    .map(|d| format!("\n{d}"))
                    .unwrap_or_default()
            ));
        }
    }
    text
}

/// Codex's skill discovery coverage.
pub(super) fn coverage(state: &State) -> String {
    let mut text = state.inventory.coverage.join("\n\n");
    if let Some(c) = &state.config {
        text = format!("Project context: {}\n\n{text}", c.project.display());
    }
    let count = |v: Validity| {
        state
            .inventory
            .skills
            .iter()
            .filter(|s| s.metadata == v)
            .count()
    };
    text.push_str(&format!(
        "\n\n{} discovered; {} valid; {} invalid; {} not evaluated.\n",
        state.inventory.skills.len(),
        count(Validity::Valid),
        count(Validity::Invalid),
        count(Validity::NotEvaluated)
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
    text
}
