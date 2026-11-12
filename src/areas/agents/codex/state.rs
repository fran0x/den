//! Codex collection state. Browsing is in `agents`; these toggles change what Codex
//! data the Agents area shows.
use super::*;

#[derive(Default)]
pub struct State {
    pub context: Option<Context>,
    pub inventory: Option<Inventory>,
    pub requested: bool,
    pub running: bool,
    pub error: Option<String>,
    /// Which of an MCP server's definitions the panel and `e` use (`V` cycles it).
    pub mcp_source: usize,
    /// Settings shows Codex source files instead of settings.
    pub sources: bool,
    /// Instructions shows the chain as if the project were trusted.
    pub conditional: bool,
}
impl State {
    pub fn configure(&mut self, context: Context) {
        self.context = Some(context);
        self.requested = true;
    }
    pub fn finish(&mut self, result: io::Result<Inventory>, skills: &mut skills::state::State) {
        self.running = false;
        skills.running = false;
        match result {
            Ok(mut inventory) => {
                skills.config = Some(inventory.skills_config.clone());
                skills.finish(Ok(std::mem::take(&mut inventory.skills)));
                skills.enablement = inventory.enablement.clone();
                self.inventory = Some(inventory);
                self.error = None;
            }
            Err(_) => {
                self.error = Some(
                    "Codex collection failed; previous configuration, skills and MCP retained (stale)"
                        .into(),
                );
                skills.error = self.error.clone();
            }
        }
    }
    pub fn chain(&self) -> Option<&instructions::Chain> {
        self.inventory.as_ref().map(|i| {
            if self.conditional {
                i.conditional_instructions
                    .as_ref()
                    .unwrap_or(&i.instructions)
            } else {
                &i.instructions
            }
        })
    }
}
