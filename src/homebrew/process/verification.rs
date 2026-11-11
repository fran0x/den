//! Opt-in comparisons against a handful of public Homebrew packages, never per-package collection.
// Test-only (`cfg(all(test, unix))`): a failed live check should stop with its message.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::{command, unix};
use crate::{
    graph::{Evidence, Graph, Provenance, Resolution},
    inventory::{HomebrewMetadata, Inventory, ResourceKind},
};
use std::{collections::BTreeSet, process::Command, sync::atomic::AtomicBool};

fn brew(args: &[&str]) -> String {
    let mut command = Command::new("brew");
    command
        .args(args)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1");
    let bytes = unix::execute(
        &mut command,
        &AtomicBool::new(false),
        unix::Limits::default(),
    )
    .expect("read-only verification command must succeed");
    String::from_utf8(bytes).expect("Homebrew command output must be UTF-8")
}
fn name(inventory: &Inventory, node: usize) -> String {
    let r = &inventory.resources[node];
    match &r.homebrew {
        Some(HomebrewMetadata::Formula {
            full_name: Some(name),
            ..
        }) => name.clone(),
        _ => r.name.clone(),
    }
}
fn reference(inventory: &Inventory, graph: &Graph, index: usize) -> String {
    match graph.edges[index].target {
        Resolution::Installed(node) => name(inventory, node),
        _ => graph.edges[index].reference.clone(),
    }
}

