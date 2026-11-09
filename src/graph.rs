//! Homebrew evidence graph. Nodes refer only to installed resources; no I/O or UI.
use crate::inventory::{HomebrewMetadata, InstallationIntent, Inventory, ResourceKind};
use std::collections::{HashMap, HashSet, VecDeque};

pub const MAX_EDGES: usize = 100_000;
pub const MAX_VISITS: usize = 10_000;
pub const MAX_DEPTH: usize = 32;
pub const MAX_PATHS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Evidence {
    Recorded,
    Current,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provenance {
    RecordedDirect,
    RecordedTransitive,
    RecordedUnknown,
    Required,
    Recommended,
    Optional,
    Build,
    Test,
}
impl Provenance {
    pub fn label(self) -> &'static str {
        match self {
            Self::RecordedDirect => "recorded direct runtime",
            Self::RecordedTransitive => "recorded transitive runtime (intermediates unknown)",
            Self::RecordedUnknown => "recorded runtime (directness unknown)",
            Self::Required => "current declared dependency",
            Self::Recommended => "current recommended declaration",
            Self::Optional => "current optional declaration",
            Self::Build => "current build-only declaration",
            Self::Test => "current test-only declaration",
        }
    }
    pub fn evidence(self) -> Evidence {
        match self {
            Self::RecordedDirect | Self::RecordedTransitive | Self::RecordedUnknown => {
                Evidence::Recorded
            }
            _ => Evidence::Current,
        }
    }
    pub fn traversable(self) -> bool {
        matches!(
            self,
            Self::RecordedDirect | Self::Required | Self::Recommended
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resolution {
    Installed(usize),
    Missing,
    Ambiguous(Vec<usize>),
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Edge {
    pub from: usize,
    pub target: Resolution,
    pub reference: String,
    pub kind: ResourceKind,
    pub provenance: Provenance,
    pub source_keg: Option<String>,
    pub target_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Installation {
    pub node: usize,
    pub keg: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub ids: Vec<String>,
    pub edges: Vec<Edge>,
    pub forward: Vec<Vec<usize>>,
    pub reverse: Vec<Vec<usize>>,
    pub truncated: bool,
    versions: Vec<Vec<String>>,
    preferred: Vec<Option<String>>,
    recorded_available: HashSet<Installation>,
    current_available: HashSet<usize>,
    requested: Vec<bool>,
}

#[derive(Debug, Default)]
pub struct Paths {
    pub paths: Vec<Vec<usize>>,
    pub truncated: bool,
}

#[derive(Debug, Default)]
pub struct Traversal {
    pub nodes: Vec<Installation>,
    pub cycle: bool,
    pub truncated: bool,
}

type Names = HashMap<(ResourceKind, String), Vec<usize>>;

impl Graph {
    pub fn build(inventory: &Inventory) -> Self {
        let count = inventory.resources.len();
        let mut graph = Self {
            ids: inventory.resources.iter().map(|r| r.id.clone()).collect(),
            forward: vec![vec![]; count],
            reverse: vec![vec![]; count],
            versions: vec![vec![]; count],
            preferred: vec![None; count],
            requested: vec![false; count],
            ..Self::default()
        };
        let mut names = Names::new();
        for (node, resource) in inventory.resources.iter().enumerate() {
            let Some(metadata) = &resource.homebrew else {
                continue;
            };
            graph.requested[node] = metadata.intent() == InstallationIntent::Requested;
            let (full, tap, aliases) = match metadata {
                HomebrewMetadata::Formula {
                    full_name,
                    tap,
                    installed,
                    linked_keg,
                    aliases,
                    ..
                } => {
                    graph.versions[node] = installed.iter().map(|k| k.version.clone()).collect();
                    graph.preferred[node] = linked_keg
                        .as_ref()
                        .filter(|v| graph.versions[node].contains(v))
                        .cloned()
                        .or_else(|| (installed.len() == 1).then(|| installed[0].version.clone()));
                    for keg in installed {
                        if keg.runtime_dependencies.is_some() {
                            graph.recorded_available.insert(Installation {
                                node,
                                keg: Some(keg.version.clone()),
                            });
                        }
                    }
                    (full_name.as_deref(), tap.as_deref(), aliases)
                }
                HomebrewMetadata::Cask {
                    full_token,
                    tap,
                    old_tokens,
                    ..
                } => (full_token.as_deref(), tap.as_deref(), old_tokens),
            };
            for name in std::iter::once(resource.name.as_str())
                .chain(full)
                .chain(aliases.iter().map(String::as_str))
            {
                if name.trim().is_empty() {
                    continue;
                }
                names
                    .entry((resource.kind, name.to_owned()))
                    .or_default()
                    .push(node);
                if !name.contains('/')
                    && let Some(tap) = tap
                {
                    names
                        .entry((resource.kind, format!("{tap}/{name}")))
                        .or_default()
                        .push(node);
                }
            }
            if let Some((_, qualified)) = resource
                .id
                .split_once(':')
                .and_then(|(_, rest)| rest.split_once(':'))
            {
                names
                    .entry((resource.kind, qualified.to_owned()))
                    .or_default()
                    .push(node);
            }
        }
        for candidates in names.values_mut() {
            candidates.sort_unstable();
            candidates.dedup();
        }
        let mut seen = HashSet::new();
        'packages: for (node, resource) in inventory.resources.iter().enumerate() {
            match &resource.homebrew {
                Some(HomebrewMetadata::Formula {
                    installed,
                    dependencies,
                    ..
                }) => {
                    for keg in installed {
                        for raw in keg.runtime_dependencies.iter().flatten() {
                            let provenance =
                                match raw.get("declared_directly").and_then(|v| v.as_bool()) {
                                    Some(true) => Provenance::RecordedDirect,
                                    Some(false) => Provenance::RecordedTransitive,
                                    None => Provenance::RecordedUnknown,
                                };
                            let version = raw
                                .get("pkg_version")
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                                .or_else(|| {
                                    raw.get("version").and_then(|v| v.as_str()).map(|v| {
                                        match raw.get("revision").and_then(|r| r.as_u64()) {
                                            Some(revision) if revision > 0 => {
                                                format!("{v}_{revision}")
                                            }
                                            _ => v.to_owned(),
                                        }
                                    })
                                });
                            if !graph.add(
                                &names,
                                &mut seen,
                                Edge {
                                    from: node,
                                    target: Resolution::Invalid,
                                    reference: raw
                                        .get("full_name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_owned(),
                                    kind: ResourceKind::Formula,
                                    provenance,
                                    source_keg: Some(keg.version.clone()),
                                    target_version: version,
                                },
                            ) {
                                break 'packages;
                            }
                        }
                    }
                    for (key, provenance) in [
                        ("dependencies", Provenance::Required),
                        ("recommended_dependencies", Provenance::Recommended),
                        ("optional_dependencies", Provenance::Optional),
                        ("build_dependencies", Provenance::Build),
                        ("test_dependencies", Provenance::Test),
                    ] {
                        if let Some(value) = dependencies.get(key) {
                            graph.current_available.insert(node);
                            if !graph.declarations(
                                &names,
                                &mut seen,
                                node,
                                ResourceKind::Formula,
                                provenance,
                                value,
                            ) {
                                break 'packages;
                            }
                        }
                    }
                }
                Some(HomebrewMetadata::Cask {
                    depends_on: Some(dependencies),
                    ..
                }) => {
                    graph.current_available.insert(node);
                    for (key, kind) in [
                        ("formula", ResourceKind::Formula),
                        ("cask", ResourceKind::Cask),
                    ] {
                        if let Some(value) = dependencies.get(key)
                            && !graph.declarations(
                                &names,
                                &mut seen,
                                node,
                                kind,
                                Provenance::Required,
                                value,
                            )
                        {
                            break 'packages;
                        }
                    }
                    if !dependencies.is_object() {
                        graph.add(
                            &names,
                            &mut seen,
                            Edge {
                                from: node,
                                target: Resolution::Invalid,
                                reference: String::new(),
                                kind: ResourceKind::Cask,
                                provenance: Provenance::Required,
                                source_keg: None,
                                target_version: None,
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        graph
    }

    fn declarations(
        &mut self,
        names: &Names,
        seen: &mut HashSet<Edge>,
        node: usize,
        kind: ResourceKind,
        provenance: Provenance,
        value: &serde_json::Value,
    ) -> bool {
        let items = value
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(std::slice::from_ref(value));
        for item in items {
            if !self.add(
                names,
                seen,
                Edge {
                    from: node,
                    target: Resolution::Invalid,
                    reference: item.as_str().unwrap_or("").to_owned(),
                    kind,
                    provenance,
                    source_keg: None,
                    target_version: None,
                },
            ) {
                return false;
            }
        }
        true
    }

    fn add(&mut self, names: &Names, seen: &mut HashSet<Edge>, mut edge: Edge) -> bool {
        if self.edges.len() >= MAX_EDGES {
            self.truncated = true;
            return false;
        }
        edge.target = if edge.reference.trim().is_empty() {
            Resolution::Invalid
        } else {
            match names
                .get(&(edge.kind, edge.reference.clone()))
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                [] => Resolution::Missing,
                [node] => Resolution::Installed(*node),
                nodes => Resolution::Ambiguous(nodes.to_vec()),
            }
        };
        let mut identity = edge.clone();
        // Different aliases for one installed target are one relationship.
        if matches!(identity.target, Resolution::Installed(_)) {
            identity.reference.clear();
        }
        if seen.insert(identity) {
            let index = self.edges.len();
            self.forward[edge.from].push(index);
            if let Resolution::Installed(node) = edge.target {
                self.reverse[node].push(index);
            }
            self.edges.push(edge);
        }
        true
    }

    pub fn preferred(&self, node: usize) -> Installation {
        Installation {
            node,
            keg: self.preferred.get(node).cloned().flatten(),
        }
    }
    pub fn versions(&self, node: usize) -> &[String] {
        self.versions.get(node).map(Vec::as_slice).unwrap_or(&[])
    }
    pub fn available(&self, state: &Installation, evidence: Evidence) -> bool {
        match evidence {
            Evidence::Recorded => self.recorded_available.contains(state),
            Evidence::Current => self.current_available.contains(&state.node),
        }
    }
    pub fn is_requested(&self, node: usize) -> bool {
        self.requested.get(node).copied().unwrap_or(false)
    }

    /// Positive recorded runtime evidence from another installed package. This
    /// neither establishes installation intent nor proves nonmatching packages unused.
    pub fn has_recorded_users(&self, node: usize) -> bool {
        self.reverse.get(node).into_iter().flatten().any(|&index| {
            let edge = &self.edges[index];
            edge.from != node
                && edge.provenance.evidence() == Evidence::Recorded
                && self.target_state(edge).is_some()
        })
    }

    /// Unknown/missing target versions are never silently replaced by a different keg.
    pub fn target_state(&self, edge: &Edge) -> Option<Installation> {
        let Resolution::Installed(node) = edge.target else {
            return None;
        };
        if edge.provenance.evidence() == Evidence::Current {
            return Some(Installation { node, keg: None });
        }
        let versions = self.versions(node);
        let keg = match &edge.target_version {
            Some(version) if versions.contains(version) => version.clone(),
            None if versions.len() == 1 => versions[0].clone(),
            _ => return None,
        };
        Some(Installation {
            node,
            keg: Some(keg),
        })
    }

    pub fn adjacent(
        &self,
        state: &Installation,
        evidence: Evidence,
        reverse: bool,
        all: bool,
    ) -> Vec<usize> {
        let adjacency = if reverse {
            &self.reverse
        } else {
            &self.forward
        };
        adjacency
            .get(state.node)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&index| {
                let edge = &self.edges[index];
                if edge.provenance.evidence() != evidence {
                    return false;
                }
                if !all
                    && matches!(
                        edge.provenance,
                        Provenance::RecordedTransitive | Provenance::RecordedUnknown
                    )
                {
                    return false;
                }
                evidence == Evidence::Current
                    || if reverse {
                        // Show incoming uncertain-version evidence too, but never traverse it.
                        all || edge
                            .target_version
                            .as_ref()
                            .is_none_or(|v| Some(v) == state.keg.as_ref())
                    } else {
                        edge.source_keg == state.keg
                    }
            })
            .collect()
    }

    pub fn step(&self, edge: &Edge, state: &Installation, reverse: bool) -> Option<Installation> {
        if !edge.provenance.traversable() {
            return None;
        }
        if reverse {
            let target = self.target_state(edge)?;
            if edge.provenance.evidence() == Evidence::Recorded && target != *state {
                return None;
            }
            Some(Installation {
                node: edge.from,
                keg: edge.source_keg.clone(),
            })
        } else {
            self.target_state(edge)
        }
    }

    /// Bounded DFS with an active-path set, so shared dependencies are not cycles.
    pub fn traverse(
        &self,
        mut start: Installation,
        evidence: Evidence,
        reverse: bool,
    ) -> Traversal {
        if evidence == Evidence::Current {
            start.keg = None;
        }
        let mut result = Traversal {
            truncated: self.truncated,
            ..Traversal::default()
        };
        let mut visited = HashSet::new();
        let mut active = HashSet::new();
        let mut stack = vec![(start, 0, false)];
        let mut work = 0;
        while let Some((state, depth, leaving)) = stack.pop() {
            if leaving {
                active.remove(&state);
                continue;
            }
            work += 1;
            if work > MAX_VISITS {
                result.truncated = true;
                break;
            }
            if active.contains(&state) {
                result.cycle = true;
                continue;
            }
            if !visited.insert(state.clone()) {
                continue;
            }
            result.nodes.push(state.clone());
            active.insert(state.clone());
            stack.push((state.clone(), depth, true));
            let edges = self.adjacent(&state, evidence, reverse, false);
            for index in edges.into_iter().rev() {
                if let Some(next) = self.step(&self.edges[index], &state, reverse) {
                    if depth >= MAX_DEPTH {
                        result.truncated = true;
                    } else {
                        stack.push((next, depth + 1, false));
                    }
                }
            }
        }
        result
    }

    /// One shortest known direct-edge path per requested installation, capped at 20.
    /// Paths run from the requested root to the selected installation.
    pub fn paths_to_requested(&self, mut target: Installation, evidence: Evidence) -> Paths {
        if evidence == Evidence::Current {
            target.keg = None;
        }
        let mut result = Paths {
            truncated: self.truncated,
            ..Paths::default()
        };
        let mut queue = VecDeque::from([(target.clone(), Vec::new())]);
        let mut visited = HashSet::from([target]);
        let mut work = 0;
        while let Some((state, path)) = queue.pop_front() {
            if self.is_requested(state.node) && !path.is_empty() {
                if result.paths.len() >= MAX_PATHS {
                    result.truncated = true;
                    break;
                }
                result.paths.push(path.clone());
            }
            for index in self.adjacent(&state, evidence, true, false) {
                work += 1;
                if work > MAX_VISITS {
                    result.truncated = true;
                    return result;
                }
                if let Some(next) = self.step(&self.edges[index], &state, true) {
                    if path.len() >= MAX_DEPTH {
                        result.truncated = true;
                        continue;
                    }
                    if visited.insert(next.clone()) {
                        let mut next_path = vec![index];
                        next_path.extend_from_slice(&path);
                        queue.push_back((next, next_path));
                    }
                }
            }
        }
        result
    }
}
