//! Skill collection state and the status filter. Browsing is in `agents`.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Valid,
    Invalid,
    NotEvaluated,
    Findings,
    Duplicate,
    Inaccessible,
}
impl Filter {
    /// The `i` key: All → each filter in turn → All.
    pub fn next(current: Option<Self>) -> Option<Self> {
        match current {
            None => Some(Self::Valid),
            Some(Self::Valid) => Some(Self::Invalid),
            Some(Self::Invalid) => Some(Self::NotEvaluated),
            Some(Self::NotEvaluated) => Some(Self::Findings),
            Some(Self::Findings) => Some(Self::Duplicate),
            Some(Self::Duplicate) => Some(Self::Inaccessible),
            Some(Self::Inaccessible) => None,
        }
    }
    pub fn accepts(self, s: &Skill) -> bool {
        match self {
            Self::Valid => s.metadata == Validity::Valid,
            Self::Invalid => s.metadata == Validity::Invalid,
            Self::NotEvaluated => s.metadata == Validity::NotEvaluated,
            Self::Findings => !s.findings.is_empty(),
            Self::Duplicate => s.duplicate,
            Self::Inaccessible => !s.accessible,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Valid => "Frontmatter valid",
            Self::Invalid => "Frontmatter invalid",
            Self::NotEvaluated => "Frontmatter unchecked",
            Self::Findings => "With findings",
            Self::Duplicate => "Duplicate names",
            Self::Inaccessible => "Inaccessible",
        }
    }
}
#[derive(Default)]
pub struct State {
    pub config: Option<Config>,
    pub inventory: Inventory,
    /// Codex's status per skill, aligned with `inventory.skills`.
    pub enablement: Vec<crate::codex::Enablement>,
    pub requested: bool,
    pub running: bool,
    pub loaded: bool,
    pub error: Option<String>,
}
impl State {
    pub fn configure(&mut self, config: Config) {
        self.config = Some(config);
        self.requested = true;
    }
    pub fn finish(&mut self, result: io::Result<Inventory>) {
        self.running = false;
        match result {
            Ok(inventory) => {
                self.inventory = inventory;
                self.loaded = true;
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!(
                    "Collection failed: {}; previous skills retained",
                    error
                ))
            }
        }
    }
    pub fn status(&self) -> String {
        if self.config.is_none() {
            "Unavailable in this snapshot/demo; no filesystem discovery performed".into()
        } else if self.running {
            if self.loaded {
                "Refreshing skills; showing previous inventory".into()
            } else {
                "Discovering skills...".into()
            }
        } else if let Some(error) = &self.error {
            format!("{}{}", if self.loaded { "STALE: " } else { "" }, error)
        } else if self.loaded {
            format!(
                "{}{}{}",
                crate::inventory::count(self.inventory.skills.len(), "skill"),
                if self.inventory.partial {
                    " · partial"
                } else {
                    ""
                },
                if self.enablement.is_empty() {
                    " · config not evaluated"
                } else {
                    ""
                }
            )
        } else {
            "Not collected; r:discover".into()
        }
    }
}
