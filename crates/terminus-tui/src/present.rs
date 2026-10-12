//! Layout and line builders for the play screen.
//!
//! One function owns the split of a terminal rectangle. Widgets render into
//! those rectangles and do not measure the screen again.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType};
use terminus_core::{Channel, ConnectionState, ExitView, FeedLine, GameView};
use unicode_width::UnicodeWidthStr;

use crate::theme::{Role, Theme};

pub const PROMPT_HINT: &str = "up/down history   tab complete   pgup feed   help";

const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

#[derive(Clone, Copy, Debug)]
pub struct ScreenPlan {
    pub header: Rect,
    pub location: Rect,
    pub side: Option<Rect>,
    pub feed: Rect,
    pub prompt: Rect,
    pub bordered: bool,
    pub show_hint: bool,
}

/// `None` when the terminal cannot hold a header, one content row, and a prompt.
pub fn screen_plan(area: Rect) -> Option<ScreenPlan> {
    if area.width < 20 || area.height < 8 {
        return None;
    }
    let bordered = area.width >= 48 && area.height >= 16;
    let header_h: u16 = if bordered { 3 } else { 1 };
    let prompt_h: u16 = if bordered {
        3
    } else if area.height >= 12 {
        2
    } else {
        1
    };
    let body_h = area.height.saturating_sub(header_h.saturating_add(prompt_h));
    if body_h < 2 {
        return None;
    }
    let [header, body, prompt] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(body_h),
        Constraint::Length(prompt_h),
    ])
    .areas(area);

    let wide = area.width >= 100 && body_h >= 8;
    let (location, side, feed) = if wide {
        let loc_h = share(body_h, 45, 5, 12);
        let [top, feed] =
            Layout::vertical([Constraint::Length(loc_h), Constraint::Min(2)]).areas(body);
        let [location, side] =
            Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)]).areas(top);
        (location, Some(side), feed)
    } else {
        let loc_h = share(body_h, 46, 3, body_h.saturating_sub(2));
        let [location, feed] =
            Layout::vertical([Constraint::Length(loc_h), Constraint::Min(2)]).areas(body);
        (location, None, feed)
    };

    Some(ScreenPlan { header, location, side, feed, prompt, bordered, show_hint: prompt_h > 1 })
}

fn share(total: u16, percent: u16, min: u16, max: u16) -> u16 {
    let cap = max.min(total.saturating_sub(2)).max(1);
    let floor = min.min(cap);
    (total.saturating_mul(percent) / 100).clamp(floor, cap)
}

pub fn can_box(area: Rect, bordered: bool) -> bool {
    bordered && area.width >= 8 && area.height >= 3
}

pub fn panel<'a>(
    theme: &'a Theme,
    title: &'a str,
    area: Rect,
    bordered: bool,
) -> Option<Block<'a>> {
    if !can_box(area, bordered) {
        return None;
    }
    let mut block = Block::bordered().border_style(theme.style(Role::Border)).style(theme.ink());
    block = if theme.ascii() {
        block.border_set(ASCII_BORDER)
    } else {
        block.border_type(BorderType::Rounded)
    };
    if title.is_empty() {
        Some(block)
    } else {
        Some(block.title(Line::from(Span::styled(
            format!(" {title} "),
            theme.style(Role::Primary).add_modifier(Modifier::BOLD),
        ))))
    }
}

pub fn header_line(view: &GameView, theme: &Theme, width: usize) -> Line<'static> {
    let (state, state_role) = match view.connection {
        ConnectionState::Connected => ("connected", Role::Success),
        ConnectionState::Offline => ("offline", Role::Warning),
    };
    let mut budget = width.saturating_sub(state.len().saturating_add(1));
    let world = fit(&view.world_name, budget.min(24));
    budget = budget.saturating_sub(world.width());
    let who = if view.operator_name.is_empty() || budget < 3 {
        String::new()
    } else {
        fit(&view.operator_name, budget.saturating_sub(2).min(16))
    };
    let mut spans =
        vec![Span::styled(world, theme.style(Role::Primary).add_modifier(Modifier::BOLD))];
    if !who.is_empty() {
        spans.push(Span::styled("  ".to_string(), theme.ink()));
        spans.push(Span::styled(who, theme.style(Role::Accent)));
    }
    let used: usize = spans.iter().map(|span| span.content.width()).sum();
    let gap = width.saturating_sub(used.saturating_add(state.len()));
    if gap > 0 {
        spans.push(Span::styled(" ".repeat(gap), theme.ink()));
    }
    spans.push(Span::styled(state.to_string(), theme.style(state_role)));
    Line::from(spans)
}

