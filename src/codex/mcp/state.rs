//! MCP-only filter controls; selection, search, focus and scrolling use Codex state.
use super::*;
#[derive(Default)]
pub struct Controls {
    pub section: usize,
    pub source: usize,
    pub dimension: usize,
    pub transport: usize,
    pub scope: usize,
    pub status: usize,
    pub validation: usize,
}
impl Controls {
    pub fn cycle_dimension(&mut self) {
        self.dimension = (self.dimension + 1) % 4;
    }
    pub fn cycle_value(&mut self) {
        match self.dimension {
            0 => self.transport = (self.transport + 1) % 5,
            1 => self.scope = (self.scope + 1) % 5,
            2 => self.status = (self.status + 1) % 7,
            _ => self.validation = (self.validation + 1) % 4,
        }
    }
    pub fn reset(&mut self) {
        self.transport = 0;
        self.scope = 0;
        self.status = 0;
        self.validation = 0;
    }
    pub fn accepts(&self, s: &Server) -> bool {
        let transport = match self.transport {
            0 => true,
            1 => s.transport() == Transport::Stdio,
            2 => s.transport() == Transport::Http,
            3 => s.transport() == Transport::Unknown,
            _ => s.transport() == Transport::Ambiguous,
        };
        let scope = match self.scope {
            0 => true,
            n => s.definitions.iter().any(|d| {
                Some(d.scope)
                    == [
                        None,
                        Some(Scope::User),
                        Some(Scope::Project),
                        Some(Scope::Profile),
                        Some(Scope::System),
                    ][n]
            }),
        };
        let status = match self.status {
            0 => true,
            1 => s.status == Status::Eligible,
            2 => s
                .definitions
                .iter()
                .any(|d| d.activity != Activity::Inactive && d.enabled == Some(false)),
            3 => s.status == Status::Conditional,
            4 => s.status == Status::Invalid,
            5 => s.status == Status::PluginUnresolved,
            _ => s.status == Status::Configured,
        };
        let validation = match self.validation {
            0 => true,
            1 => s.validation() == "No detected errors",
            2 => s.validation() == "Errors",
            _ => s.validation() == "Review / unknown",
        };
        transport && scope && status && validation
    }
    pub fn label(&self) -> String {
        format!(
            "f: {} · i: value | type {} · scope {} · state {} · validation {}",
            ["Transport", "Scope", "Enablement", "Validation"][self.dimension],
            ["All", "Stdio", "HTTP", "Unknown", "Ambiguous"][self.transport],
            ["All", "User", "Project", "Profile", "System"][self.scope],
            [
                "All",
                "Eligible",
                "Disabled",
                "Conditional",
                "Invalid",
                "Plugin unresolved",
                "Inactive"
            ][self.status],
            ["All", "No errors", "Errors", "Review/unknown"][self.validation]
        )
    }
}