#[test]
#[ignore = "opt-in real Homebrew dependency comparison; no inventory is saved"]
fn live_dependency_examples() {
    let bytes = unix::execute(
        &mut command(),
        &AtomicBool::new(false),
        unix::Limits::default(),
    )
    .expect("live inventory collection");
    let inventory = super::super::parse(&bytes).expect("valid inventory");
    let graph = Graph::build(&inventory);
    let health = crate::diagnostics::Health::build(&graph);
    for ((cause, evidence), edges) in &health.groups {
        println!("{} / {evidence}: {}", cause.label(), edges.len());
    }
    assert!(!graph.truncated, "local graph must fit configured limits");
    let unresolved = graph
        .edges
        .iter()
        .filter(|e| !matches!(e.target, Resolution::Installed(_)))
        .count();
    let version_gaps = graph
        .edges
        .iter()
        .filter(|e| {
            e.provenance.evidence() == Evidence::Recorded
                && matches!(e.target, Resolution::Installed(_))
                && graph.target_state(e).is_none()
        })
        .count();
    println!(
        "Graph: {} installed nodes, {} evidence edges, {unresolved} unresolved references, {version_gaps} unresolved target-keg links",
        graph.ids.len(),
        graph.edges.len()
    );
    let samples: Vec<_> = ["git", "wget", "curl", "python@3.14", "openssl@3"]
        .into_iter()
        .filter_map(|sample| {
            inventory
                .resources
                .iter()
                .position(|r| r.kind == ResourceKind::Formula && r.name == sample)
                .map(|node| (sample, node))
        })
        .take(2)
        .collect();
    assert_eq!(
        samples.len(),
        2,
        "install two sample formulae separately before opting into this verification"
    );
    for (sample, node) in samples {
        let declared: BTreeSet<_> = graph.forward[node]
            .iter()
            .copied()
            .filter(|&i| {
                matches!(
                    graph.edges[i].provenance,
                    Provenance::Required | Provenance::Recommended
                )
            })
            .map(|i| reference(&inventory, &graph, i))
            .collect();
        let direct: BTreeSet<_> = brew(&["deps", "--direct", "--full-name", sample])
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        println!(
            "{sample}: current direct declarations {} / brew deps --direct {}; differences {}",
            declared.len(),
            direct.len(),
            declared.symmetric_difference(&direct).count()
        );
        assert!(
            declared == direct,
            "current direct declaration comparison failed; raw output withheld"
        );
        let tree = brew(&["deps", "--tree", "--full-name", sample]);
        let tree_names: BTreeSet<_> = tree
            .lines()
            .skip(1)
            .filter_map(|line| {
                line.trim_start_matches(|ch: char| {
                    ch.is_whitespace() || matches!(ch, '│' | '├' | '└' | '─')
                })
                .split_whitespace()
                .next()
                .map(str::to_owned)
            })
            .collect();
        let traversal = graph.traverse(graph.preferred(node), Evidence::Current, false);
        let descendants: BTreeSet<_> = traversal
            .nodes
            .iter()
            .filter(|s| s.node != node)
            .map(|s| name(&inventory, s.node))
            .collect();
        println!(
            "{sample}: current transitive installed nodes {} / brew deps --tree {}; differences {}",
            descendants.len(),
            tree_names.len(),
            descendants.symmetric_difference(&tree_names).count()
        );
        assert!(
            descendants == tree_names && !traversal.truncated,
            "current declaration tree comparison failed; raw output withheld"
        );
    }
    let shared = [
        "gettext",
        "pcre2",
        "zstd",
        "libunistring",
        "readline",
        "xz",
        "sqlite",
        "openssl@3",
    ]
    .into_iter()
    .filter_map(|sample| {
        inventory
            .resources
            .iter()
            .position(|r| r.name == sample && r.kind == ResourceKind::Formula)
            .map(|node| (sample, node))
    })
    .find(|(_, target)| {
        graph.reverse[*target]
            .iter()
            .filter(|&&i| graph.edges[i].provenance.evidence() == Evidence::Recorded)
            .map(|&i| graph.edges[i].from)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
    });
    if let Some((sample, target)) = shared {
        // This Homebrew command uses runtime records, including transitive entries,
        // and is not equivalent to Den's direct-only reverse list.
        let recorded: BTreeSet<_> = graph.reverse[target]
            .iter()
            .filter(|&&i| graph.edges[i].provenance.evidence() == Evidence::Recorded)
            .map(|&i| name(&inventory, graph.edges[i].from))
            .collect();
        let uses: BTreeSet<_> = brew(&["uses", "--installed", "--formula", sample])
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        println!(
            "{sample}: recorded runtime dependents (all directness) {} / brew uses --installed --formula {}; differences {}",
            recorded.len(),
            uses.len(),
            recorded.symmetric_difference(&uses).count()
        );
        assert!(
            recorded == uses,
            "recorded reverse comparison failed; raw output withheld"
        );
        assert!(recorded.len() > 1, "shared dependency example required");
        for evidence in [Evidence::Recorded, Evidence::Current] {
            let paths = graph.paths_to_requested(graph.preferred(target), evidence);
            println!(
                "{sample}: {:?} requested-root paths: {}, partial: {}",
                evidence,
                paths.paths.len(),
                paths.truncated
            );
            let public = [
                "git",
                "wget",
                "curl",
                "python@3.14",
                "openssl@3",
                "libssh2",
                "libgit2",
                "cmake",
                "node",
                "ffmpeg",
                "rust",
                "postgresql@17",
                "python@3.13",
                "python@3.15",
                "ruby",
                "gettext",
                "pcre2",
                "zstd",
                "libunistring",
                "readline",
                "xz",
                "sqlite",
                "bat",
                "eza",
                "jq",
                "ripgrep",
                "just",
                "neovim",
                "gnupg",
                "glib",
                "gobject-introspection",
                "harfbuzz",
                "graphite2",
                "pango",
                "cairo",
                "libass",
                "libarchive",
                "tesseract",
                "imagemagick",
                "openjdk",
                "watchman",
                "gts",
                "graphviz",
                "librsvg",
            ];
            for path in paths.paths.iter() {
                let mut names = vec![inventory.resources[graph.edges[path[0]].from].name.as_str()];
                for &i in path {
                    if let Resolution::Installed(node) = graph.edges[i].target {
                        names.push(inventory.resources[node].name.as_str());
                    }
                }
                if names.iter().all(|name| public.contains(name)) {
                    println!("  Public-package path: {}", names.join(" -> "));
                }
            }
        }
    } else {
        panic!("a public shared dependency must be available for this optional verification");
    }
    if let Some(root) = inventory.resources.iter().position(|r| r.name == "wget") {
        let descendants = graph.traverse(graph.preferred(root), Evidence::Current, false);
        let path = descendants
            .nodes
            .iter()
            .find_map(|target| {
                graph
                    .paths_to_requested(target.clone(), Evidence::Current)
                    .paths
                    .into_iter()
                    .find(|path| path.len() > 1 && graph.edges[path[0]].from == root)
            })
            .expect("verified wget declaration tree must contain a transitive requested-root path");
        let mut names = vec!["wget".to_owned()];
        for &index in &path {
            if let Resolution::Installed(node) = graph.edges[index].target {
                names.push(inventory.resources[node].name.clone());
            }
        }
        let public = [
            "wget",
            "libidn2",
            "libunistring",
            "openssl@3",
            "openssl@3.5",
            "openssl@3.6",
            "ca-certificates",
            "gettext",
            "pcre2",
            "libiconv",
        ];
        if names.iter().all(|name| public.contains(&name.as_str())) {
            println!(
                "Verified transitive current-declaration path: {}",
                names.join(" -> ")
            );
        } else {
            println!(
                "Verified wget transitive requested-root path: {} hops (package names withheld)",
                path.len()
            );
        }
    }
}

