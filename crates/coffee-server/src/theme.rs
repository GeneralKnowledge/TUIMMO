use ratatui::style::Color;
use terminus_tui::Theme;

/// Warm near-black behind the amber and cyan foregrounds.
const GROUND: Color = Color::Rgb(18, 16, 14);

/// Amber and cyan on a dark ground. The engine default stays neutral.
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
        background: GROUND,
        ascii,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use terminus_core::GameView;
    use terminus_tui::App;

    #[test]
    fn coffee_screen_paints_a_dark_ground() {
        let theme = coffee_theme(false);
        assert_eq!(theme.background, GROUND);
        assert_ne!(theme.background, Color::White);
        assert_ne!(theme.background, Color::Reset);
        let app = App::play(theme.clone(), GameView::empty("Undernet"));
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area().height {
            for x in 0..buffer.area().width {
                assert_eq!(
                    buffer[(x, y)].bg,
                    GROUND,
                    "cell {x},{y} is {:?}",
                    buffer[(x, y)].symbol()
                );
            }
        }
    }
}