pub fn location_lines(view: &GameView, theme: &Theme) -> Vec<Line<'static>> {
    if let Some(overlay) = &view.overlay {
        return overlay.lines().map(|line| Line::styled(line.to_string(), theme.ink())).collect();
    }
    let mut lines = Vec::new();
    if view.description.is_empty() {
        lines.push(Line::styled("Nothing is written here.".to_string(), theme.style(Role::Muted)));
    } else {
        for (index, paragraph) in view.description.split('\n').enumerate() {
            if index > 0 && paragraph.is_empty() {
                lines.push(Line::raw(""));
                continue;
            }
            lines.push(Line::styled(paragraph.to_string(), theme.ink()));
        }
    }
    lines.push(Line::raw(""));
    lines.push(labeled(theme, "exits", &exit_text(&view.exits), !view.exits.is_empty()));
    lines.push(labeled(theme, "here", &present_text(view), !view.present.is_empty()));
    lines.push(labeled(theme, "objects", &object_text(view), !view.objects.is_empty()));
    lines
}

fn exit_text(exits: &[ExitView]) -> String {
    if exits.is_empty() {
        return "none".into();
    }
    exits
        .iter()
        .map(|exit| format!("{} ({})", exit.keyword, exit.destination))
        .collect::<Vec<_>>()
        .join("  ")
}

fn present_text(view: &GameView) -> String {
    if view.present.is_empty() {
        "nobody else".into()
    } else {
        view.present.iter().map(|person| person.name.clone()).collect::<Vec<_>>().join(", ")
    }
}

fn object_text(view: &GameView) -> String {
    if view.objects.is_empty() {
        "nothing obvious".into()
    } else {
        view.objects.iter().map(|object| object.keyword.clone()).collect::<Vec<_>>().join("  ")
    }
}

fn labeled(theme: &Theme, label: &str, value: &str, accent_value: bool) -> Line<'static> {
    let value_style =
        if accent_value { theme.style(Role::Accent) } else { theme.style(Role::Muted) };
    Line::from(vec![
        Span::styled(format!("{label}  "), theme.style(Role::Muted)),
        Span::styled(value.to_string(), value_style),
    ])
}

pub fn side_lines(view: &GameView, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let name = if view.operator_name.is_empty() {
        "no operator".to_string()
    } else {
        view.operator_name.clone()
    };
    lines.push(Line::styled(name, theme.style(Role::Accent).add_modifier(Modifier::BOLD)));
    if view.status.is_empty() {
        lines.push(Line::styled("no status".to_string(), theme.style(Role::Muted)));
    } else {
        for line in &view.status {
            lines.push(Line::styled(line.clone(), theme.style(Role::Secondary)));
        }
    }
    if !view.inventory.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled("carried".to_string(), theme.style(Role::Muted)));
        for item in view.inventory.iter().take(6) {
            lines.push(Line::styled(format!("  {}", item.name), theme.ink()));
        }
    }
    lines
}

pub fn feed_lines(
    feed: &[FeedLine],
    height: usize,
    scroll: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if height == 0 {
        return Vec::new();
    }
    if feed.is_empty() {
        return vec![Line::styled("quiet".to_string(), theme.style(Role::Muted))];
    }
    let max_scroll = feed.len().saturating_sub(height);
    let scroll = scroll.min(max_scroll);
    let end = feed.len().saturating_sub(scroll);
    let start = end.saturating_sub(height);
    feed[start..end].iter().map(|line| feed_line(line, theme)).collect()
}

pub fn feed_line(line: &FeedLine, theme: &Theme) -> Line<'static> {
    let styled = theme.feed_style(line.channel, line.speaker.is_some());
    let mut spans =
        vec![Span::styled(format!("{} ", format_hms(line.at_ms)), theme.style(Role::Muted))];
    if !styled.tag.is_empty() {
        spans.push(Span::styled(format!("{} ", styled.tag), styled.tag_style));
    }
    if let Some(name) = &line.speaker {
        if matches!(
            line.channel,
            Channel::Local | Channel::Global | Channel::Region | Channel::Private
        ) {
            spans.push(Span::styled(format!("{name}: "), theme.style(Role::Accent)));
        }
    }
    spans.push(Span::styled(line.text.clone(), styled.body));
    Line::from(spans)
}

pub fn format_hms(ms: u64) -> String {
    let secs = ms / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

pub fn inner_size(area: Rect, boxed: bool) -> (usize, usize) {
    if boxed {
        (usize::from(area.width.saturating_sub(2)), usize::from(area.height.saturating_sub(2)))
    } else {
        (usize::from(area.width), usize::from(area.height))
    }
}

pub fn fit(text: &str, width: usize) -> String {
    if width == 0 || text.is_empty() {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    let ellipsis = "...";
    let budget = width.saturating_sub(ellipsis.len());
    if budget == 0 {
        return ellipsis.chars().take(width).collect();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let next =
            out.width().saturating_add(unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0));
        if next > budget {
            break;
        }
        out.push(ch);
    }
    out.push_str(ellipsis);
    out
}

/// Keep the cursor on screen by showing the tail of a long command.
pub fn input_window(text: &str, cursor_chars: usize, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor_chars.min(chars.len());
    let full: String = chars.iter().collect();
    if full.width() <= width {
        let shown: String = chars.iter().take(cursor).collect();
        return (full, shown.width());
    }
    let end = cursor;
    let mut start = end;
    let mut used = 0usize;
    while start > 0 {
        let ch = chars[start - 1];
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used.saturating_add(w) > width {
            break;
        }
        used = used.saturating_add(w);
        start -= 1;
    }
    let visible: String = chars[start..end].iter().collect();
    let cursor_cells = visible.width();
    (visible, cursor_cells)
}

pub fn heading(theme: &Theme, title: &str) -> Line<'static> {
    Line::styled(title.to_string(), theme.style(Role::Primary).add_modifier(Modifier::BOLD))
}

