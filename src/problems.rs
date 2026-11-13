//! The Problems overview: everything that needs attention, from every area Den has read,
//! in one list, grouped by place. Problems first, then available updates. Each entry knows where it lives,
//! so Enter can jump there.
use crate::{
    app::App,
    areas::{
        agents, docker,
        homebrew::diagnostics::{Cause, Tab},
        toolchains,
    },
    inventory::safe_text,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Problem,
    Update,
}

/// Where an entry lives, to jump there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A Homebrew package, by its index in the inventory.
    Package(usize),
    Diagnostics(Tab),
    Agents(agents::Section, String),
    Aws(String),
    Kubernetes(String),
    Docker(docker::Section, String),
    Toolchains(toolchains::Section, String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    /// "Cloud › AWS".
    pub place: String,
    pub name: String,
    pub detail: String,
    pub target: Target,
}

/// The overview's selection and scroll.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub selected: usize,
}

/// Everything that needs attention in what Den has read so far: problems, then updates.
pub fn collect(app: &App) -> Vec<Entry> {
    let mut entries = vec![];
    homebrew(app, &mut entries);
    agents(app, &mut entries);
    cloud(app, &mut entries);
    docker(app, &mut entries);
    toolchains(app, &mut entries);
    tool_updates(app, &mut entries);
    // Stable: problems keep their area order, then updates.
    entries.sort_by_key(|e| e.kind == Kind::Update);
    entries
}

/// Problems only: what the top-bar badge counts.
pub fn problems(app: &App) -> usize {
    collect(app)
        .iter()
        .filter(|e| e.kind == Kind::Problem)
        .count()
}

fn problem(place: &str, name: &str, detail: &str, target: Target) -> Entry {
    Entry {
        kind: Kind::Problem,
        place: place.into(),
        name: safe_text(name),
        detail: safe_text(detail),
        target,
    }
}

fn homebrew(app: &App, entries: &mut Vec<Entry>) {
    // Only missing runtime dependencies are problems. Absent build, test or optional
    // dependencies and references to a version since upgraded are normal.
    let missing: usize = app
        .health
        .groups
        .iter()
        .filter(|((cause, _), _)| matches!(cause, Cause::AbsentRecorded | Cause::AbsentCurrent))
        .map(|(_, items)| items.len())
        .sum();
    if missing > 0 {
        entries.push(problem(
            "Homebrew › Diagnostics",
            "Missing dependencies",
            &format!(
                "{} that installed packages need at run time {} not installed",
                crate::inventory::count(missing, "dependency"),
                if missing == 1 { "is" } else { "are" }
            ),
            Target::Diagnostics(Tab::Unresolved),
        ));
    }
    for (index, resource) in app.inventory.resources.iter().enumerate() {
        if let Some(latest) = resource.homebrew.as_ref().and_then(|m| m.update()) {
            entries.push(Entry {
                kind: Kind::Update,
                place: "Homebrew › Packages".into(),
                name: safe_text(&resource.display_name()),
                detail: format!(
                    "↑ {} available · installed {}",
                    safe_text(latest),
                    safe_text(resource.version.as_deref().unwrap_or("unknown"))
                ),
                target: Target::Package(index),
            });
        }
    }
}

fn agents(app: &App, entries: &mut Vec<Entry>) {
    let sources = app.agent_sources();
    for section in agents::Section::ALL {
        for row in sources.rows(section) {
            let tools = [
                ("Codex", sources.codex_status(section, &row)),
                ("Claude Code", sources.claude_status(section, &row)),
            ];
            let details: Vec<String> = tools
                .into_iter()
                .filter_map(|(tool, status)| {
                    let (status, why) = status?;
                    (status == agents::Status::Problem).then(|| match (why.is_empty(), section) {
                        (false, _) => format!("{tool}: {why}"),
                        (true, agents::Section::Instructions) => {
                            format!("{tool}: an instruction file could not be read")
                        }
                        (true, _) => format!("{tool}: Problem"),
                    })
                })
                .collect();
            if !details.is_empty() {
                entries.push(problem(
                    &format!("Agents › {}", section.label()),
                    &row.name,
                    &details.join(" · "),
                    Target::Agents(section, row.key.clone()),
                ));
            }
        }
    }
}

fn cloud(app: &App, entries: &mut Vec<Entry>) {
    if let Some(i) = &app.aws.inventory {
        for row in i.rows.iter().filter(|r| r.status.is_problem()) {
            entries.push(problem(
                "Cloud › AWS",
                &row.name,
                &row.status.detail,
                Target::Aws(row.key.clone()),
            ));
        }
    }
    if let Some(i) = &app.kube.inventory {
        for row in i.rows.iter().filter(|r| r.status.is_problem()) {
            entries.push(problem(
                "Cloud › Kubernetes",
                &row.name,
                &row.status.detail,
                Target::Kubernetes(row.key.clone()),
            ));
        }
    }
}

fn docker(app: &App, entries: &mut Vec<Entry>) {
    let Some(i) = &app.docker.inventory else {
        return;
    };
    for row in i
        .containers
        .iter()
        .filter(|r| matches!(r.state.as_str(), "Dead" | "Restarting"))
    {
        entries.push(problem(
            "Docker › Containers",
            &row.name,
            &row.state,
            Target::Docker(docker::Section::Containers, row.key.clone()),
        ));
    }
    if i.unreadable > 0 {
        entries.push(problem(
            "Docker",
            "Unreadable output",
            &format!(
                "{} from docker could not be read, so counts may be low",
                crate::inventory::count(i.unreadable, "line")
            ),
            Target::Docker(docker::Section::Containers, String::new()),
        ));
    }
}

fn toolchains(app: &App, entries: &mut Vec<Entry>) {
    let Some(i) = &app.toolchains.inventory else {
        return;
    };
    for section in [
        toolchains::Section::Path,
        toolchains::Section::Rust,
        toolchains::Section::Packages,
    ] {
        for row in i.rows(section).iter().filter(|r| r.status.is_problem()) {
            entries.push(problem(
                &format!("Toolchains › {}", section.label()),
                &row.name,
                &row.status.detail,
                Target::Toolchains(section, row.key.clone()),
            ));
        }
    }
}

/// Codex's own update check knows of versions Homebrew may not flag (casks that update
/// themselves). Homebrew's own updates are already listed above.
fn tool_updates(app: &App, entries: &mut Vec<Entry>) {
    let own = app
        .codex
        .inventory
        .as_ref()
        .and_then(|i| i.update_check.as_ref())
        .map(|c| c.latest.as_str());
    let Some(latest) = crate::updates::tool(&app.inventory, &["codex"], own) else {
        return;
    };
    let Some((index, resource)) = app
        .inventory
        .resources
        .iter()
        .enumerate()
        .find(|(_, r)| r.name == "codex")
    else {
        return;
    };
    if resource
        .homebrew
        .as_ref()
        .and_then(|m| m.update())
        .is_none()
    {
        entries.push(Entry {
            kind: Kind::Update,
            place: "Agents".into(),
            name: "Codex".into(),
            detail: format!(
                "↑ {} available (Codex's own check) · installed {}",
                safe_text(&latest),
                safe_text(resource.version.as_deref().unwrap_or("unknown"))
            ),
            target: Target::Package(index),
        });
    }
}
