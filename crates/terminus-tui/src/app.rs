//! Screen state: header, location, status, feed, and the command prompt.

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;
use terminus_core::{CommandHint, ConnectionState, ExitView, FeedLine, GameView, Key};
use unicode_width::UnicodeWidthStr;

use crate::editor::LineEditor;
use crate::onboard::{Gate, Onboard, OnboardEffect};
use crate::theme::Theme;

#[derive(Clone, Debug)]
pub enum Screen {
    Onboard(Onboard),
    Play,
}

pub enum Effect {
    None,
    SubmitCommand(String),
    SubmitGuest {
        username: String,
        operator_name: String,
    },
    SubmitLogin {
        username: String,
        password: String,
    },
    SubmitRegister {
        username: String,
        password: String,
        operator_name: String,
        email: Option<String>,
    },
    SubmitRecover {
        username: String,
        recovery_code: String,
    },
    SubmitSecret {
        password: String,
        upgrade: bool,
        email: Option<String>,
    },
    RotateRecovery,
    RevokeRecovery,
    OpenAccount,
    Quit,
}

pub struct App {
    theme: Theme,
    screen: Screen,
    view: GameView,
    editor: LineEditor,
    notice: Option<String>,
    /// Lines hidden above the bottom of the feed. Zero stays pinned to the latest line.
    feed_scroll: usize,
    completion_note: Option<String>,
}

impl App {
    pub fn new(theme: Theme, screen: Screen) -> Self {
        let world = "Terminus".to_string();
        Self {
            theme,
            screen,
            view: GameView::empty(world),
            editor: LineEditor::default(),
            notice: None,
            feed_scroll: 0,
            completion_note: None,
        }
    }

    pub fn welcome(theme: Theme) -> Self {
        let mut app = Self::new(theme, Screen::Onboard(Onboard::welcome()));
        app.view.world_name = "Terminus".into();
        app.view.location_name = "The Undernet".into();
        app
    }

