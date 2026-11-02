use super::*;
use crate::app::Focus;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    #[default]
    Skills,
    Configuration,
    Instructions,
}
#[derive(Default)]
struct Position {
    query: String,
    scroll: u16,
    details: bool,
}
#[derive(Default)]
pub struct State {
    pub context: Option<Context>,
    pub inventory: Option<Inventory>,
    pub view: View,
    pub requested: bool,
    pub running: bool,
    pub error: Option<String>,
    pub selected: [usize; 3],
    positions: [Position; 2],
    source_return: Option<(usize, String)>,
    pub scroll: u16,
    pub scroll_max: u16,
    pub page: usize,
    pub query: String,
    pub searching: bool,
    pub focus_details: bool,
    pub sources: bool,
    pub preview: bool,
    pub conditional: bool,
    pub help: bool,
    pub return_to_skill: bool,
}
impl State {
    pub fn cycle_view(&mut self) {
        if self.view != View::Skills {
            let n = usize::from(self.view == View::Instructions);
            self.positions[n] = Position {
                query: self.query.clone(),
                scroll: self.scroll,
                details: self.focus_details,
            };
        }
        self.view = match self.view {
            View::Skills => View::Configuration,
            View::Configuration => View::Instructions,
            View::Instructions => View::Skills,
        };
        if self.view != View::Skills {
            let p = &self.positions[usize::from(self.view == View::Instructions)];
            self.query = p.query.clone();
            self.scroll = p.scroll;
            self.focus_details = p.details;
        }
        self.searching = false;
        self.help = false;
        self.clamp();
    }
    pub fn configure(&mut self, context: Context) {
        self.context = Some(context);
        self.requested = true;
    }
    pub fn finish(&mut self, result: io::Result<Inventory>, skills: &mut skills::state::State) {
        self.running = false;
        skills.running = false;
        let identity = self.selected_identity();
        match result {
            Ok(mut inventory) => {
                skills.config = Some(inventory.skills_config.clone());
                skills.finish(Ok(std::mem::take(&mut inventory.skills)));
                skills.enablement = inventory.enablement.clone();
                self.inventory = Some(inventory);
                self.error = None;
                if let Some(identity) = identity
                    && let Some(position) = self
                        .rows()
                        .iter()
                        .position(|(n, _)| self.identity(*n) == identity)
                {
                    self.selected[self.slot()] = position;
                }
                self.clamp();
            }
            Err(_) => {
                self.error=Some("Configuration collection failed; previous configuration and skills retained (stale)".into());
                skills.error = self.error.clone();
            }
        }
    }
    fn identity(&self, n: usize) -> String {
        let Some(i) = &self.inventory else {
            return String::new();
        };
        match self.view {
            View::Configuration if self.sources => i
                .configuration
                .layers
                .get(n)
                .map(|l| l.path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            View::Configuration => i
                .configuration
                .settings
                .get(n)
                .map(|s| s.key.clone())
                .unwrap_or_default(),
            View::Instructions => self
                .chain()
                .and_then(|c| c.files.get(n))
                .map(|f| f.path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            View::Skills => String::new(),
        }
    }
    fn selected_identity(&self) -> Option<String> {
        self.rows()
            .get(self.selected[self.slot()])
            .map(|(n, _)| self.identity(*n))
    }
    pub fn rows(&self) -> Vec<(usize, String)> {
        let Some(i) = &self.inventory else {
            return vec![];
        };
        let rows: Vec<_> = match self.view {
            View::Skills => vec![],
            View::Configuration if self.sources => i
                .configuration
                .layers
                .iter()
                .enumerate()
                .map(|(n, l)| {
                    (
                        n,
                        format!("{:?} · {} · {}", l.scope, l.status, privacy::path(&l.path)),
                    )
                })
                .collect(),
            View::Configuration => i
                .configuration
                .settings
                .iter()
                .enumerate()
                .map(|(n, s)| {
                    (
                        n,
                        format!(
                            "{}: {}{}",
                            s.key,
                            s.value,
                            if s.conditional.is_some() {
                                " [conditional]"
                            } else {
                                ""
                            }
                        ),
                    )
                })
                .collect(),
            View::Instructions => self
                .chain()
                .map(|c| {
                    c.files
                        .iter()
                        .enumerate()
                        .map(|(n, f)| (n, format!("{:?} · {}", f.status, privacy::path(&f.path))))
                        .collect()
                })
                .unwrap_or_default(),
        };
        rows.into_iter()
            .filter(|(_, s)| crate::inventory::matches_query(&self.query, s))
            .collect()
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
    pub fn slot(&self) -> usize {
        if self.view == View::Instructions {
            2
        } else {
            usize::from(self.sources)
        }
    }
    pub fn clamp(&mut self) {
        let slot = self.slot();
        self.selected[slot] = self.selected[slot].min(self.rows().len().saturating_sub(1));
    }
    pub fn status(&self) -> String {
        if self.context.is_none() {
            return "Unavailable in snapshot/skills-fixture mode; no live configuration access"
                .into();
        }
        if self.running || self.requested {
            return "Loading / refreshing Codex configuration and skills…".into();
        }
        if let Some(e) = &self.error {
            return e.clone();
        }
        self.inventory.as_ref().map_or("Not checked".into(),|i|format!("Locally resolved · trust {:?} · profile {} · {} layers · {} selected instructions{}",i.configuration.trust,i.configuration.profile.as_deref().unwrap_or("none selected"),i.configuration.layers.iter().filter(|l|l.present).count(),i.instructions.selected(),if self.conditional {" · IF TRUSTED scenario"} else {""}))
    }
    pub fn text(&self) -> String {
        if self.help {
            return "Codex local evidence\nF2: Homebrew/Codex; F3: Skills/Configuration/Instructions\n/: search · Tab: switch panes · arrows/j/k: select/scroll\nConfiguration: s toggles settings/sources; Enter opens selected setting's safe source\nInstructions: Enter toggles redacted preview; t toggles if-trusted scenario\nBackspace: return to settings or originating skill\nr: refresh configuration + skills atomically\nPageUp/PageDown/Home/End: navigate/scroll\n?: help · q: quit\n\nUnknown values are withheld; raw TOML is never displayed. CLI, managed policies and runtime configuration are unobserved.".into();
        }
        let Some(i) = &self.inventory else {
            return format!("{}\n\n{LIMITATIONS}", self.status());
        };
        let n = self.rows().get(self.selected[self.slot()]).map(|(n, _)| *n);
        let text = match (self.view, n) {
            (View::Configuration, Some(n)) if self.sources => i.configuration.source_text(n),
            (View::Configuration, Some(n)) => i.configuration.setting_text(n),
            (View::Instructions, Some(n)) => self
                .chain()
                .map_or(String::new(), |c| c.text(n, self.preview)),
            _ => "No matching entries".into(),
        };
        format!(
            "{text}\n\nCollection findings / coverage:\n{}",
            i.configuration.findings.join("\n\n")
        )
    }
    pub fn jump_source(&mut self, source: usize) {
        if self.view == View::Configuration && !self.sources {
            self.source_return = Some((self.selected[0], self.query.clone()));
        }
        self.view = View::Configuration;
        self.sources = true;
        self.query.clear();
        self.selected[1] = source;
        self.scroll = 0;
        self.focus_details = true;
    }
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        if self.searching {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear()
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.push(c);
                }
                _ => (),
            }
            self.clamp();
            return false;
        }
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.help = !self.help;
                self.scroll = 0;
            }
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Char('r') if self.context.is_some() && !self.running => self.requested = true,
            KeyCode::Tab => self.focus_details = !self.focus_details,
            KeyCode::Char('s') if self.view == View::Configuration => {
                self.sources = !self.sources;
                self.clamp();
                self.scroll = 0;
            }
            KeyCode::Char('t') if self.view == View::Instructions => {
                self.conditional = !self.conditional;
                self.scroll = 0;
            }
            KeyCode::Enter if self.view == View::Instructions => {
                self.preview = !self.preview;
                self.focus_details = true;
                self.scroll = 0;
            }
            KeyCode::Enter if self.view == View::Configuration && !self.sources => {
                if let Some((n, _)) = self.rows().get(self.selected[0])
                    && let Some(source) = self
                        .inventory
                        .as_ref()
                        .and_then(|i| i.configuration.settings.get(*n))
                        .and_then(|s| s.source.or_else(|| s.conditional.as_ref().map(|(_, i)| *i)))
                {
                    self.jump_source(source);
                }
            }
            KeyCode::Backspace => {
                if self.return_to_skill {
                    self.view = View::Skills;
                    self.return_to_skill = false;
                } else {
                    self.sources = false;
                    if let Some((selected, query)) = self.source_return.take() {
                        self.selected[0] = selected;
                        self.query = query;
                    }
                    self.preview = false;
                    self.focus_details = false;
                    self.clamp();
                }
                self.scroll = 0;
            }
            KeyCode::Esc => {
                self.help = false;
                self.searching = false;
                self.query.clear();
                self.clamp();
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_by(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_by(-1),
            KeyCode::PageDown => self.move_by(self.page.max(1) as isize),
            KeyCode::PageUp => self.move_by(-(self.page.max(1) as isize)),
            KeyCode::Home => {
                if self.focus_details || self.help {
                    self.scroll = 0;
                } else {
                    self.selected[self.slot()] = 0;
                }
            }
            KeyCode::End => {
                if self.focus_details || self.help {
                    self.scroll = self.scroll_max;
                } else {
                    self.selected[self.slot()] = self.rows().len().saturating_sub(1);
                }
            }
            _ => (),
        };
        false
    }
    fn move_by(&mut self, delta: isize) {
        if self.focus_details || self.help {
            self.scroll = (self.scroll as usize)
                .saturating_add_signed(delta)
                .min(self.scroll_max as usize) as u16;
        } else {
            let slot = self.slot();
            self.selected[slot] = self.selected[slot].saturating_add_signed(delta);
            self.clamp();
            self.scroll = 0;
            self.preview = false;
        }
    }
    pub fn focus(&self) -> Focus {
        if self.focus_details {
            Focus::Details
        } else {
            Focus::Inventory
        }
    }
}
