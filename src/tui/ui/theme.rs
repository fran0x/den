//! Colours by role, so every screen styles accents, secondary text and selection alike.
//! Catppuccin (Mocha, or Latte on a light terminal) needs 24-bit colour; other terminals
//! get the 16 ANSI colours, and `NO_COLOR` gets none. Backgrounds stay the terminal's
//! own except on the selected row; without colour, selection falls back to reverse video.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

/// `None` means "no colour for this role": the style falls back to a text modifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    accent: Option<Color>,
    muted: Option<Color>,
    key: Option<Color>,
    /// Healthy states, such as a running container.
    ok: Option<Color>,
    /// Waiting on someone, such as an approval.
    warn: Option<Color>,
    /// Something is wrong.
    bad: Option<Color>,
    /// Foreground and background of the selected row.
    selected: Option<(Color, Color)>,
    /// Homebrew casks, so they stand apart from formulae.
    cask: Option<Color>,
    /// What is in use: the active AWS profile, the current Kubernetes context.
    current: Option<Color>,
}

static THEME: OnceLock<Theme> = OnceLock::new();

/// Set the theme once, before the first draw. Later calls are ignored.
pub fn set_theme(theme: Theme) {
    let _ = THEME.set(theme);
}

/// The active theme; Catppuccin Mocha when none was set (tests and fixtures).
pub(super) fn theme() -> Theme {
    *THEME.get_or_init(Theme::mocha)
}

impl Theme {
    pub fn mocha() -> Self {
        Self {
            accent: Some(Color::Rgb(137, 180, 250)),
            muted: Some(Color::Rgb(127, 132, 156)),
            key: Some(Color::Rgb(205, 214, 244)),
            ok: Some(Color::Rgb(166, 227, 161)),
            warn: Some(Color::Rgb(249, 226, 175)),
            bad: Some(Color::Rgb(243, 139, 168)),
            selected: Some((Color::Rgb(205, 214, 244), Color::Rgb(49, 50, 68))),
            cask: Some(Color::Rgb(203, 166, 247)),
            current: Some(Color::Rgb(148, 226, 213)),
        }
    }

    pub fn latte() -> Self {
        Self {
            accent: Some(Color::Rgb(30, 102, 245)),
            muted: Some(Color::Rgb(140, 143, 161)),
            key: Some(Color::Rgb(76, 79, 105)),
            ok: Some(Color::Rgb(64, 160, 43)),
            warn: Some(Color::Rgb(223, 142, 29)),
            bad: Some(Color::Rgb(210, 15, 57)),
            selected: Some((Color::Rgb(76, 79, 105), Color::Rgb(204, 208, 218))),
            cask: Some(Color::Rgb(136, 57, 239)),
            current: Some(Color::Rgb(23, 146, 153)),
        }
    }

    /// 16-colour terminals: a blue accent; dim and reverse video for the rest.
    pub fn ansi() -> Self {
        Self {
            accent: Some(Color::Blue),
            muted: None,
            key: None,
            ok: Some(Color::Green),
            warn: Some(Color::Yellow),
            bad: Some(Color::Red),
            selected: None,
            cask: Some(Color::Magenta),
            current: Some(Color::Cyan),
        }
    }

    /// `NO_COLOR`: bold, dim, underline and reverse video only.
    pub fn plain() -> Self {
        Self {
            accent: None,
            muted: None,
            key: None,
            ok: None,
            warn: None,
            bad: None,
            selected: None,
            cask: None,
            current: None,
        }
    }

    pub fn from_env() -> Self {
        let var = |name| std::env::var(name).ok();
        Self::detect(
            var("NO_COLOR").as_deref(),
            var("COLORTERM").as_deref(),
            var("COLORFGBG").as_deref(),
        )
    }

    /// `COLORFGBG` is "fg;bg" (sometimes "fg;default;bg"); ANSI 7 and 9-15 are light.
    fn detect(no_color: Option<&str>, colorterm: Option<&str>, colorfgbg: Option<&str>) -> Self {
        if no_color.is_some_and(|v| !v.is_empty()) {
            return Self::plain();
        }
        if !matches!(colorterm, Some("truecolor" | "24bit")) {
            return Self::ansi();
        }
        let light = colorfgbg
            .and_then(|v| v.rsplit(';').next())
            .and_then(|bg| bg.parse::<u8>().ok())
            .is_some_and(|bg| matches!(bg, 7 | 9..=15));
        if light { Self::latte() } else { Self::mocha() }
    }