    pub fn play(theme: Theme, view: GameView) -> Self {
        Self {
            theme,
            screen: Screen::Play,
            view,
            editor: LineEditor::default(),
            notice: None,
            feed_scroll: 0,
            completion_note: None,
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn view(&self) -> &GameView {
        &self.view
    }

    pub fn set_view(&mut self, view: GameView) {
        self.view = view;
    }

    pub fn set_notice(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
    }

    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    pub fn enter_play(&mut self, view: GameView) {
        self.view = view;
        self.screen = Screen::Play;
        self.notice = None;
        self.editor.clear();
    }

    pub fn show_recovery_code(&mut self, code: String, kind: String, view: GameView) {
        self.view = view;
        self.screen = Screen::Onboard(Onboard::code(code, kind));
        self.notice = None;
    }

    pub fn show_account(&mut self, kind: String, message: String) {
        self.screen = Screen::Onboard(Onboard::account(kind, message));
        self.notice = None;
    }

    pub fn handle_key(&mut self, key: Key) -> Effect {
        match &self.screen {
            Screen::Onboard(_) => self.handle_onboard(key),
            Screen::Play => self.handle_play(key),
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < 40 || area.height < 10 {
            frame.render_widget(
                Paragraph::new("Terminal too small. Resize to at least 40x10."),
                area,
            );
            return;
        }
        let [header, body, prompt] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(4), Constraint::Length(3)])
                .areas(area);

        self.render_header(frame, header);
        if body.width >= 70 {
            let [main, side] =
                Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
                    .areas(body);
            let [status, feed] =
                Layout::vertical([Constraint::Length(9), Constraint::Min(3)]).areas(side);
            self.render_location(frame, main);
            self.render_status(frame, status);
            self.render_feed(frame, feed);
        } else {
            let [main, status, feed] = Layout::vertical([
                Constraint::Percentage(40),
                Constraint::Length(6),
                Constraint::Min(3),
            ])
            .areas(body);
            self.render_location(frame, main);
            self.render_status(frame, status);
            self.render_feed(frame, feed);
        }
        self.render_prompt(frame, prompt);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let state = match self.view.connection {
            ConnectionState::Connected => "connected",
            ConnectionState::Offline => "offline",
        };
        let title = format!(" {} ", self.view.world_name);
        let right = format!(" {state} ");
        let line = Line::from(vec![
            Span::styled(title, Style::default().fg(self.theme.title).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(self.view.location_name.clone(), Style::default().fg(self.theme.accent)),
            Span::raw(" ".repeat(area.width.saturating_sub(8) as usize)),
        ]);
        let block = self
            .block("")
            .title_bottom(Line::from(Span::styled(right, Style::default().fg(self.theme.accent))));
        frame.render_widget(Paragraph::new(line).block(block), area);
    }

    fn render_location(&self, frame: &mut Frame, area: Rect) {
        let text = if let Screen::Onboard(onboard) = &self.screen {
            onboard.body()
        } else if let Some(overlay) = &self.view.overlay {
            overlay.clone()
        } else {
            location_body(&self.view)
        };
        let paragraph = Paragraph::new(text)
            .block(self.block("location"))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(self.theme.text));
        frame.render_widget(paragraph, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();
        if self.view.operator_name.is_empty() {
            lines.push(Line::from("no operator"));
        } else {
            lines.push(Line::styled(
                self.view.operator_name.clone(),
                Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
            ));
        }
        for line in &self.view.status {
            lines
                .push(Line::from(Span::styled(line.clone(), Style::default().fg(self.theme.text))));
        }
        if !self.view.inventory.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::styled("carried", Style::default().fg(self.theme.muted)));
            for item in self.view.inventory.iter().take(4) {
                lines.push(Line::from(item.name.clone()));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(self.block("operator")).wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_feed(&self, frame: &mut Frame, area: Rect) {
        let inner_height = area.height.saturating_sub(2) as usize;
        let lines = feed_window(&self.view.feed, inner_height, self.feed_scroll, &self.theme);
        frame.render_widget(
            Paragraph::new(lines).block(self.block("feed")).wrap(Wrap { trim: false }),
            area,
        );
    }

    fn render_prompt(&self, frame: &mut Frame, area: Rect) {
        let (label, value, secret) = self.prompt_parts();
        let shown = if secret { "*".repeat(value.chars().count()) } else { value.to_string() };
        let notice =
            self.notice.clone().or_else(|| self.completion_note.clone()).unwrap_or_default();
        let block = self.block(if notice.is_empty() { "command" } else { "" });
        let prefix = label.width() as u16;
        let paragraph = Paragraph::new(Line::from(vec![
            Span::styled(label, Style::default().fg(self.theme.accent)),
            Span::styled(shown, Style::default().fg(self.theme.text)),
        ]))
        .block(block.title_bottom(Line::from(Span::styled(
            truncate(&notice, area.width.saturating_sub(4) as usize),
            Style::default().fg(self.theme.warn),
        ))));
        frame.render_widget(paragraph, area);

        let cursor_cells = if secret {
            value.chars().count() as u16
        } else if matches!(self.screen, Screen::Play) {
            self.editor.cursor_width() as u16
        } else {
            value.chars().count() as u16
        };
        let x = area.x.saturating_add(1).saturating_add(prefix).saturating_add(cursor_cells);
        let y = area.y.saturating_add(1);
        if x < area.x.saturating_add(area.width.saturating_sub(1))
            && y < area.y.saturating_add(area.height)
        {
            frame.set_cursor_position(Position::new(x, y));
        }
    }

    fn prompt_parts(&self) -> (String, String, bool) {
        match &self.screen {
            Screen::Play => ("> ".into(), self.editor.text().to_string(), false),
            Screen::Onboard(onboard) => {
                let (label, value, secret) = onboard.prompt();
                (label.to_string(), value, secret)
            }
        }
    }

    fn block<'a>(&self, title: &'a str) -> Block<'a> {
        let mut block = Block::bordered()
            .border_type(self.theme.border_type())
            .border_style(Style::default().fg(self.theme.border))
            .style(Style::default().fg(self.theme.text));
        if !title.is_empty() {
            block = block.title(Line::from(Span::styled(
                format!(" {title} "),
                Style::default().fg(self.theme.title),
            )));
        }
        block
    }

    fn handle_play(&mut self, key: Key) -> Effect {
        match key {
            Key::CtrlC | Key::CtrlD if self.editor.text().is_empty() => Effect::Quit,
            Key::CtrlL => Effect::None,
            Key::PageUp => {
                self.feed_scroll = self.feed_scroll.saturating_add(5);
                Effect::None
            }
            Key::PageDown => {
                self.feed_scroll = self.feed_scroll.saturating_sub(5);
                Effect::None
            }
            Key::Enter => {
                let line = self.editor.take();
                self.completion_note = None;
                if line.trim() == "account" {
                    Effect::OpenAccount
                } else if line.is_empty() {
                    Effect::None
                } else {
                    Effect::SubmitCommand(line)
                }
            }
            Key::Tab => {
                self.complete();
                Effect::None
            }
            Key::Up => {
                self.editor.history_up();
                Effect::None
            }
            Key::Down => {
                self.editor.history_down();
                Effect::None
            }
            Key::Left => {
                self.editor.left();
                Effect::None
            }
            Key::Right => {
                self.editor.right();
                Effect::None
            }
            Key::Home => {
                self.editor.home();
                Effect::None
            }
            Key::End => {
                self.editor.end();
                Effect::None
            }
            Key::Backspace => {
                self.editor.backspace();
                Effect::None
            }
            Key::Delete => {
                self.editor.delete();
                Effect::None
            }
            Key::CtrlU | Key::Esc => {
                self.editor.clear();
                Effect::None
            }
            Key::CtrlW => {
                self.editor.delete_word();
                Effect::None
            }
            Key::Char(ch) => {
                self.notice = None;
                self.editor.insert(ch);
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn handle_onboard(&mut self, key: Key) -> Effect {
        let entering = matches!(key, Key::Enter);
        let (effect, on_form) = {
            let Screen::Onboard(onboard) = &mut self.screen else {
                return Effect::None;
            };
            let on_form = matches!(onboard.gate, Gate::Form(_));
            (onboard.handle(key), on_form)
        };
        if matches!(effect, OnboardEffect::EnterWorld) {
            self.screen = Screen::Play;
            self.notice = None;
            return Effect::None;
        }
        if matches!(effect, OnboardEffect::None) && entering && on_form {
            self.notice =
                Some("Check the fields. A password and its confirmation must match.".into());
        }
        match effect {
            OnboardEffect::None | OnboardEffect::EnterWorld => Effect::None,
            OnboardEffect::Quit => Effect::Quit,
            OnboardEffect::SubmitGuest { username, operator_name } => {
                Effect::SubmitGuest { username, operator_name }
            }
            OnboardEffect::SubmitLogin { username, password } => {
                Effect::SubmitLogin { username, password }
            }
            OnboardEffect::SubmitRegister { username, password, operator_name, email } => {
                Effect::SubmitRegister { username, password, operator_name, email }
            }
            OnboardEffect::SubmitRecover { username, recovery_code } => {
                Effect::SubmitRecover { username, recovery_code }
            }
            OnboardEffect::SubmitSecret { password, upgrade, email } => {
                Effect::SubmitSecret { password, upgrade, email }
            }
            OnboardEffect::Rotate => Effect::RotateRecovery,
            OnboardEffect::Revoke => Effect::RevokeRecovery,
        }
    }

    fn complete(&mut self) {
        let line = self.editor.text().to_string();
        let trimmed = line.trim_end();
        let (head, rest_started) = match trimmed.find(char::is_whitespace) {
            Some(idx) => (&trimmed[..idx], true),
            None => (trimmed, false),
        };
        let candidates = if rest_started {
            contextual_candidates(&self.view, head)
                .into_iter()
                .filter(|word| {
                    let typed = trimmed.split_whitespace().nth(1).unwrap_or("");
                    word.to_ascii_lowercase().starts_with(&typed.to_ascii_lowercase())
                        && !typed.is_empty()
                        || typed.is_empty()
                })
                .collect::<Vec<_>>()
        } else {
            command_candidates(&self.view.commands, head)
        };
        match candidates.len() {
            0 => self.completion_note = Some("no completion".into()),
            1 => {
                let word = candidates[0].clone();
                if rest_started {
                    self.editor.set_text(format!("{head} {word}"));
                } else {
                    self.editor.set_text(format!("{word} "));
                }
                self.completion_note = None;
            }
            _ => {
                self.completion_note = Some(candidates.join("  "));
            }
        }
    }
}

fn location_body(view: &GameView) -> String {
    let exits = if view.exits.is_empty() {
        "none".to_string()
    } else {
        view.exits.iter().map(exit_label).collect::<Vec<_>>().join("   ")
    };
    let present = if view.present.is_empty() {
        "nobody else".to_string()
    } else {
        view.present.iter().map(|person| person.name.clone()).collect::<Vec<_>>().join(", ")
    };
    let objects = if view.objects.is_empty() {
        "nothing obvious".to_string()
    } else {
        view.objects.iter().map(|object| object.keyword.clone()).collect::<Vec<_>>().join(", ")
    };
    format!(
        "{}\n\n{}\n\nExits: {exits}\nHere: {present}\nObjects: {objects}",
        view.location_name, view.description
    )
}

fn exit_label(exit: &ExitView) -> String {
    format!("{} ({})", exit.keyword, exit.destination)
}

fn feed_window(
    feed: &[FeedLine],
    height: usize,
    scroll: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if height == 0 {
        return Vec::new();
    }
    let max_scroll = feed.len().saturating_sub(height);
    let scroll = scroll.min(max_scroll);
    let end = feed.len().saturating_sub(scroll);
    let start = end.saturating_sub(height);
    feed[start..end]
        .iter()
        .map(|line| {
            let stamp = format_hms(line.at_ms);
            let (style, body) = match line.channel {
                terminus_core::Channel::System => {
                    (Style::default().fg(theme.system), line.text.clone())
                }
                terminus_core::Channel::Local => (
                    Style::default().fg(theme.local),
                    match &line.speaker {
                        Some(name) => format!("{name}: {}", line.text),
                        None => line.text.clone(),
                    },
                ),
                terminus_core::Channel::Global => (
                    Style::default().fg(theme.global),
                    match &line.speaker {
                        Some(name) => format!("[all] {name}: {}", line.text),
                        None => format!("[all] {}", line.text),
                    },
                ),
            };
            Line::from(vec![
                Span::styled(format!("{stamp} "), Style::default().fg(theme.muted)),
                Span::styled(body, style),
            ])
        })
        .collect()
}

fn command_candidates(commands: &[CommandHint], prefix: &str) -> Vec<String> {
    let prefix = prefix.to_ascii_lowercase();
    let mut words = Vec::new();
    for hint in commands {
        if hint.name.starts_with(&prefix) {
            words.push(hint.name.clone());
        }
        for alias in &hint.aliases {
            if alias.starts_with(&prefix) && alias != "'" {
                words.push(alias.clone());
            }
        }
    }
    words.sort();
    words.dedup();
    words
}

fn contextual_candidates(view: &GameView, command: &str) -> Vec<String> {
    match command.to_ascii_lowercase().as_str() {
        "go" | "move" => view.exits.iter().map(|exit| exit.keyword.clone()).collect(),
        "inspect" | "examine" | "ex" => {
            let mut words =
                view.objects.iter().map(|object| object.keyword.clone()).collect::<Vec<_>>();
            words.extend(view.present.iter().map(|person| person.name.clone()));
            words.extend(view.inventory.iter().map(|item| item.keyword.clone()));
            words
        }
        "help" | "?" => view.commands.iter().map(|hint| hint.name.clone()).collect(),
        "who" => vec!["here".into(), "all".into()],
        _ => Vec::new(),
    }
}

fn format_hms(ms: u64) -> String {
    let secs = ms / 1000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        if out.width() + unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) > width {
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use terminus_core::{ConnectionState, ExitView, PresenceView};

    #[test]
    fn renders_location_status_and_prompt() {
        let theme = Theme::neutral();
        let mut view = GameView::empty("Undernet");
        view.connection = ConnectionState::Connected;
        view.operator_name = "ada".into();
        view.location_name = "The Coffee Shop".into();
        view.description = "Steam and terminals.".into();
        view.exits = vec![ExitView {
            keyword: "alley".into(),
            label: "Neon Alley".into(),
            destination: "Neon Alley".into(),
        }];
        view.present = vec![PresenceView { name: "bob".into() }];
        view.status = vec!["place: The Coffee Shop".into()];
        let mut app = App::play(theme, view);
        app.handle_key(Key::Char('l'));
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("The Coffee Shop"), "{text}");
        assert!(text.contains("ada"), "{text}");
        assert!(text.contains("alley"), "{text}");
        assert!(text.contains("bob"), "{text}");
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let mut lines = Vec::new();
        for y in 0..buffer.area().height {
            let mut line = String::new();
            for x in 0..buffer.area().width {
                line.push_str(buffer[(x, y)].symbol());
            }
            lines.push(line);
        }
        lines.join("\n")
    }
}
