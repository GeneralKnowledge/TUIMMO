//! Semantic colours and border style.
//!
//! Games pick a palette. The engine default is a 16-colour neutral theme.
//! Coffee.shop uses [`Theme::undernet`]. `--ascii` selects the same roles with
//! named colours and plain box drawing, so a limited terminal stays legible.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;
use terminus_core::Channel;

/// A handful of roles. Widgets ask for a role instead of a raw colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Primary,
    Secondary,
    Muted,
    Accent,
    Discovery,
    Success,
    Warning,
    Error,
    Border,
    Focused,
    Text,
}

#[derive(Clone, Debug)]
pub struct Theme {
    primary: Color,
    secondary: Color,
    muted: Color,
    accent: Color,
    discovery: Color,
    success: Color,
    warning: Color,
    error: Color,
    border: Color,
    focused: Color,
    text: Color,
    background: Color,
    local: Color,
    global: Color,
    region: Color,
    whisper: Color,
    ascii: bool,
}

impl Theme {
    pub fn neutral() -> Self {
        Self::named(false)
    }

    /// Phosphor cyan and amber on a black field. Indexed colours when the
    /// terminal can show them; named colours when `ascii` is set.
    pub fn undernet(ascii: bool) -> Self {
        if ascii {
            return Self::named(true);
        }
        Self {
            primary: Color::Indexed(43),
            secondary: Color::Indexed(249),
            muted: Color::Indexed(245),
            accent: Color::Indexed(80),
            discovery: Color::Indexed(178),
            success: Color::Indexed(108),
            warning: Color::Indexed(215),
            error: Color::Indexed(167),
            border: Color::Indexed(23),
            focused: Color::Indexed(51),
            text: Color::Indexed(252),
            background: Color::Indexed(0),
            local: Color::Indexed(252),
            global: Color::Indexed(80),
            region: Color::Indexed(108),
            whisper: Color::Indexed(183),
            ascii: false,
        }
    }

    fn named(ascii: bool) -> Self {
        Self {
            primary: Color::Cyan,
            secondary: Color::Gray,
            muted: Color::DarkGray,
            accent: Color::Cyan,
            discovery: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            border: Color::DarkGray,
            focused: Color::Cyan,
            text: Color::White,
            background: Color::Black,
            local: Color::White,
            global: Color::Cyan,
            region: Color::Green,
            whisper: Color::Magenta,
            ascii,
        }
    }

    pub fn ascii(&self) -> bool {
        self.ascii
    }

    pub fn background(&self) -> Color {
        self.background
    }

    pub fn color(&self, role: Role) -> Color {
        match role {
            Role::Primary => self.primary,
            Role::Secondary => self.secondary,
            Role::Muted => self.muted,
            Role::Accent => self.accent,
            Role::Discovery => self.discovery,
            Role::Success => self.success,
            Role::Warning => self.warning,
            Role::Error => self.error,
            Role::Border => self.border,
            Role::Focused => self.focused,
            Role::Text => self.text,
        }
    }

    pub fn style(&self, role: Role) -> Style {
        Style::default().fg(self.color(role)).bg(self.background)
    }

    pub fn ink(&self) -> Style {
        Style::default().fg(self.text).bg(self.background)
    }

    pub fn border_type(&self) -> BorderType {
        if self.ascii {
            BorderType::Plain
        } else {
            BorderType::Rounded
        }
    }

    pub fn fg(&self, color: Color) -> Style {
        Style::default().fg(color).bg(self.background)
    }

    /// How one feed line should look. The tag is text, so the channel is still
    /// clear when colour is unavailable. Classification uses the channel and
    /// whether a speaker is present, not the message body.
    pub fn feed_style(&self, channel: Channel, speaker: bool) -> FeedStyle {
        match (channel, speaker) {
            (Channel::Local, true) => FeedStyle {
                body: self.fg(self.local),
                tag: "",
                tag_style: self.style(Role::Accent),
            },
            (Channel::Local, false) => FeedStyle {
                body: self.style(Role::Secondary).add_modifier(Modifier::ITALIC),
                tag: "*",
                tag_style: self.style(Role::Secondary).add_modifier(Modifier::ITALIC),
            },
            (Channel::Global, _) => FeedStyle {
                body: self.style(Role::Text),
                tag: "[all]",
                tag_style: self.fg(self.global),
            },
            (Channel::Region, _) => FeedStyle {
                body: self.style(Role::Text),
                tag: "[region]",
                tag_style: self.fg(self.region),
            },
            (Channel::Private, _) => FeedStyle {
                body: self.style(Role::Text),
                tag: "[whisper]",
                tag_style: self.fg(self.whisper),
            },
            (Channel::System, _) => FeedStyle {
                body: self.style(Role::Secondary),
                tag: "[note]",
                tag_style: self.fg(self.discovery),
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FeedStyle {
    pub body: Style,
    pub tag: &'static str,
    pub tag_style: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::neutral()
    }
}