    /// Title, focused pane, headings.
    pub(super) fn accent(self) -> Style {
        self.accent
            .map_or_else(Style::default, |c| Style::default().fg(c))
    }

    /// Secondary text and hints.
    pub(super) fn muted(self) -> Style {
        match self.muted {
            Some(c) => Style::default().fg(c),
            None => Style::default().add_modifier(Modifier::DIM),
        }
    }

    /// Key names in action hints.
    pub(super) fn key(self) -> Style {
        self.key
            .map_or_else(Style::default, |c| Style::default().fg(c))
    }

    /// Healthy states; plain text without colour.
    pub(super) fn ok(self) -> Style {
        self.ok
            .map_or_else(Style::default, |c| Style::default().fg(c))
    }

    /// Waiting states; plain text without colour.
    pub(super) fn warn(self) -> Style {
        self.warn
            .map_or_else(Style::default, |c| Style::default().fg(c))
    }

    /// Problems; bold without colour.
    pub(super) fn bad(self) -> Style {
        match self.bad {
            Some(c) => Style::default().fg(c),
            None => Style::default().add_modifier(Modifier::BOLD),
        }
    }

    /// Cask names; italic without colour.
    pub(super) fn cask(self) -> Style {
        match self.cask {
            Some(c) => Style::default().fg(c),
            None => Style::default().add_modifier(Modifier::ITALIC),
        }
    }

    /// The profile or context in use; underlined without colour.
    pub(super) fn current(self) -> Style {
        match self.current {
            Some(c) => Style::default().fg(c).add_modifier(Modifier::BOLD),
            None => Style::default().add_modifier(Modifier::UNDERLINED | Modifier::BOLD),
        }
    }

    /// The selected row: a soft background, or reverse video without colour.
    pub(super) fn selection(self) -> Style {
        match self.selected {
            Some((fg, bg)) => Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            None => Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
        }
    }

    /// The selection highlight for rows that colour their own text (status lists):
    /// background and bold only, so status colours survive selection.
    pub(super) fn selection_background(self) -> Style {
        match self.selected {
            Some((_, bg)) => Style::default().bg(bg).add_modifier(Modifier::BOLD),
            None => Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
        }
    }

    /// Plain text on the selected row of such lists.
    pub(super) fn selected_text(self) -> Style {
        self.selected
            .map_or_else(Style::default, |(fg, _)| Style::default().fg(fg))
    }

    /// The active area in the top bar. Bold and underline keep it visible without colour.
    pub(super) fn active(self) -> Style {
        let style = self
            .accent()
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
        match self.selected {
            Some((_, bg)) => style.bg(bg),
            None => style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_follows_no_color_truecolor_and_background() {
        assert_eq!(
            Theme::detect(Some("1"), Some("truecolor"), None),
            Theme::plain()
        );
        assert_eq!(
            Theme::detect(Some(""), Some("truecolor"), None),
            Theme::mocha()
        );
        assert_eq!(Theme::detect(None, None, None), Theme::ansi());
        assert_eq!(Theme::detect(None, Some("256"), None), Theme::ansi());
        assert_eq!(
            Theme::detect(None, Some("24bit"), Some("0;15")),
            Theme::latte()
        );
        assert_eq!(
            Theme::detect(None, Some("truecolor"), Some("15;default;0")),
            Theme::mocha()
        );
        assert_eq!(
            Theme::detect(None, Some("truecolor"), Some("junk")),
            Theme::mocha()
        );
    }

    #[test]
    fn selection_is_visible_without_colour() {
        for theme in [Theme::ansi(), Theme::plain()] {
            assert!(theme.selection().add_modifier.contains(Modifier::REVERSED));
            assert!(theme.muted().add_modifier.contains(Modifier::DIM));
        }
        assert_eq!(Theme::plain().accent(), Style::default());
        assert!(Theme::plain().bad().add_modifier.contains(Modifier::BOLD));
        let active = Theme::plain().active().add_modifier;
        assert!(active.contains(Modifier::BOLD | Modifier::UNDERLINED));
    }

    #[test]
    fn casks_and_the_profile_in_use_stand_out_without_colour() {
        let plain = Theme::plain();
        assert!(plain.cask().add_modifier.contains(Modifier::ITALIC));
        assert!(plain.current().add_modifier.contains(Modifier::UNDERLINED));
        for theme in [Theme::mocha(), Theme::latte(), Theme::ansi()] {
            assert!(theme.cask().fg.is_some() && theme.current().fg.is_some());
            assert_ne!(theme.cask().fg, theme.current().fg);
        }
    }
}