#[test]
#[ignore = "opt-in bounded, read-only Homebrew diagnostic reconciliation; no inventory saved"]
fn live_diagnostic_categories() {
    use crate::diagnostics::{Check, Health, Report, Status};
    let inventory = super::collect(&AtomicBool::new(false)).expect("live inventory");
    let graph = Graph::build(&inventory);
    let health = Health::build(&graph);
    assert!(!health.partial);
    println!(
        "Dependency health: {} unresolved package references; {} unresolved target-keg links",
        health.unresolved, health.keg_gaps
    );
    for ((cause, evidence), edges) in &health.groups {
        println!("  {} / {evidence}: {}", cause.label(), edges.len());
    }
    for (label, nodes) in [
        ("all evidence", &health.no_dependents),
        ("recorded runtime", &health.no_recorded_dependents),
        (
            "current required/recommended",
            &health.no_current_dependents,
        ),
    ] {
        let formulae = nodes
            .iter()
            .filter(|&&n| inventory.resources[n].kind == ResourceKind::Formula)
            .count();
        println!(
            "No observed dependents ({label}): {} packages / {formulae} formulae",
            nodes.len()
        );
    }
    for check in [Check::Missing, Check::Leaves, Check::Cleanup] {
        let report = Report::interpret(
            check,
            super::diagnose(check, &AtomicBool::new(false)),
            &inventory,
            &graph,
            0,
        );
        println!(
            "{check:?}: {}; structured references {:?}; correlated {}",
            report.status.label(),
            report.count,
            report
                .entries
                .iter()
                .filter(|e| e.package_id.is_some())
                .count()
        );
        assert!(
            matches!(report.status, Status::Informational | Status::Confirmed),
            "unexpected diagnostic result; inspect locally in Den (raw output withheld)"
        );
        if check == Check::Leaves {
            let leaves: BTreeSet<_> = report
                .entries
                .iter()
                .filter_map(|e| e.package_id.as_deref())
                .collect();
            for (label, nodes) in [
                ("all evidence", &health.no_dependents),
                ("recorded runtime", &health.no_recorded_dependents),
                (
                    "current required/recommended",
                    &health.no_current_dependents,
                ),
            ] {
                let den: BTreeSet<_> = nodes
                    .iter()
                    .filter(|&&n| inventory.resources[n].kind == ResourceKind::Formula)
                    .map(|&n| inventory.resources[n].id.as_str())
                    .collect();
                println!(
                    "  Homebrew leaves vs Den {label}: {} only Homebrew / {} only Den",
                    leaves.difference(&den).count(),
                    den.difference(&leaves).count()
                );
                let mut differences = std::collections::BTreeMap::new();
                for id in leaves.symmetric_difference(&den) {
                    let node = graph
                        .ids
                        .iter()
                        .position(|candidate| candidate == id)
                        .expect("correlated package");
                    for &edge in &graph.reverse[node] {
                        *differences
                            .entry(graph.edges[edge].provenance.label())
                            .or_insert(0usize) += 1;
                    }
                }
                for (evidence, count) in differences {
                    println!("    Incoming evidence on differing formulae: {evidence}: {count}");
                }
            }
            println!(
                "  Semantics: Den all evidence includes build/test/optional declarations; recorded uses all kegs and excludes cask declarations; current uses today's required/recommended declarations. Homebrew may select a keg, fall back to formula evaluation, and includes cask requirements."
            );
        }
    }
}
