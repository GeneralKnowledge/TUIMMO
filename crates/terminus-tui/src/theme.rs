//! Colours and border style. Games supply a palette; the engine default is neutral.

use ratatui::style::Color;
use ratatui::widgets::BorderType;

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
            ascii: false,
        }
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
