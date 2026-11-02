use super::*;
use crate::{
    app::Focus,
    inventory::{matches_query, safe_text},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Overview,
    Instructions,
    Files,
    Findings,
    Coverage,
}
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
pub struct State {
    pub config: Option<Config>,
    pub inventory: Inventory,
    pub enablement: Vec<crate::codex::Enablement>,
    pub requested: bool,
    pub running: bool,
    pub loaded: bool,
    pub error: Option<String>,
    pub query: String,
    pub searching: bool,
    pub scope: Option<Scope>,
    pub filter: Option<Filter>,
    pub visible: Vec<usize>,
    pub selected: Option<usize>,
    pub focus: Focus,
    pub section: Section,
    pub file: usize,
    pub file_open: bool,
    pub scroll: u16,
    pub scroll_max: u16,
    pub page: usize,
    pub help: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            config: None,
            inventory: Inventory::default(),
            enablement: vec![],
            requested: false,
            running: false,
            loaded: false,
            error: None,
            query: String::new(),
            searching: false,
            scope: None,
            filter: None,
            visible: vec![],
            selected: None,
            focus: Focus::Inventory,
            section: Section::Overview,
            file: 0,
            file_open: false,
            scroll: 0,
            scroll_max: 0,
            page: 1,
            help: false,
        }
    }
}
impl State {
    pub fn configure(&mut self, config: Config) {
        self.config = Some(config);
        self.requested = true;
    }
    pub fn selected_skill(&self) -> Option<&Skill> {
        self.selected
            .and_then(|i| self.visible.get(i))
            .map(|&n| &self.inventory.skills[n])
    }
    pub fn refresh_visible(&mut self) {
        let old = self.selected_skill().map(|s| (s.scope, s.path.clone()));
        self.visible = self
            .inventory
            .skills
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let status = match self.filter {
                    None => true,
                    Some(Filter::Valid) => s.metadata == Validity::Valid,
                    Some(Filter::Invalid) => s.metadata == Validity::Invalid,
                    Some(Filter::NotEvaluated) => s.metadata == Validity::NotEvaluated,
                    Some(Filter::Findings) => !s.findings.is_empty(),
                    Some(Filter::Duplicate) => s.duplicate,
                    Some(Filter::Inaccessible) => !s.accessible,
                };
                (self.scope.is_none_or(|scope| scope == s.scope)
                    && status
                    && matches_query(&self.query, &format!("{} {}", s.name, s.description)))
                .then_some(i)
            })
            .collect();
        self.selected = old
            .and_then(|path| {
                self.visible.iter().position(|&n| {
                    (
                        self.inventory.skills[n].scope,
                        self.inventory.skills[n].path.clone(),
                    ) == path
                })
            })
            .or_else(|| (!self.visible.is_empty()).then_some(0));
        self.scroll = 0;
        self.file = 0;
        self.file_open = false;
    }
    pub fn finish(&mut self, result: io::Result<Inventory>) {
        self.running = false;
        match result {
            Ok(inventory) => {
                let path = self.selected_skill().map(|s| (s.scope, s.path.clone()));
                let file = self
                    .selected_skill()
                    .and_then(|s| s.files.get(self.file))
                    .map(|f| f.path.clone());
                let scroll = self.scroll;
                let open = self.file_open;
                self.inventory = inventory;
                self.selected = None;
                self.refresh_visible();
                if let Some(index) = path.and_then(|path| {
                    self.visible.iter().position(|&n| {
                        (
                            self.inventory.skills[n].scope,
                            self.inventory.skills[n].path.clone(),
                        ) == path
                    })
                }) {
                    self.selected = Some(index);
                    self.scroll = scroll;
                    if let Some(file) = file.and_then(|path| {
                        self.selected_skill()
                            .and_then(|s| s.files.iter().position(|f| f.path == path))
                    }) {
                        self.file = file;
                        self.file_open = open;
                    }
                }
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
                "{}{} discovered skills; config not evaluated | 5:coverage",
                if self.inventory.partial {
                    "PARTIAL: "
                } else {
                    ""
                },
                self.inventory.skills.len()
            )
        } else {
            "Not collected; r:discover".into()
        }
    }
    pub fn handle(&mut self, key: KeyEvent) -> bool {
        if self.help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => {
                    self.help = false;
                    self.scroll = 0;
                }
                KeyCode::Char('q') => return true,
                _ => self.movement(key.code),
            };
            return false;
        }
        if key.code == KeyCode::F(1) {
            self.help = true;
            self.scroll = 0;
            return false;
        }
        if self.searching {
            match key.code {
                KeyCode::Enter | KeyCode::Esc => self.searching = false,
                KeyCode::Backspace => {
                    self.query.pop();
                    self.refresh_visible();
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.query.clear();
                    self.refresh_visible();
                }
                KeyCode::Char(ch)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && self.query.len() < 256
                        && safe_text(&ch.to_string()) == ch.to_string() =>
                {
                    self.query.push(ch);
                    self.refresh_visible();
                }
                _ => {}
            }
            return false;
        }
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') => {
                self.help = true;
                self.scroll = 0;
            }
            KeyCode::Char('/') => {
                self.searching = true;
                self.focus = Focus::Inventory;
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Inventory {
                    Focus::Details
                } else {
                    Focus::Inventory
                }
            }
            KeyCode::Char(ch @ '1'..='5') => {
                self.section = match ch {
                    '1' => Section::Overview,
                    '2' => Section::Instructions,
                    '3' => Section::Files,
                    '4' => Section::Findings,
                    _ => Section::Coverage,
                };
                self.focus = Focus::Details;
                self.scroll = 0;
                self.file_open = false;
            }
            KeyCode::Char('f') => {
                self.scope = match self.scope {
                    None => Some(Scope::ALL[0]),
                    Some(scope) => Scope::ALL
                        .iter()
                        .position(|&s| s == scope)
                        .and_then(|i| Scope::ALL.get(i + 1))
                        .copied(),
                };
                self.refresh_visible();
            }
            KeyCode::Char('i') => {
                self.filter = match self.filter {
                    None => Some(Filter::Valid),
                    Some(Filter::Valid) => Some(Filter::Invalid),
                    Some(Filter::Invalid) => Some(Filter::NotEvaluated),
                    Some(Filter::NotEvaluated) => Some(Filter::Findings),
                    Some(Filter::Findings) => Some(Filter::Duplicate),
                    Some(Filter::Duplicate) => Some(Filter::Inaccessible),
                    _ => None,
                };
                self.refresh_visible();
            }
            KeyCode::Char('r') if self.config.is_some() && !self.running => self.requested = true,
            KeyCode::Enter | KeyCode::Right => {
                if self.focus == Focus::Details && self.section == Section::Files {
                    self.file_open = true;
                    self.scroll = 0;
                }
                self.focus = Focus::Details;
            }
            KeyCode::Backspace | KeyCode::Left => {
                if self.file_open {
                    self.file_open = false;
                    self.scroll = 0;
                } else {
                    self.focus = Focus::Inventory;
                }
            }
            KeyCode::Esc => {
                self.query.clear();
                self.scope = None;
                self.filter = None;
                self.refresh_visible();
            }
            code => self.movement(code),
        }
        false
    }
    fn movement(&mut self, code: KeyCode) {
        let delta = match code {
            KeyCode::Down | KeyCode::Char('j') => 1,
            KeyCode::Up | KeyCode::Char('k') => -1,
            KeyCode::PageDown => self.page as isize,
            KeyCode::PageUp => -(self.page as isize),
            KeyCode::Home => -isize::MAX,
            KeyCode::End => isize::MAX,
            _ => return,
        };
        if self.help || self.focus == Focus::Details {
            if !self.help && self.section == Section::Files && !self.file_open {
                let len = self.selected_skill().map_or(0, |s| s.files.len());
                self.file = self
                    .file
                    .saturating_add_signed(delta)
                    .min(len.saturating_sub(1));
            } else {
                self.scroll = (self.scroll as usize)
                    .saturating_add_signed(delta)
                    .min(self.scroll_max as usize) as u16;
            }
        } else if let Some(selected) = self.selected {
            self.selected = Some(
                selected
                    .saturating_add_signed(delta)
                    .min(self.visible.len().saturating_sub(1)),
            );
            if self.selected != Some(selected) {
                self.scroll = 0;
                self.file = 0;
                self.file_open = false;
            }
        }
    }
}
