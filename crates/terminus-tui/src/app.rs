//! Screen state: header, location, status, feed, and the command prompt.

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
use ratatui::Frame;
use terminus_core::{CommandHint, ConnectionState, ExitView, FeedLine, GameView, Key};
use unicode_width::UnicodeWidthStr;

use crate::editor::LineEditor;
use crate::theme::Theme;

#[derive(Clone, Debug)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
    pub operator_name: String,
    pub field: LoginField,
    pub register: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginField {
    Username,
    Password,
    OperatorName,
}

impl LoginForm {
    pub fn new() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            operator_name: String::new(),
            field: LoginField::Username,
            register: false,
        }
    }
}

impl Default for LoginForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
pub enum Screen {
    Login(LoginForm),
    Play,
}

#[derive(Clone, Debug)]
pub enum Effect {
    None,
    SubmitCommand(String),
    SubmitAuth { username: String, password: String, operator_name: String, register: bool },
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

    pub fn login() -> Self {
        Self::new(Theme::neutral(), Screen::Login(LoginForm::new()))
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

    pub fn handle_key(&mut self, key: Key) -> Effect {
        match &self.screen {
            Screen::Login(_) => self.handle_login(key),
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
        if let Screen::Login(form) = &self.screen {
            self.paint_block(frame, area, "location", &login_help(form), self.theme.text, None);
            return;
        }
        let finding = self.view.finding.as_deref().filter(|text| !text.is_empty());
        let notes = self.view.overlay.as_deref().filter(|text| !text.is_empty());
        if finding.is_none() && notes.is_none() {
            self.paint_block(
                frame,
                area,
                "location",
                &location_body(&self.view),
                self.theme.text,
                None,
            );
            return;
        }
        // A short pane cannot spare a second border. Keep the place, then the note.
        if area.height < 12 {
            self.paint_block(
                frame,
                area,
                "location",
                &compact_place(&self.view),
                self.theme.text,
                None,
            );
            return;
        }
        let mut constraints = vec![Constraint::Min(5)];
        if finding.is_some() {
            constraints.push(Constraint::Length(noticed_height(area.height)));
        }
        if notes.is_some() {
            constraints.push(Constraint::Min(4));
        }
        let rects = Layout::vertical(constraints).split(area);
        let mut index = 0;
        self.paint_block(
            frame,
            rects[index],
            "location",
            &location_body(&self.view),
            self.theme.text,
            None,
        );
        index += 1;
        if let Some(text) = finding {
            self.paint_block(frame, rects[index], "noticed", text, self.theme.accent, None);
            index += 1;
        }
        if let Some(text) = notes {
            self.paint_block(
                frame,
                rects[index],
                "notes",
                text,
                self.theme.text,
                Some("look returns"),
            );
        }
    }

    fn paint_block(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        text: &str,
        color: ratatui::style::Color,
        footer: Option<&str>,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut block = self.block(title);
        if let Some(footer) = footer {
            block = block.title_bottom(Line::from(Span::styled(
                format!(" {footer} "),
                Style::default().fg(self.theme.muted),
            )));
        }
        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(color));
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
            Screen::Login(form) => {
                let (label, value, secret) = match form.field {
                    LoginField::Username => ("user> ", form.username.clone(), false),
                    LoginField::Password => ("pass> ", form.password.clone(), true),
                    LoginField::OperatorName => ("name> ", form.operator_name.clone(), false),
                };
                (label.into(), value, secret)
            }
        }
    }

