//! The Cloud area: AWS profiles and Kubernetes contexts, with the same layout as Agents.
use super::*;
use crate::app::CloudSection;

pub(super) const HELP: &str = "AWS: profiles from ~/.aws\n\
* active profile. Credential values discarded; SSO caches never read.\n\
Nothing is called, so credentials are not tested.";

pub(super) const KUBE_HELP: &str = "Kubernetes: contexts from KUBECONFIG or ~/.kube/config\n\
* current context. Tokens, keys and server hosts withheld.\n\
Nothing is called, so access is not tested.";

pub(super) fn render(frame: &mut Frame, app: &mut App) {
    match app.cloud {
        CloudSection::Aws => {
            let view = aws(app);
            let a = &mut app.aws;
            list_view(frame, view, &mut a.page, &mut a.panel);
        }
        CloudSection::Kubernetes => {
            let view = kube(app);
            let k = &mut app.kube;
            list_view(frame, view, &mut k.page, &mut k.panel);
        }
    }
}

fn aws(app: &App) -> ListView {
    let a = &app.aws;
    let message = match (&a.error, &a.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if a.context.is_none() => "AWS not configured in this mode".into(),
        (None, None) => "Collecting AWS profiles...".into(),
        (None, Some(i)) if !i.found => String::new(),
        (None, Some(i)) => {
            let profiles = i
                .rows
                .iter()
                .filter(|r| r.kind == crate::areas::cloud::aws::Kind::Profile)
                .count();
            let problems = i.rows.iter().filter(|r| r.status.is_problem()).count();
            format!(
                "{} · {} · {} · active: {}",
                crate::inventory::count(profiles, "profile"),
                crate::inventory::count(i.sso_sessions, "SSO session"),
                crate::inventory::count(problems, "problem"),
                i.active.label()
            )
        }
    };
    let coverage = a
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.coverage.join("\n"));
    let tracked = tracked(a);
    let now = std::time::SystemTime::now();
    let text = a.selected_row().map_or_else(
        || {
            if let Some(empty) = &tracked.empty {
                format!("{empty}\n\n{HEADING}Coverage\n{coverage}")
            } else if !a.query.is_empty() {
                "No matches.\n/:search".into()
            } else if let Some(i) = &a.inventory {
                format!(
                    "{}\n\n{HEADING}Coverage\n{coverage}",
                    if i.found {
                        "No AWS profiles found."
                    } else {
                        "AWS not set up here: no config or credentials file."
                    },
                )
            } else {
                message.clone()
            }
        },
        |r| {
            format!(
                "{HEADING}{}{}\n\n{HEADING}Coverage\n{coverage}",
                r.detail,
                a.changes.panel("AWS", &r.key, now)
            )
        },
    );
    // The active profile is starred and highlighted: a narrow Status column can cut "active".
    let active = a
        .inventory
        .as_ref()
        .and_then(|i| i.active.profile())
        .map(|p| format!("profile:{p}"));
    ListView {
        title: "Cloud / AWS",
        pane: " AWS ",
        header: ("  Profile", "Auth", "Status"),
        // Until profiles load, the list carries the message; the status line stays quiet.
        status: if a.inventory.is_some() {
            format!("{message}{}", tracked.status)
        } else {
            String::new()
        },
        rows: a
            .rows()
            .iter()
            .map(|r| {
                let name = mark(&r.name, active.as_ref() == Some(&r.key));
                let name = format!("{}{name}", a.changes.mark("AWS", &r.key));
                (name, r.auth.clone(), r.status.to_string())
            })
            .collect(),
        current: a
            .rows()
            .iter()
            .position(|r| active.as_ref() == Some(&r.key)),
        text,
        selected: a.selected(),
        query: a.query.clone(),
        searching: a.searching,
        actions: false,
        filter: tracked.filter,
        removed: tracked.removed,
    }
}

fn kube(app: &App) -> ListView {
    let k = &app.kube;
    let message = match (&k.error, &k.inventory) {
        (Some(e), _) => e.clone(),
        (None, None) if k.context.is_none() => "Kubernetes not configured in this mode".into(),
        (None, None) => "Collecting Kubernetes contexts...".into(),
        (None, Some(i)) if !i.found => String::new(),
        (None, Some(i)) => {
            let problems = i.rows.iter().filter(|r| r.status.is_problem()).count();
            format!(
                "{} · {} · current: {}",
                crate::inventory::count(i.contexts, "context"),
                crate::inventory::count(problems, "problem"),
                i.current
                    .as_deref()
                    .map_or_else(|| "none".into(), safe_text)
            )
        }
    };
    let coverage = k
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.coverage.join("\n"));
    let tracked = tracked(k);
    let now = std::time::SystemTime::now();
    let text = k.selected_row().map_or_else(
        || {
            if let Some(empty) = &tracked.empty {
                format!("{empty}\n\n{HEADING}Coverage\n{coverage}")
            } else if !k.query.is_empty() {
                "No matches.\n/:search".into()
            } else if let Some(i) = &k.inventory {
                let files = k.context.as_ref().map_or_else(String::new, |c| {
                    c.files
                        .iter()
                        .map(|p| c.short(p))
                        .collect::<Vec<_>>()
                        .join(" or ")
                });
                format!(
                    "{}\n\n{HEADING}Coverage\n{coverage}",
                    if i.found {
                        "No Kubernetes contexts found.".into()
                    } else {
                        format!("Kubernetes not set up here: no {files}.")
                    },
                )
            } else {
                message.clone()
            }
        },
        |r| {
            format!(
                "{HEADING}{}{}\n\n{HEADING}Coverage\n{coverage}",
                r.detail,
                k.changes.panel("Kubernetes", &r.key, now)
            )
        },
    );
    let current = k
        .inventory
        .as_ref()
        .and_then(|i| i.current.as_ref())
        .map(|c| format!("context:{c}"));
    ListView {
        title: "Cloud / Kubernetes",
        pane: " Kubernetes ",
        header: ("  Context", "Auth", "Status"),
        status: if k.inventory.is_some() {
            format!("{message}{}", tracked.status)
        } else {
            String::new()
        },
        rows: k
            .rows()
            .iter()
            .map(|r| {
                let name = mark(&r.name, current.as_ref() == Some(&r.key));
                let name = format!("{}{name}", k.changes.mark("Kubernetes", &r.key));
                (name, r.auth.clone(), r.status.to_string())
            })
            .collect(),
        current: k
            .rows()
            .iter()
            .position(|r| current.as_ref() == Some(&r.key)),
        text,
        selected: k.selected(),
        query: k.query.clone(),
        searching: k.searching,
        actions: false,
        filter: tracked.filter,
        removed: tracked.removed,
    }
}

/// Marks the profile or context in use with `*`, as kubectl does: a narrow Status column
/// can cut the word "active" or "current".
fn mark(name: &str, in_use: bool) -> String {
    format!("{} {name}", if in_use { "*" } else { " " })
}
