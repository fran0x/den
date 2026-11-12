//! Read-only observations over the existing graph. These are not repair/removal advice.
use crate::areas::homebrew::graph::{Evidence, Graph, Provenance, Resolution};
use crate::inventory::Inventory;
use std::collections::BTreeMap;
mod checks;
pub use checks::{Check, CheckState, Report, ReportEntry, Status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cause {
    AbsentRecorded,
    AbsentCurrent,
    AbsentOptional,
    AbsentBuild,
    AbsentTest,
    AmbiguousIdentity,
    IncompleteReference,
    TargetVersionAbsent,
    TargetVersionUnknown,
}
impl Cause {
    pub fn label(self) -> &'static str {
        match self {
            Self::AbsentRecorded => "Recorded runtime package absent",
            Self::AbsentCurrent => "Current runtime declaration absent",
            Self::AbsentOptional => "Optional declaration absent",
            Self::AbsentBuild => "Build-only declaration absent",
            Self::AbsentTest => "Test-only declaration absent",
            Self::AmbiguousIdentity => "Ambiguous package identity",
            Self::IncompleteReference => "Incomplete dependency reference",
            Self::TargetVersionAbsent => "Recorded target keg version absent",
            Self::TargetVersionUnknown => "Target keg metadata insufficient",
        }
    }
    pub fn explanation(self) -> &'static str {
        match self {
            Self::AbsentRecorded => {
                "Review required: recorded runtime package is absent from this inventory; brew missing can provide supplementary evidence."
            }
            Self::AbsentCurrent => {
                "Review required: current declaration is absent. A current definition need not describe the installed keg's historical dependencies."
            }
            Self::AbsentOptional | Self::AbsentBuild | Self::AbsentTest => {
                "Informational: this declaration does not establish an ordinary runtime requirement. Its absence alone is not a broken installation."
            }
            Self::AmbiguousIdentity => {
                "Review required: multiple installed identities match; no target was chosen."
            }
            Self::IncompleteReference => {
                "Review required: Homebrew metadata lacks a usable package reference."
            }
            Self::TargetVersionAbsent => {
                "Review required: the package resolves, but the recorded version is not installed. This may be a historical record after an upgrade; it does not alone prove a broken installation."
            }
            Self::TargetVersionUnknown => {
                "Review required: the package resolves, but metadata cannot identify a unique installed target keg."
            }
        }
    }
    pub fn keg(self) -> bool {
        matches!(self, Self::TargetVersionAbsent | Self::TargetVersionUnknown)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Health {
    /// Grouped by cause and exact evidence source; values index the existing graph.
    pub groups: BTreeMap<(Cause, &'static str), Vec<usize>>,
    pub unresolved: usize,
    pub keg_gaps: usize,
    pub no_dependents: Vec<usize>,
    pub no_recorded_dependents: Vec<usize>,
    pub no_current_dependents: Vec<usize>,
    pub partial: bool,
}
impl Health {
    pub fn build(graph: &Graph) -> Self {
        let mut health = Self {
            partial: graph.truncated,
            ..Self::default()
        };
        for (index, edge) in graph.edges.iter().enumerate() {
            let cause = match edge.target {
                Resolution::Missing => Some(match edge.provenance {
                    Provenance::Required | Provenance::Recommended => Cause::AbsentCurrent,
                    Provenance::Optional => Cause::AbsentOptional,
                    Provenance::Build => Cause::AbsentBuild,
                    Provenance::Test => Cause::AbsentTest,
                    _ => Cause::AbsentRecorded,
                }),
                Resolution::Ambiguous(_) => Some(Cause::AmbiguousIdentity),
                Resolution::Invalid => Some(Cause::IncompleteReference),
                Resolution::Installed(_)
                    if edge.provenance.evidence() == Evidence::Recorded
                        && graph.target_state(edge).is_none() =>
                {
                    Some(
                        if edge
                            .target_version
                            .as_deref()
                            .is_some_and(|v| !v.trim().is_empty())
                        {
                            Cause::TargetVersionAbsent
                        } else {
                            Cause::TargetVersionUnknown
                        },
                    )
                }
                _ => None,
            };
            if let Some(cause) = cause {
                if cause.keg() {
                    health.keg_gaps += 1;
                } else {
                    health.unresolved += 1;
                }
                health
                    .groups
                    .entry((cause, edge.provenance.label()))
                    .or_default()
                    .push(index);
            }
        }
        for (node, incoming) in graph.reverse.iter().enumerate() {
            // Count observed package-level evidence, including old kegs and unknown directness.
            // Ambiguous/unresolved references remain coverage gaps, never proof of removability.
            if incoming.is_empty() {
                health.no_dependents.push(node);
            }
            if !incoming
                .iter()
                .any(|&i| graph.edges[i].provenance.evidence() == Evidence::Recorded)
            {
                health.no_recorded_dependents.push(node);
            }
            if !incoming.iter().any(|&i| {
                matches!(
                    graph.edges[i].provenance,
                    Provenance::Required | Provenance::Recommended
                )
            }) {
                health.no_current_dependents.push(node);
            }
        }
        health
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Unresolved,
    Missing,
    Leaves,
    Cleanup,
}
impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unresolved => "Unresolved",
            Self::Missing => "Missing",
            Self::Leaves => "Leaves",
            Self::Cleanup => "Cleanup preview",
        }
    }
    pub fn check(self) -> Option<Check> {
        match self {
            Self::Unresolved => None,
            Self::Missing => Some(Check::Missing),
            Self::Leaves => Some(Check::Leaves),
            Self::Cleanup => Some(Check::Cleanup),
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct View {
    pub tab: Option<Tab>,
    pub group: Option<usize>,
    pub selected: usize,
    pub scroll: u16,
    pub raw: bool,
    pub expanded: bool,
}
#[derive(Debug, Clone, Copy)]
pub enum Item {
    Group(usize),
    Edge(usize),
    Package(usize),
    Entry(usize),
}

impl View {
    pub fn items(&self, health: &Health, checks: &[CheckState; 3]) -> Vec<Item> {
        match self.tab {
            Some(Tab::Unresolved) => match self.group {
                Some(group) => health
                    .groups
                    .values()
                    .nth(group)
                    .into_iter()
                    .flatten()
                    .map(|&e| Item::Edge(e))
                    .collect(),
                None => (0..health.groups.len()).map(Item::Group).collect(),
            },
            Some(Tab::Leaves) => match self.group {
                None => (0..4).map(Item::Group).collect(),
                Some(group @ 0..=2) => [
                    &health.no_dependents,
                    &health.no_recorded_dependents,
                    &health.no_current_dependents,
                ][group]
                    .iter()
                    .copied()
                    .map(Item::Package)
                    .collect(),
                _ => checks[Check::Leaves.index()]
                    .report
                    .as_ref()
                    .into_iter()
                    .flat_map(|r| 0..r.entries.len())
                    .map(Item::Entry)
                    .collect(),
            },
            Some(tab) => tab
                .check()
                .into_iter()
                .flat_map(|c| checks[c.index()].report.as_ref())
                .flat_map(|r| 0..r.entries.len())
                .map(Item::Entry)
                .collect(),
            None => vec![],
        }
    }
}

pub fn describe(
    item: Item,
    view: &View,
    health: &Health,
    inventory: &Inventory,
    graph: &Graph,
    checks: &[CheckState; 3],
    epoch: u64,
) -> (String, String) {
    match item {
        Item::Group(group) if view.tab == Some(Tab::Unresolved) => {
            let Some(((cause, evidence), edges)) = health.groups.iter().nth(group) else {
                return (String::new(), String::new());
            };
            (
                format!("{} · {} · {evidence}", edges.len(), cause.label()),
                format!(
                    "{}\nEvidence: {evidence}\nEnter: inspect {} references. Space: full explanation.",
                    cause.explanation(),
                    edges.len()
                ),
            )
        }
        Item::Group(group) => {
            let (label, count, description) = match group {
                0 => (
                    "No observed dependents (all graph evidence)",
                    Some(health.no_dependents.len()),
                    "All resolved incoming edges, including build/test/optional declarations and every recorded keg.",
                ),
                1 => (
                    "No recorded runtime dependents",
                    Some(health.no_recorded_dependents.len()),
                    "Every installed source keg, including transitive and unknown-directness runtime records. Cask current declarations are excluded.",
                ),
                2 => (
                    "No current runtime dependents",
                    Some(health.no_current_dependents.len()),
                    "Current required/recommended formula declarations and cask package requirements; build/test/optional excluded.",
                ),
                _ => (
                    "Homebrew leaves (formulae)",
                    checks[Check::Leaves.index()]
                        .report
                        .as_ref()
                        .and_then(|r| r.count),
                    "Homebrew's own evaluation can select a keg, fall back to formula metadata, and include cask requirements.",
                ),
            };
            (
                format!(
                    "{label}: {}",
                    count.map_or("Not checked / count unavailable".into(), |n| n.to_string())
                ),
                format!(
                    "Informational: {description}\nDen counts packages (including casks); Homebrew leaves are formulae. Unresolved references or absent metadata limit coverage. No observed dependents, a Homebrew leaf, and autoremove eligibility are different facts; none proves safety. Enter: inspect list."
                ),
            )
        }
        Item::Edge(index) => {
            let edge = &graph.edges[index];
            let resource = &inventory.resources[edge.from];
            let cause = health
                .groups
                .iter()
                .find(|(_, edges)| edges.contains(&index))
                .map(|((cause, _), _)| *cause);
            let resolution = match &edge.target {
                Resolution::Installed(node) => format!(
                    "Package resolved: {}\nInstalled target kegs: {}",
                    inventory.resources[*node].id,
                    graph.versions(*node).join(", ")
                ),
                Resolution::Missing => "No installed package matches this reference.".into(),
                Resolution::Ambiguous(nodes) => format!(
                    "Ambiguous candidates: {}",
                    nodes
                        .iter()
                        .map(|&n| inventory.resources[n].id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Resolution::Invalid => "Missing or invalid reference metadata.".into(),
            };
            let state = graph.preferred(edge.from);
            let current_difference = if edge.provenance.evidence() == Evidence::Current
                && graph.available(&state, Evidence::Recorded)
            {
                let observed = graph.forward[edge.from].iter().any(|&i| {
                    let recorded = &graph.edges[i];
                    recorded.provenance.evidence() == Evidence::Recorded && recorded.source_keg == state.keg &&
                    (recorded.reference == edge.reference || matches!((&recorded.target, &edge.target), (Resolution::Installed(a), Resolution::Installed(b)) if a == b))
                });
                if observed {
                    "Reference also occurs in the preferred keg's runtime records."
                } else {
                    "This current declaration is not observed in the preferred keg's runtime records; definitions and installed history may differ."
                }
            } else {
                ""
            };
            (
                format!(
                    "{} [{}] -> {}",
                    resource.name,
                    edge.source_keg.as_deref().unwrap_or("current"),
                    edge.reference
                ),
                format!(
                    "{}\nSource: {}\nSource keg: {}\nReference: {} ({})\nEvidence: {}\nRecorded target version: {}\n{resolution}\n{current_difference}\nEnter: source package. Backspace: groups.",
                    cause.map_or("Review required", Cause::explanation),
                    resource.id,
                    edge.source_keg
                        .as_deref()
                        .unwrap_or("current declaration; no installed keg asserted"),
                    edge.reference,
                    edge.kind.label(),
                    edge.provenance.label(),
                    edge.target_version.as_deref().unwrap_or("not recorded")
                ),
            )
        }
        Item::Package(node) => {
            let r = &inventory.resources[node];
            (
                format!("{} ({})", r.name, r.kind.label()),
                format!(
                    "Informational: no observed dependents in the selected evidence view.\n{}\nIntent: {} (unchanged).\nIncomplete metadata and unresolved references limit this observation. It does not imply unused, autoremove-eligible, or safe to remove. Enter: package; Backspace: groups.",
                    r.id,
                    r.homebrew.as_ref().map_or("Unknown", |m| m.intent_label())
                ),
            )
        }
        Item::Entry(index) => {
            let Some(report) = view
                .tab
                .and_then(Tab::check)
                .and_then(|c| checks[c.index()].report.as_ref())
            else {
                return (String::new(), String::new());
            };
            let entry = &report.entries[index];
            let correlation = entry
                .package_id
                .as_deref()
                .unwrap_or("No unique exact installed identity; report retained without mapping");
            let evidence = if report.epoch == epoch {
                entry
                    .edge
                    .map(|i| format!("Graph evidence: {}", graph.edges[i].provenance.label()))
                    .unwrap_or_else(|| "No unique graph edge asserted.".into())
            } else {
                "STALE: report predates this inventory; graph correlation is not reused.".into()
            };
            (
                entry.label.clone(),
                format!(
                    "{}\n{correlation}\n{evidence}\n{}\nHomebrew leaves and autoremove eligibility are distinct; neither proves safe removal. Enter: correlated package; o: original output.",
                    report.status.label(),
                    report.message
                ),
            )
        }
    }
}
