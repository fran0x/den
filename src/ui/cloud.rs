//! The Cloud area: AWS profiles and Kubernetes contexts, with the same layout as Agents.
use super::*;
use crate::app::CloudSection;

pub(super) const HELP: &str = "Cloud › AWS: profiles from ~/.aws/config and ~/.aws/credentials\n\
One row per profile, then each SSO session. Auth says how the profile gets credentials: Role, SSO, Process, Static keys or None set. Status is Ready (with the region) or Problem (with what is missing).\n\
The panel shows the region, account and role, the role chain and where the profile is defined. AWS_PROFILE marks the active profile.\n\
Privacy: from the credentials file only profile and key names are kept; values are discarded as they are read. SSO start URLs, endpoints, external IDs and credential_process commands are withheld. SSO and CLI caches are never read. A credentials file other users can read is a Problem.\n\
Nothing is called: no AWS API, SSO login or credential process, so whether credentials work is not checked.\n\
/ searches; PgUp/PgDn scroll the panel; Shift+Tab switches to Kubernetes; r refreshes.";

pub(super) const KUBE_HELP: &str = "Cloud › Kubernetes: contexts from KUBECONFIG, or ~/.kube/config\n\
One row per context. Auth says how its user signs in: Exec (with the program name), an auth provider, Client cert, Token, Basic auth or None set. Status is Ready (current marks current-context, then the namespace) or Problem: a missing cluster or user, TLS verification off, plain http, a stored password, a removed auth provider, or a file other users can read.\n\
With several KUBECONFIG files, for each name the first file wins, as in kubectl.\n\
Privacy: tokens, passwords, certificate and key data are never kept, only whether they are set. Server hosts are withheld (scheme, kind of address and port are shown). Exec plugins show the program and env variable names; arguments and values are withheld.\n\
Nothing is called: no cluster, kubectl or exec plugin, so whether access works is not checked.\n\
/ searches; PgUp/PgDn scroll the panel; Shift+Tab switches to AWS; r refreshes.";

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
                .filter(|r| r.kind == crate::aws::Kind::Profile)
                .count();
            let problems = i
                .rows
                .iter()
                .filter(|r| r.status.starts_with("Problem"))
                .count();
            format!(
                "{} · {} · {} · active: {}",
                crate::inventory::count(profiles, "profile"),
                crate::inventory::count(i.sso_sessions, "SSO session"),
                crate::inventory::count(problems, "problem"),
                i.active.as_deref().map_or_else(
                    || "default (AWS_PROFILE not set)".into(),
                    |p| format!("{} (AWS_PROFILE)", safe_text(p))
                )
            )
        }
    };
    let coverage = a
        .inventory
        .as_ref()
        .map_or(String::new(), |i| i.coverage.join("\n"));
    let text = a.selected_row().map_or_else(
        || {
            if !a.query.is_empty() {
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
        |r| format!("{HEADING}{}\n\n{HEADING}Coverage\n{coverage}", r.detail),
    );
    ListView {
        title: "Cloud / AWS",
        pane: " AWS ",
        header: ("Profile", "Auth", "Status"),
        // Until profiles load, the list carries the message; the status line stays quiet.
        status: if a.inventory.is_some() {
            message
        } else {
            String::new()
        },
        rows: a
            .rows()
            .iter()
            .map(|r| (r.name.clone(), r.auth.clone(), r.status.clone()))
            .collect(),
        text,
        selected: a.selected,
        query: a.query.clone(),
        searching: a.searching,
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
            let problems = i
                .rows
                .iter()
                .filter(|r| r.status.starts_with("Problem"))
                .count();
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
    let text = k.selected_row().map_or_else(
        || {
            if !k.query.is_empty() {
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
        |r| format!("{HEADING}{}\n\n{HEADING}Coverage\n{coverage}", r.detail),
    );
    ListView {
        title: "Cloud / Kubernetes",
        pane: " Kubernetes ",
        header: ("Context", "Auth", "Status"),
        status: if k.inventory.is_some() {
            message
        } else {
            String::new()
        },
        rows: k
            .rows()
            .iter()
            .map(|r| (r.name.clone(), r.auth.clone(), r.status.clone()))
            .collect(),
        text,
        selected: k.selected,
        query: k.query.clone(),
        searching: k.searching,
    }
}
