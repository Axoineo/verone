//! The Verone logo for the terminal, shown by `verone --version`.
//!
//! The drawing is a copy of `docs/brand/terminal/logo-large.txt`, the design source.

use std::io::{IsTerminal, Write};

/// The mark: an Android phone rising out of a laptop.
const LOGO: [&str; 9] = [
    "        ╭──────╮",
    "        │  ──  │",
    "  ╭─────┤      ├─────╮",
    "  │     │ ╲  ╱ │     │",
    "  │     │  ╲╱  │     │",
    "  │     │      │     │",
    "  │     ╰──────╯     │",
    "  ╰──────────────────╯",
    " ━━━━━━━━━━━━━━━━━━━━━━",
];

/// Width of the logo column, in terminal cells; the text starts after it.
const LOGO_WIDTH: usize = 24;

/// The line of the logo the text beside it starts on.
const TEXT_LINE: usize = 3;

/// Verone green, as the nearest colour in the 256-colour palette.
const GREEN: &str = "\x1b[38;5;149m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// How `--version` is printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// One line, `verone 0.1.0`, for scripts and pipes.
    Plain,
    /// The logo with the version beside it, without colour.
    Logo,
    /// The logo with the version beside it, in colour.
    Colour,
}

impl Style {
    /// Chooses the style: the logo only on a terminal, and colour only when
    /// `NO_COLOR` is unset or empty (see <https://no-color.org>).
    pub fn detect() -> Self {
        let no_colour = std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty());
        Self::choose(std::io::stdout().is_terminal(), no_colour)
    }

    fn choose(terminal: bool, no_colour: bool) -> Self {
        match (terminal, no_colour) {
            (false, _) => Style::Plain,
            (true, true) => Style::Logo,
            (true, false) => Style::Colour,
        }
    }
}

/// The text `verone --version` prints, ending in a newline.
pub fn version_text(style: Style) -> String {
    let version = env!("CARGO_PKG_VERSION");
    if style == Style::Plain {
        return format!("verone {version}\n");
    }
    let (green, bold, dim, reset) = if style == Style::Colour {
        (GREEN, BOLD, DIM, RESET)
    } else {
        ("", "", "", "")
    };
    let text = [
        format!("{bold}{green}verone{reset} {version}"),
        format!("{dim}a virtual Android phone{reset}"),
        format!("{dim}on your PC, laptop or server{reset}"),
    ];
    let mut out = String::new();
    for (index, line) in LOGO.iter().enumerate() {
        let beside = index
            .checked_sub(TEXT_LINE)
            .and_then(|position| text.get(position));
        let mut row = format!("{green}{line}{reset}");
        if let Some(beside) = beside {
            let padding = LOGO_WIDTH.saturating_sub(line.chars().count());
            row.push_str(&" ".repeat(padding));
            row.push_str(beside);
        }
        out.push_str(&row);
        out.push('\n');
    }
    out
}

/// Prints the version in the style that suits where standard output goes.
pub fn print_version() {
    // A closed pipe (`verone --version | head -c1`) is not worth an error.
    let _ = std::io::stdout().write_all(version_text(Style::detect()).as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_matches_the_design_source() {
        let source = include_str!("../../../docs/brand/terminal/logo-large.txt");
        assert_eq!(LOGO.to_vec(), source.lines().collect::<Vec<_>>());
    }

    #[test]
    fn the_logo_fits_its_column() {
        for line in LOGO {
            assert!(line.chars().count() < LOGO_WIDTH, "too wide: {line:?}");
        }
    }

    #[test]
    fn plain_style_is_one_line() {
        assert_eq!(
            version_text(Style::Plain),
            format!("verone {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn logo_style_shows_the_version_beside_the_logo_without_colour() {
        let text = version_text(Style::Logo);
        assert!(!text.contains('\x1b'));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), LOGO.len());
        let version_line = format!("verone {}", env!("CARGO_PKG_VERSION"));
        assert!(lines[TEXT_LINE].ends_with(&version_line));
        assert_eq!(
            lines[TEXT_LINE].chars().count(),
            LOGO_WIDTH + version_line.chars().count()
        );
        assert!(lines[TEXT_LINE + 2].ends_with("on your PC, laptop or server"));
        assert!(lines.iter().all(|line| line == &line.trim_end()));
    }

    #[test]
    fn colour_style_resets_every_colour_it_sets() {
        let text = version_text(Style::Colour);
        assert!(text.contains(GREEN));
        for line in text.lines() {
            // The last escape sequence on every line must be the reset.
            assert_eq!(
                line.rfind('\x1b'),
                line.rfind(RESET),
                "colour left on: {line:?}"
            );
        }
    }

    #[test]
    fn the_logo_is_only_shown_on_a_terminal_and_colour_follows_no_color() {
        assert_eq!(Style::choose(false, false), Style::Plain);
        assert_eq!(Style::choose(false, true), Style::Plain);
        assert_eq!(Style::choose(true, true), Style::Logo);
        assert_eq!(Style::choose(true, false), Style::Colour);
    }
}