    fn block<'a>(&self, title: &'a str) -> Block<'a> {
        let mut block = if self.theme.ascii {
            Block::bordered().border_set(ASCII_BORDER)
        } else {
            Block::bordered().border_type(BorderType::Rounded)
        };
        block = block
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
                if line.is_empty() {
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

    fn handle_login(&mut self, key: Key) -> Effect {
        let Screen::Login(form) = &mut self.screen else {
            return Effect::None;
        };
        match key {
            Key::CtrlC => return Effect::Quit,
            Key::CtrlR => {
                form.register = !form.register;
                if !form.register && form.field == LoginField::OperatorName {
                    form.field = LoginField::Username;
                }
                return Effect::None;
            }
            Key::Tab => {
                form.field = next_field(form.field, form.register, true);
                return Effect::None;
            }
            Key::BackTab => {
                form.field = next_field(form.field, form.register, false);
                return Effect::None;
            }
            Key::Enter => {
                let username = form.username.trim().to_string();
                let password = form.password.clone();
                let operator_name = if form.operator_name.trim().is_empty() {
                    username.clone()
                } else {
                    form.operator_name.trim().to_string()
                };
                let register = form.register;
                if username.is_empty() || password.is_empty() {
                    self.notice = Some("Username and password are required.".into());
                    return Effect::None;
                }
                return Effect::SubmitAuth { username, password, operator_name, register };
            }
            _ => {}
        }
        let Screen::Login(form) = &mut self.screen else {
            return Effect::None;
        };
        let field = match form.field {
            LoginField::Username => &mut form.username,
            LoginField::Password => &mut form.password,
            LoginField::OperatorName => &mut form.operator_name,
        };
        match key {
            Key::Backspace => {
                field.pop();
            }
            Key::CtrlU => field.clear(),
            Key::Char(ch) if !ch.is_control() => field.push(ch),
            _ => {}
        }
        Effect::None
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

fn next_field(field: LoginField, register: bool, forward: bool) -> LoginField {
    let fields = if register {
        vec![LoginField::Username, LoginField::Password, LoginField::OperatorName]
    } else {
        vec![LoginField::Username, LoginField::Password]
    };
    let index = fields.iter().position(|item| *item == field).unwrap_or(0);
    let len = fields.len();
    let next = if forward { (index + 1) % len } else { (index + len - 1) % len };
    fields[next]
}

fn login_help(form: &LoginForm) -> String {
    let mode = if form.register { "register" } else { "log in" };
    format!(
        "Coffee.shop\n\n\
{mode} to an operator.\n\
Tab switches fields. Ctrl-R toggles registration.\n\
Enter submits. Ctrl-C quits.\n\n\
Usernames are lowercase, 3-16 characters.\n\
Passwords are 8-128 characters and are not stored in the clear.\n\n\
New accounts begin in the Coffee Shop."
    )
}

const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

fn noticed_height(area_height: u16) -> u16 {
    (area_height / 3).clamp(4, 8)
}

fn compact_place(view: &GameView) -> String {
    let mut text = location_body(view);
    if let Some(found) = view.finding.as_deref().filter(|text| !text.is_empty()) {
        text.push_str("\n\nNoticed:\n");
        text.push_str(found);
    }
    if let Some(notes) = view.overlay.as_deref().filter(|text| !text.is_empty()) {
        text.push_str("\n\n");
        text.push_str(notes);
        text.push_str("\nlook returns to the place");
    }
    text
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
                terminus_core::Channel::Region => (
                    Style::default().fg(theme.region),
                    match &line.speaker {
                        Some(name) => format!("[region] {name}: {}", line.text),
                        None => format!("[region] {}", line.text),
                    },
                ),
                terminus_core::Channel::Private => (
                    Style::default().fg(theme.private),
                    match &line.speaker {
                        Some(name) => format!("[whisper] {name}: {}", line.text),
                        None => format!("[whisper] {}", line.text),
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
        "journal" => vec!["recent".into(), "read".into(), "record".into()],
        "waypoint" => {
            vec!["add".into(), "list".into(), "show".into(), "note".into(), "remove".into()]
        }
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

    #[test]
    fn inspection_stays_visible_beside_the_place() {
        let mut view = sample_view();
        view.finding = Some(
            "The counter is scarred laminate. Someone has carved a tiny cup into the corner."
                .into(),
        );
        view.feed.push(terminus_core::FeedLine {
            at_ms: 1_700_000_000_000,
            channel: terminus_core::Channel::Local,
            speaker: Some("bob".into()),
            text: "anyone for coffee".into(),
        });
        let app = App::play(Theme::neutral(), view);
        let text = draw(&app, 100, 30);
        assert!(text.contains("Steam and terminals"), "{text}");
        assert!(text.contains("noticed"), "{text}");
        assert!(text.contains("scarred laminate"), "{text}");
        assert!(text.contains("alley"), "{text}");
        assert!(text.contains("anyone for coffee"), "{text}");
    }

    #[test]
    fn journal_keeps_the_place_and_says_how_to_return() {
        let mut view = sample_view();
        view.finding = Some("relay insulation, and it does not say which dish it left.".into());
        view.overlay = Some(
            "Field journal (1)\nsurvey-mark  Repeated mark\n`journal read <id>`\n`look` returns to the place."
                .into(),
        );
        let app = App::play(Theme::neutral(), view);
        let text = draw(&app, 100, 32);
        assert!(text.contains("The Coffee Shop"), "{text}");
        assert!(text.contains("Steam and terminals"), "{text}");
        assert!(text.contains("alley"), "{text}");
        assert!(text.contains("survey-mark"), "{text}");
        assert!(text.contains("look returns"), "{text}");
        assert!(text.contains("relay insulation"), "{text}");
    }

    #[test]
    fn waypoint_overlay_keeps_the_place_on_screen() {
        let mut view = sample_view();
        view.overlay = Some(
            "Waypoint: marked-stone\n\
             Destination: open grass\n\
             Where: birth (0, 0), tile 7,8\n\
             Note: fiber under the stone\n\
             Direction: From the Roof Array, go outskirts. From the city gate: west.\n\
             `look` returns to the place."
                .into(),
        );
        let app = App::play(Theme::neutral(), view);
        let text = draw(&app, 100, 32);
        assert!(text.contains("The Coffee Shop"), "{text}");
        assert!(text.contains("Steam and terminals"), "{text}");
        assert!(text.contains("Waypoint: marked-stone"), "{text}");
        assert!(text.contains("Destination: open grass"), "{text}");
        assert!(text.contains("fiber under the stone"), "{text}");
        assert!(text.contains("look returns"), "{text}");
    }

    #[test]
    fn long_notes_and_narrow_ascii_layouts_hold() {
        let mut view = sample_view();
        view.finding = Some("mark ".repeat(80));
        view.overlay = Some(format!("Field journal\n{}", "note ".repeat(80)));
        let wide = App::play(Theme::neutral(), view.clone());
        let wide_text = draw(&wide, 100, 30);
        assert!(wide_text.contains("The Coffee Shop"), "{wide_text}");
        assert!(wide_text.contains("mark"), "{wide_text}");
        assert!(wide_text.contains("Field journal"), "{wide_text}");
        assert!(wide_text.contains('╭'), "{wide_text}");

        let mut ascii = Theme::neutral();
        ascii.ascii = true;
        let narrow = App::play(ascii, view);
        let narrow_text = draw(&narrow, 60, 20);
        assert!(narrow_text.contains('+'), "{narrow_text}");
        assert!(!narrow_text.contains('╭'), "{narrow_text}");
        assert!(narrow_text.contains("The Coffee Shop"), "{narrow_text}");

        let tiny = App::play(Theme::neutral(), sample_view());
        let tiny_text = draw(&tiny, 40, 12);
        assert!(
            tiny_text.contains("Coffee")
                || tiny_text.contains("too small")
                || tiny_text.contains("40"),
            "{tiny_text}"
        );
    }

    #[test]
    fn history_completion_scroll_and_masked_password_still_work() {
        let mut view = sample_view();
        view.commands = vec![terminus_core::CommandHint {
            name: "help".into(),
            aliases: vec!["?".into()],
            usage: "help".into(),
            summary: "List commands".into(),
        }];
        view.feed.push(terminus_core::FeedLine {
            at_ms: 3_600_000,
            channel: terminus_core::Channel::System,
            speaker: None,
            text: "Ada arrives from the alley.".into(),
        });
        let mut app = App::play(Theme::neutral(), view);
        for ch in "look".chars() {
            app.handle_key(Key::Char(ch));
        }
        assert!(
            matches!(app.handle_key(Key::Enter), Effect::SubmitCommand(line) if line == "look")
        );
        app.handle_key(Key::Up);
        assert_eq!(app.editor.text(), "look");
        app.handle_key(Key::CtrlU);
        for ch in "he".chars() {
            app.handle_key(Key::Char(ch));
        }
        app.handle_key(Key::Tab);
        assert_eq!(app.editor.text(), "help ");
        app.handle_key(Key::PageUp);
        app.handle_key(Key::PageDown);
        let played = draw(&app, 80, 24);
        assert!(played.contains("help"), "{played}");

        let mut login = App::login();
        login.handle_key(Key::Tab);
        for ch in "s3cret!".chars() {
            login.handle_key(Key::Char(ch));
        }
        let masked = draw(&login, 80, 24);
        assert!(!masked.contains("s3cret"), "{masked}");
        assert!(masked.contains("*******"), "{masked}");
        login.set_notice("You are sending commands too quickly.");
        let noticed = draw(&login, 80, 24);
        assert!(noticed.contains("too quickly"), "{noticed}");
        assert!(noticed.contains("Coffee"), "{noticed}");
    }

    fn sample_view() -> GameView {
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
        view
    }

    fn draw(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        buffer_text(terminal.backend().buffer())
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
