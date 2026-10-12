use terminus_tui::Theme;

/// Amber and cyan palette used by Coffee.shop. The engine default stays neutral.
///
/// `ascii` selects named colours and plain borders for terminals without a
/// 256-colour palette. The default uses indexed colours, not true colour.
pub fn coffee_theme(ascii: bool) -> Theme {
    Theme::undernet(ascii)
}
