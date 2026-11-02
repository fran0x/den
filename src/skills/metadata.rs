use super::*;
use serde_json::Value;

fn yaml(text: &str) -> Result<Value, ()> {
    if text.len() > 64 * 1024 {
        return Err(());
    }
    let options = serde_saphyr::options! {
        reject_unsupported_tags: true,
        budget: serde_saphyr::budget! {max_depth: 32,max_events: 20_000,max_total_scalar_bytes: 128*1024, max_aliases: 128, max_nodes: 10_000},
    };
    // No includes, property substitution, callbacks or executable tags are enabled.
    serde_saphyr::from_str_with_options(text, options).map_err(|_| ())
}
pub fn instructions(skill: &mut Skill, text: String) {
    skill.instructions = text.clone();
    skill.metadata = Validity::Invalid;
    let normalized = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines = normalized.split_inclusive('\n');
    let first = lines.next();
    if first.map(|s| s.trim_end_matches(['\r', '\n'])) != Some("---") {
        skill.finding(
            Severity::Error,
            "SKILL.md requires YAML frontmatter beginning with ---",
        );
        return;
    }
    let mut front = String::new();
    let mut offset = first.unwrap_or("").len();
    let mut closed = false;
    for line in lines {
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']) == "---" {
            closed = true;
            break;
        }
        front.push_str(line);
    }
    if !closed {
        skill.finding(Severity::Error, "Frontmatter closing --- is missing");
        return;
    }
    skill.instructions = normalized.get(offset..).unwrap_or("").to_owned();
    let Ok(value) = yaml(&front) else {
        skill.finding(
            Severity::Error,
            "Invalid YAML frontmatter or parser budget exceeded",
        );
        return;
    };
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 256);
    let desc = value
        .get("description")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 8192);
    if let Some(name) = name {
        skill.name = name.into();
        skill.declared_name = Some(name.into());
    } else {
        skill.finding(
            Severity::Error,
            "Required name must be a nonempty string (max 256 bytes)",
        );
    }
    if let Some(desc) = desc {
        skill.description = desc.into();
    } else {
        skill.finding(
            Severity::Error,
            "Required description must be a nonempty string (max 8192 bytes)",
        );
    }
    if name.is_some() && desc.is_some() {
        skill.metadata = Validity::Valid;
    }
}
pub fn optional(skill: &mut Skill, text: &str) {
    skill.optional = Validity::Invalid;
    let Ok(value) = yaml(text) else {
        skill.finding(
            Severity::Warning,
            "Invalid agents/openai.yaml or parser budget exceeded",
        );
        return;
    };
    let Some(map) = value.as_object() else {
        skill.finding(Severity::Warning, "agents/openai.yaml must be a mapping");
        return;
    };
    let mut valid = true;
    if let Some(interface) = map.get("interface") {
        if let Some(object) = interface.as_object() {
            for key in [
                "display_name",
                "short_description",
                "icon_small",
                "icon_large",
                "brand_color",
                "default_prompt",
            ] {
                if object.get(key).is_some_and(|v| !v.is_string()) {
                    valid = false;
                }
            }
        } else {
            valid = false;
        }
    }
    if let Some(policy) = map.get("policy") {
        valid &= policy.is_object()
            && policy
                .get("allow_implicit_invocation")
                .is_none_or(Value::is_boolean);
    }
    if let Some(deps) = map.get("dependencies") {
        if !deps.is_object() {
            valid = false;
        }
        if let Some(tools) = deps.get("tools") {
            if let Some(tools) = tools.as_array().filter(|t| t.len() <= 128) {
                for tool in tools {
                    let kind = tool.get("type").and_then(Value::as_str);
                    let name = tool
                        .get("value")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty());
                    let strings = ["description", "transport", "url"]
                        .iter()
                        .all(|k| tool.get(k).is_none_or(Value::is_string));
                    if kind.is_none() || name.is_none() || !strings {
                        valid = false;
                        continue;
                    }
                    if kind == Some("mcp") {
                        skill.mcp.push(McpDependency {
                            value: name.unwrap_or_default().into(),
                            description: tool
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            transport: tool
                                .get("transport")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            url: tool.get("url").and_then(Value::as_str).map(str::to_owned),
                        });
                    }
                }
            } else {
                valid = false;
            }
        }
    }
    if valid {
        skill.optional = Validity::Valid;
    } else {
        skill.finding(
            Severity::Warning,
            "Invalid known field types in agents/openai.yaml; declarations are unverified",
        );
    }
    if let Some(interface) = map.get("interface") {
        for key in ["icon_small", "icon_large"] {
            if let Some(path) = interface.get(key).and_then(Value::as_str) {
                check_reference(skill, path);
            }
        }
    }
}
pub fn references(skill: &mut Skill) {
    // Only simple inline Markdown links, with no escapes/titles/URLs, are reliable here.
    let text = skill.instructions.clone();
    let mut fence: Option<(char, usize)> = None;
    let mut references = 0;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let first = trimmed.chars().next();
        if matches!(first, Some('`' | '~')) {
            let marker = first.unwrap_or('`');
            let length = trimmed.chars().take_while(|&c| c == marker).count();
            if length >= 3 {
                match fence {
                    None => fence = Some((marker, length)),
                    Some((ch, n)) if ch == marker && length >= n => fence = None,
                    _ => {}
                }
                continue;
            }
        }
        // Deliberately skip code examples and complicated link syntax; this is
        // a conservative file-reference check, not a Markdown interpreter.
        if fence.is_some() || line.starts_with("    ") || line.contains('`') {
            continue;
        }
        for tail in line.split("](").skip(1) {
            if references >= 512 {
                return;
            }
            references += 1;
            if let Some((target, _)) = tail.split_once(')') {
                if target.contains([' ', '\n', '\\', '(', '%'])
                    || target.starts_with('#')
                    || target.contains(':')
                {
                    continue;
                }
                let path = target.split('#').next().unwrap_or("");
                if !path.is_empty() {
                    check_reference(skill, path);
                }
            }
        }
    }
}
fn check_reference(skill: &mut Skill, target: &str) {
    use std::path::Component;
    let path = std::path::Path::new(target);
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(p) => normalized.push(p),
            _ => {
                skill.finding(
                    Severity::Info,
                    format!("Reference outside skill boundary not inspected: {target}"),
                );
                return;
            }
        }
    }
    if !skill.files.iter().any(|f| f.path == normalized) {
        skill.finding(
            Severity::Warning,
            format!("Referenced file not observed in bounded file inventory: {target}"),
        );
    }
}
