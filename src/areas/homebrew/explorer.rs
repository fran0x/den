//! Dependency-pane state and bounded tree/path projection; no terminal widgets.
use crate::{
    areas::homebrew::graph::{Evidence, Graph, Installation, Provenance, Resolution},
    inventory::Inventory,
};
use std::collections::HashSet;

pub const MAX_ROWS: usize = 400;

#[derive(Clone, Copy)]
struct Projection {
    rows: usize,
    depth: usize,
    passive: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    #[default]
    Overview,
    Dependencies,
    RequiredBy,
}
impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Dependencies => "Dependencies",
            Self::RequiredBy => "Required by",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub label: String,
    pub detail: String,
    pub target: Option<Installation>,
    pub branch: Vec<usize>,
    pub expandable: bool,
    pub cycle: bool,
}

#[derive(Debug, Clone)]
pub struct Explorer {
    pub section: Section,
    /// Evidence shown for the selected package: recorded runtime, or current
    /// declarations for this package only when it has no recorded metadata.
    pub evidence: Evidence,
    pub tree: bool,
    pub installation: Option<Installation>,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub offset: usize,
    pub expanded: HashSet<Vec<usize>>,
    pub note: String,
    pub limited: bool,
}

impl Default for Explorer {
    fn default() -> Self {
        Self {
            section: Section::Overview,
            evidence: Evidence::Recorded,
            tree: true,
            installation: None,
            rows: vec![],
            selected: 0,
            offset: 0,
            expanded: HashSet::new(),
            note: String::new(),
            limited: false,
        }
    }
}

