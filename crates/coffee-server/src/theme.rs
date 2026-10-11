use ratatui::style::Color;
use terminus_tui::Theme;

/// Amber and cyan palette used by Coffee.shop. The engine default stays neutral.
pub fn coffee_theme(ascii: bool) -> Theme {
    Theme {
        title: Color::Rgb(255, 196, 87),
        border: Color::Rgb(64, 112, 122),
        text: Color::Rgb(226, 220, 206),
        accent: Color::Rgb(120, 220, 204),
        muted: Color::Rgb(120, 124, 132),
        warn: Color::Rgb(255, 150, 90),
        local: Color::Rgb(236, 226, 206),
        global: Color::Rgb(125, 214, 230),
        region: Color::Rgb(150, 196, 140),
        private: Color::Rgb(196, 160, 220),
        system: Color::Rgb(150, 158, 166),
        ascii,
    }
}
