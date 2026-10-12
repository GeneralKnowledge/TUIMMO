//! Colours and border style. Games supply a palette; the engine default is neutral.

use ratatui::style::{Color, Style};
use ratatui::widgets::BorderType;

/// Draw the palette even when `NO_COLOR` is set.
///
/// `NO_COLOR` is for incidental CLI colour. This UI is the whole screen: without
/// the ground and the ink, a light terminal stays a white page.
pub fn enable_theme_colors() {
    crossterm::style::force_color_output(true);
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub title: Color,
    pub border: Color,
    pub text: Color,
    pub accent: Color,
    pub muted: Color,
    pub warn: Color,
    pub local: Color,
    pub global: Color,
    pub region: Color,
    pub private: Color,
    pub system: Color,
    /// Cell background. [`Color::Reset`] follows the terminal.
    pub background: Color,
    pub ascii: bool,
}

impl Theme {
    pub fn neutral() -> Self {
        Self {
            title: Color::White,
            border: Color::Gray,
            text: Color::White,
            accent: Color::Cyan,
            muted: Color::DarkGray,
            warn: Color::Yellow,
            local: Color::White,
            global: Color::Cyan,
            region: Color::Green,
            private: Color::Magenta,
            system: Color::DarkGray,
            background: Color::Reset,
            ascii: false,
        }
    }

    /// Foreground drawn on the theme ground.
    pub fn ink(&self, foreground: Color) -> Style {
        Style::default().fg(foreground).bg(self.background)
    }

    pub fn border_type(&self) -> BorderType {
        if self.ascii {
            BorderType::Plain
        } else {
            BorderType::Rounded
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::neutral()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ratatui::backend::{Backend, CrosstermBackend};
    use ratatui::buffer::Cell;

    use super::*;

    #[test]
    fn themed_cells_keep_their_ground_when_no_color_is_set() {
        crossterm::style::Colored::set_ansi_color_disabled(true);
        enable_theme_colors();
        let mut cell = Cell::EMPTY;
        cell.set_symbol("A");
        cell.set_fg(Color::Rgb(226, 220, 206));
        cell.set_bg(Color::Rgb(18, 16, 14));
        let mut writer = Cursor::new(Vec::new());
        let mut backend = CrosstermBackend::new(&mut writer);
        backend.draw(std::iter::once((0, 0, &cell))).unwrap();
        let text = String::from_utf8(writer.into_inner()).unwrap();
        assert!(text.contains("38;2;226;220;206"), "{text}");
        assert!(text.contains("48;2;18;16;14"), "{text}");
    }
}