impl Explorer {
    pub fn select(&mut self, graph: &Graph, node: Option<usize>) {
        self.installation = node.map(|node| graph.preferred(node));
        // A fallback applies to this package only; it never changes the next selection.
        self.evidence = Evidence::Recorded;
        if self.evidence == Evidence::Recorded
            && let Some(state) = &self.installation
            && graph.versions(state.node).len() <= 1
            && !graph.available(state, Evidence::Recorded)
            && graph.available(state, Evidence::Current)
        {
            self.evidence = Evidence::Current;
        }
        self.clear_navigation();
    }
    pub fn clear_navigation(&mut self) {
        self.selected = 0;
        self.offset = 0;
        self.expanded.clear();
    }
    /// A bounded, automatically opened two-level tree; never changes inspector state.
    pub fn preview(
        graph: &Graph,
        inventory: &Inventory,
        root: Installation,
        evidence: Evidence,
        reverse: bool,
        rows: usize,
    ) -> Self {
        let mut preview = Self {
            installation: Some(root),
            evidence,
            section: if reverse {
                Section::RequiredBy
            } else {
                Section::Dependencies
            },
            ..Self::default()
        };
        preview.project(
            graph,
            inventory,
            Projection {
                rows: rows.min(MAX_ROWS),
                depth: 1,
                passive: true,
            },
        );
        preview
    }
    fn project(&mut self, graph: &Graph, inventory: &Inventory, projection: Projection) {
        self.rows.clear();
        self.limited = graph.truncated;
        let Some(mut root) = self.installation.clone() else {
            self.note = "Select a package.".into();
            return;
        };
        if self.evidence == Evidence::Current {
            root.keg = None;
        }
        self.note = match self.evidence {
            Evidence::Recorded => match &root.keg {
                Some(keg) => format!("Recorded keg {keg}."),
                None if !graph.versions(root.node).is_empty() => {
                    "No identifiable linked keg; V selects a version.".into()
                }
                None => {
                    "Recorded runtime metadata unavailable; s shows current declarations.".into()
                }
            },
            Evidence::Current => {
                "Current declarations; not historical installation evidence.".into()
            }
        };
        if self.section == Section::Overview {
            return;
        }
        {
            let reverse = self.section == Section::RequiredBy;
            let edges = graph.adjacent(&root, self.evidence, reverse, self.tree);
            self.append_children(
                graph,
                inventory,
                &root,
                reverse,
                &[],
                &[root.clone()],
                &edges,
                "",
                projection,
            );
            if self.rows.is_empty() {
                if !reverse && !graph.available(&root, self.evidence) {
                    self.note.push_str(" Dependency metadata unavailable.");
                } else if self.evidence == Evidence::Current {
                    self.note
                        .push_str(" No matching current package relationships in this snapshot.");
                } else if self.tree {
                    self.note
                        .push_str(" No matching recorded relationships in this snapshot.");
                } else {
                    self.note.push_str(
                        " No matching direct relationships. t shows all records as a tree.",
                    );
                }
            }
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        if self.limited {
            self.note.push_str(" LIMIT REACHED: results are partial.");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn append_children(
        &mut self,
        graph: &Graph,
        inventory: &Inventory,
        state: &Installation,
        reverse: bool,
        branch: &[usize],
        ancestors: &[Installation],
        edges: &[usize],
        prefix: &str,
        projection: Projection,
    ) {
        for (position, &index) in edges.iter().enumerate() {
            if self.rows.len() >= projection.rows {
                self.limited = true;
                return;
            }
            let edge = &graph.edges[index];
            let next = graph.step(edge, state, reverse);
            let cycle = next.as_ref().is_some_and(|next| ancestors.contains(next));
            let mut key = branch.to_vec();
            key.push(index);
            let mut row = row_for(graph, inventory, state, index, reverse, key.clone(), cycle);
            if projection.passive && self.evidence == Evidence::Recorded {
                let keg = if reverse {
                    edge.source_keg.as_ref()
                } else {
                    edge.target_version
                        .as_ref()
                        .or_else(|| row.target.as_ref().and_then(|target| target.keg.as_ref()))
                };
                if let Some(keg) = keg {
                    row.label.push_str(&format!(" @{keg}"));
                }
            }
            let children_available = next.as_ref().is_some_and(|next| {
                !graph
                    .adjacent(next, self.evidence, reverse, true)
                    .is_empty()
            }) && !cycle;
            row.expandable = children_available && branch.len() < projection.depth;
            if projection.passive
                && next
                    .as_ref()
                    .is_some_and(|next| !graph.available(next, self.evidence))
            {
                row.label.push_str(" [metadata unavailable]");
            }
            let expanded =
                self.tree && row.expandable && (projection.passive || self.expanded.contains(&key));
            let last = position + 1 == edges.len();
            row.label = if projection.passive {
                format!(
                    "{prefix}{} {}{}",
                    if last { "└─" } else { "├─" },
                    row.label,
                    if cycle {
                        " [cycle]"
                    } else if children_available && !expanded {
                        " …"
                    } else {
                        ""
                    }
                )
            } else if self.tree {
                format!(
                    "{prefix}{} {} {}{}",
                    if last { "└─" } else { "├─" },
                    if cycle {
                        "!"
                    } else if expanded {
                        "[-]"
                    } else if row.expandable {
                        "[+]"
                    } else {
                        "·"
                    },
                    row.label,
                    if cycle { " [cycle]" } else { "" }
                )
            } else {
                format!("  {}", row.label)
            };
            self.rows.push(row);
            if self.tree
                && branch.len() >= projection.depth
                && if projection.passive {
                    children_available
                } else {
                    next.is_some()
                }
            {
                self.limited = true;
            }
            if expanded && let Some(next) = next {
                let mut path = ancestors.to_vec();
                path.push(next.clone());
                let children = graph.adjacent(&next, self.evidence, reverse, true);
                let prefix = format!("{prefix}{}", if last { "   " } else { "│  " });
                self.append_children(
                    graph, inventory, &next, reverse, &key, &path, &children, &prefix, projection,
                );
            }
        }
    }
}

fn row_for(
    graph: &Graph,
    inventory: &Inventory,
    state: &Installation,
    index: usize,
    reverse: bool,
    branch: Vec<usize>,
    cycle: bool,
) -> Row {
    let edge = &graph.edges[index];
    let mut detail = edge.provenance.label().to_owned();
    if let Some(keg) = &edge.source_keg {
        detail.push_str(&format!("; source keg {keg}"));
    }
    if let Some(version) = &edge.target_version {
        detail.push_str(&format!("; dependency recorded version {version}"));
    }
    let (mut label, target) = if reverse {
        (
            inventory.resources[edge.from].display_name(),
            Some(Installation {
                node: edge.from,
                keg: edge.source_keg.clone(),
            }),
        )
    } else {
        match &edge.target {
            Resolution::Installed(node) => (
                inventory.resources[*node].display_name(),
                Some(
                    graph
                        .target_state(edge)
                        .unwrap_or_else(|| graph.preferred(*node)),
                ),
            ),
            Resolution::Missing => (format!("{} [not installed]", edge.reference), None),
            Resolution::Ambiguous(candidates) => (
                format!(
                    "{} [ambiguous: {} installed matches]",
                    edge.reference,
                    candidates.len()
                ),
                None,
            ),
            Resolution::Invalid => ("[invalid dependency reference]".into(), None),
        }
    };
    if reverse
        && edge.provenance.evidence() == Evidence::Recorded
        && edge
            .target_version
            .as_ref()
            .is_some_and(|version| Some(version) != state.keg.as_ref())
    {
        label.push_str(" [other target keg]");
    }
    if matches!(edge.target, Resolution::Installed(_))
        && edge.provenance.evidence() == Evidence::Recorded
    {
        if graph.target_state(edge).is_none() {
            label.push_str(" [keg unresolved]");
            detail.push_str("; target keg unresolved/not installed: traversal stops (Enter opens package overview)");
        } else if edge.target_version.is_none() {
            detail.push_str("; target version unrecorded, using sole installed keg");
        }
    }
    if reverse && graph.step(edge, state, true).is_none() && edge.provenance.traversable() {
        detail.push_str("; version linkage uncertain");
    }
    label.push_str(match edge.provenance {
        Provenance::RecordedTransitive => " [transitive record]",
        Provenance::RecordedUnknown => " [directness unknown]",
        Provenance::Recommended => " [recommended]",
        Provenance::Optional => " [optional]",
        Provenance::Build => " [build only]",
        Provenance::Test => " [test only]",
        _ => "",
    });
    Row {
        label,
        detail,
        target,
        branch,
        expandable: false,
        cycle,
    }
}
