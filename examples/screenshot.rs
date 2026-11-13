//! Renders a README screenshot as SVG from the fixtures: `just screenshots` writes them to
//! docs/images. Run one with `cargo run --example screenshot -- dew > dew.svg`.
//!
//! Screens: `toolchains`, `agents-mcp` and `dew`.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use den::{
    actions::{Action, Package, Permit, State},
    app::App,
    areas::{
        agents::{claude, codex},
        homebrew::{self, CommandOutput},
        toolchains,
    },
    inventory::{HomebrewMetadata, Inventory},
    tui::{presentation::Destination, ui},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};
use std::{path::Path, sync::atomic::AtomicBool};

const WIDTH: u16 = 120;
const HEIGHT: u16 = 26;
const BACKGROUND: &str = "#1e1e2e";
const FOREGROUND: &str = "#cdd6f4";
const TITLE_BAR: &str = "#181825";

fn key(app: &mut App, code: KeyCode) {
    app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn main() {
    let screen = std::env::args().nth(1).unwrap_or_else(|| "dew".into());
    ui::set_theme(ui::Theme::mocha());
    let mut app = match screen.as_str() {
        "toolchains" => toolchains_screen(),
        "agents-mcp" => agents_screen(),
        "dew" => dew_screen(),
        other => panic!("unknown screen {other}: use toolchains, agents-mcp or dew"),
    };
    let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
    terminal.draw(|f| ui::render(f, &mut app)).unwrap();
    print!("{}", svg(terminal.backend().buffer()));
}

fn empty() -> App {
    App::new(
        Inventory {
            schema_version: 1,
            resources: vec![],
        },
        "fixture",
    )
}

/// Toolchains › PATH, with python3 (hidden by the system copy) selected.
fn toolchains_screen() -> App {
    let mut app = empty();
    let context = toolchains::Context::fixture(Path::new("fixtures/toolchains")).unwrap();
    app.toolchains.configure(context.clone());
    app.toolchains
        .finish(toolchains::discover(&context, &AtomicBool::new(false)));
    app.toolchains.requested = false;
    app.navigate(Destination::OnPath);
    app.toolchains.select_key("command:python3");
    app
}

/// Agents › MCP: Codex and Claude Code compared row by row.
fn agents_screen() -> App {
    let mut app = empty();
    let root = Path::new("fixtures/claude");
    let cancel = AtomicBool::new(false);
    let context = codex::Context::fixture(root, None).unwrap();
    app.codex.configure(context.clone());
    app.codex
        .finish(codex::discover(&context, &cancel), &mut app.skills);
    app.codex.requested = false;
    let context = claude::Context::fixture(root).unwrap();
    app.claude.configure(context.clone());
    app.claude.finish(claude::discover(&context, &cancel));
    app.claude.requested = false;
    app.navigate(Destination::Mcp);
    app
}

/// dew: the actions for ripgrep, with the upgrade's dry run shown.
fn dew_screen() -> App {
    let mut inventory = homebrew::load_snapshot(Path::new("fixtures/brew.json")).unwrap();
    // The fixture's placeholder version (99.0) reads oddly in a picture.
    for resource in &mut inventory.resources {
        if resource.name == "ripgrep"
            && let Some(HomebrewMetadata::Formula { latest, .. }) = &mut resource.homebrew
        {
            *latest = Some("15.1.0".into());
        }
    }
    let mut app = App::new(inventory, "Homebrew");
    app.actions = State::new(Permit::granted(true));
    key(&mut app, KeyCode::Char('/'));
    for c in "ripgrep".chars() {
        key(&mut app, KeyCode::Char(c));
    }
    key(&mut app, KeyCode::Enter);
    key(&mut app, KeyCode::Char('x'));
    app.actions.finish_preview(
        &Action::BrewUpgrade(Package::new("ripgrep", false).unwrap()),
        Ok(CommandOutput {
            code: Some(0),
            stdout: b"==> Would upgrade 1 outdated package:\nripgrep 14.1.1 -> 15.1.0\n".to_vec(),
            stderr: vec![],
        }),
    );
    app
}

fn hex(color: Color, default: &str) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        _ => default.into(),
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A terminal window: Catppuccin Mocha, one `<text>` per word placed by its cell, so the
/// columns line up whatever the viewer's font.
fn svg(buffer: &ratatui::buffer::Buffer) -> String {
    let (cell_w, cell_h, pad, bar) = (8.4f32, 18.0f32, 16.0f32, 30.0f32);
    let width = f32::from(WIDTH) * cell_w + pad * 2.0;
    let height = f32::from(HEIGHT) * cell_h + pad * 2.0 + bar;
    let mut out = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' viewBox='0 0 {width} {height}'>\
<rect width='100%' height='100%' rx='10' fill='{BACKGROUND}'/>\
<path d='M0 10a10 10 0 0 1 10-10h{}a10 10 0 0 1 10 10v{}h-{width}z' fill='{TITLE_BAR}'/>\
<circle cx='20' cy='15' r='6' fill='#f38ba8'/><circle cx='40' cy='15' r='6' fill='#f9e2af'/><circle cx='60' cy='15' r='6' fill='#a6e3a1'/>\
<g font-family='ui-monospace, SFMono-Regular, Menlo, Consolas, monospace' font-size='14' xml:space='preserve'>",
        width - 20.0,
        bar - 10.0,
    );
    let colours = |x: u16, y: u16| {
        let cell = &buffer[(x, y)];
        let (mut fg, mut bg) = (hex(cell.fg, FOREGROUND), hex(cell.bg, BACKGROUND));
        if cell.modifier.contains(Modifier::REVERSED) {
            std::mem::swap(&mut fg, &mut bg);
        }
        (fg, bg, cell.modifier)
    };
    let (left, top) = (pad, bar + pad);
    for y in 0..HEIGHT {
        let mut x = 0;
        while x < WIDTH {
            let (fg, bg, modifier) = colours(x, y);
            let start = x;
            let mut text = String::new();
            while x < WIDTH && colours(x, y) == (fg.clone(), bg.clone(), modifier) {
                text.push_str(buffer[(x, y)].symbol());
                x += 1;
            }
            if bg != BACKGROUND {
                out += &format!(
                    "<rect x='{}' y='{}' width='{}' height='{cell_h}' fill='{bg}'/>",
                    left + f32::from(start) * cell_w,
                    top + f32::from(y) * cell_h,
                    f32::from(x - start) * cell_w
                );
            }
            let dim = if modifier.contains(Modifier::DIM) {
                " opacity='0.55'"
            } else {
                ""
            };
            let bold = if modifier.contains(Modifier::BOLD) {
                " font-weight='bold'"
            } else {
                ""
            };
            let underline = if modifier.contains(Modifier::UNDERLINED) {
                " text-decoration='underline'"
            } else {
                ""
            };
            let cells: Vec<char> = text.chars().collect();
            let mut i = 0;
            while i < cells.len() {
                if cells[i] == ' ' {
                    i += 1;
                    continue;
                }
                let j = (i..cells.len())
                    .find(|&j| cells[j] == ' ')
                    .unwrap_or(cells.len());
                let word: String = cells[i..j].iter().collect();
                out += &format!(
                    "<text x='{}' y='{}' fill='{fg}' textLength='{}' lengthAdjust='spacingAndGlyphs'{dim}{bold}{underline}>{}</text>",
                    left + (usize::from(start) + i) as f32 * cell_w,
                    top + f32::from(y) * cell_h + 13.5,
                    (j - i) as f32 * cell_w,
                    escape(&word)
                );
                i = j;
            }
        }
    }
    out += "</g></svg>";
    out
}