#[cfg(test)]
mod tests {
    use super::*;
    use terminus_core::{Channel, ConnectionState, ExitView, PresenceView};

    #[test]
    fn layout_tracks_width_and_height() {
        assert!(screen_plan(Rect::new(0, 0, 19, 20)).is_none());
        assert!(screen_plan(Rect::new(0, 0, 80, 7)).is_none());

        let narrow = screen_plan(Rect::new(0, 0, 60, 24)).unwrap();
        assert!(narrow.side.is_none());
        assert!(narrow.bordered);

        let medium = screen_plan(Rect::new(0, 0, 80, 30)).unwrap();
        assert!(medium.side.is_none());

        let wide = screen_plan(Rect::new(0, 0, 110, 32)).unwrap();
        assert!(wide.side.is_some());

        let short = screen_plan(Rect::new(0, 0, 80, 12)).unwrap();
        assert!(!short.bordered);
        assert_eq!(short.header.height, 1);
        assert!(short.show_hint);

        let tiny = screen_plan(Rect::new(0, 0, 40, 8)).unwrap();
        assert_eq!(tiny.prompt.height, 1);
        assert!(!tiny.show_hint);

        let sum =
            wide.header.height + wide.location.height.max(wide.feed.height) + wide.prompt.height;
        assert!(sum <= 32);
        assert_eq!(wide.header.y + wide.header.height, wide.location.y);
    }

    #[test]
    fn feed_tags_follow_the_channel() {
        let theme = Theme::undernet(true);
        let speech = theme.feed_style(Channel::Local, true);
        let emote = theme.feed_style(Channel::Local, false);
        let global = theme.feed_style(Channel::Global, true);
        let region = theme.feed_style(Channel::Region, false);
        let private = theme.feed_style(Channel::Private, true);
        let system = theme.feed_style(Channel::System, false);
        assert_eq!(speech.tag, "");
        assert_eq!(emote.tag, "*");
        assert_eq!(global.tag, "[all]");
        assert_eq!(region.tag, "[region]");
        assert_eq!(private.tag, "[whisper]");
        assert_eq!(system.tag, "[note]");
        assert_ne!(global.tag_style.fg, system.tag_style.fg);
        assert_ne!(private.tag_style.fg, speech.body.fg);

        let line = FeedLine {
            at_ms: 3_600_000,
            channel: Channel::Global,
            speaker: Some("ada".into()),
            text: "the roof is open".into(),
        };
        let rendered = feed_line(&line, &theme);
        let text = rendered.spans.iter().map(|span| span.content.clone()).collect::<String>();
        assert!(text.contains("[all]"));
        assert!(text.contains("ada:"));
        assert!(text.contains("01:00:00"));
        assert!(text.contains("the roof is open"));
    }

    #[test]
    fn long_text_is_clipped_to_the_budget() {
        let fitted = fit(&"n".repeat(80), 12);
        assert!(fitted.width() <= 12, "{fitted}");
        assert!(fitted.ends_with("..."));
        assert_eq!(fit("short", 0), "");
        let (visible, cursor) = input_window(&"look ".repeat(30), 120, 10);
        assert!(visible.width() <= 10, "{visible}");
        assert!(cursor <= 10);
    }

    #[test]
    fn location_lines_keep_exits_and_overlay() {
        let theme = Theme::neutral();
        let mut view = GameView::empty("Undernet");
        view.connection = ConnectionState::Connected;
        view.description = "Steam.\n\nA counter.".into();
        view.exits = vec![ExitView {
            keyword: "alley".into(),
            label: "Neon Alley".into(),
            destination: "Neon Alley".into(),
        }];
        view.present = vec![PresenceView { name: "bob".into() }];
        let lines = location_lines(&view, &theme);
        let text = join_lines(&lines);
        assert!(text.contains("Steam."));
        assert!(text.contains("alley"));
        assert!(text.contains("bob"));
        view.overlay = Some("@ Roof Array\n  Coffee Shop".into());
        let overlay = location_lines(&view, &theme);
        let overlay_text = join_lines(&overlay);
        assert!(overlay_text.contains("@ Roof Array"));
        assert!(!overlay_text.contains("exits"));
    }

    fn join_lines(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|span| span.content.clone()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
