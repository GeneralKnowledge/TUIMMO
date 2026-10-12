//! Screen state: header, location, status, feed, and the command prompt.

use ratatui::layout::Position;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;
use terminus_core::{CommandHint, GameView, Key};
use unicode_width::UnicodeWidthStr;

use crate::editor::LineEditor;
use crate::present::{
    feed_lines, fit, header_line, heading, inner_size, input_window, location_lines, panel,
    screen_plan, side_lines, PROMPT_HINT,
};
use crate::theme::{Role, Theme};

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
        frame.render_widget(Paragraph::new("").style(self.theme.ink()), area);
        let Some(plan) = screen_plan(area) else {
            frame.render_widget(Paragraph::new("Too small").style(self.theme.ink()), area);
            return;
        };
        self.render_header(frame, plan.header, plan.bordered);
        self.render_location(frame, plan.location, plan.bordered);
        if let Some(side) = plan.side {
            self.render_side(frame, side, plan.bordered);
        }
        self.render_feed(frame, plan.feed, plan.bordered);
        self.render_prompt(frame, plan.prompt, plan.bordered, plan.show_hint);
    }

    fn render_header(&self, frame: &mut Frame, area: ratatui::layout::Rect, bordered: bool) {
        let boxed = panel(&self.theme, "", area, bordered);
        let inner = inner_size(area, boxed.is_some()).0;
        let line = header_line(&self.view, &self.theme, inner);
        let paragraph = Paragraph::new(line).style(self.theme.ink());
        frame.render_widget(
            match boxed {
                Some(block) => paragraph.block(block),
                None => paragraph,
            },
            area,
        );
    }

    fn render_location(&self, frame: &mut Frame, area: ratatui::layout::Rect, bordered: bool) {
        let title = if matches!(self.screen, Screen::Login(_)) {
            "link"
        } else {
            self.view.location_name.as_str()
        };
        let boxed = panel(&self.theme, title, area, bordered);
        let mut lines = Vec::new();
        if boxed.is_none() {
            lines.push(heading(&self.theme, title));
        }
        if let Screen::Login(form) = &self.screen {
            for line in login_help(form).lines() {
                lines.push(Line::styled(line.to_string(), self.theme.ink()));
            }
        } else {
            lines.extend(location_lines(&self.view, &self.theme));
        }
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).style(self.theme.ink());
        frame.render_widget(
            match boxed {
                Some(block) => paragraph.block(block),
                None => paragraph,
            },
            area,
        );
    }

    fn render_side(&self, frame: &mut Frame, area: ratatui::layout::Rect, bordered: bool) {
        let boxed = panel(&self.theme, "operator", area, bordered);
        let mut lines = Vec::new();
        if boxed.is_none() {
            lines.push(heading(&self.theme, "operator"));
        }
        lines.extend(side_lines(&self.view, &self.theme));
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).style(self.theme.ink());
        frame.render_widget(
            match boxed {
                Some(block) => paragraph.block(block),
                None => paragraph,
            },
            area,
        );
    }

    fn render_feed(&self, frame: &mut Frame, area: ratatui::layout::Rect, bordered: bool) {
        let boxed = panel(&self.theme, "activity", area, bordered);
        let mut lines = Vec::new();
        if boxed.is_none() {
            lines.push(heading(&self.theme, "activity"));
        }
        let inner_height =
            inner_size(area, boxed.is_some()).1.saturating_sub(if boxed.is_none() { 1 } else { 0 });
        lines.extend(feed_lines(&self.view.feed, inner_height, self.feed_scroll, &self.theme));
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false }).style(self.theme.ink());
        frame.render_widget(
            match boxed {
                Some(block) => paragraph.block(block),
                None => paragraph,
            },
            area,
        );
    }

    fn render_prompt(
        &self,
        frame: &mut Frame,
        area: ratatui::layout::Rect,
        bordered: bool,
        show_hint: bool,
    ) {
        let (label, value, secret) = self.prompt_parts();
        let raw = if secret { "*".repeat(value.chars().count()) } else { value };
        let boxed = panel(&self.theme, "command", area, bordered);
        let inner = inner_size(area, boxed.is_some()).0;
        let prefix_width = label.width();
        let room = inner.saturating_sub(prefix_width);
        let cursor_chars = if secret || !matches!(self.screen, Screen::Play) {
            raw.chars().count()
        } else {
            self.editor_cursor_chars()
        };
        let (shown, cursor_cells) = input_window(&raw, cursor_chars, room);
        let (foot, foot_role) = self.prompt_foot(inner, show_hint);
        let has_box = boxed.is_some();
        let mut paragraph = Paragraph::new(Line::from(vec![
            Span::styled(label, self.theme.style(Role::Focused)),
            Span::styled(shown, self.theme.ink()),
        ]))
        .style(self.theme.ink());
        if let Some(block) = boxed {
            let block = if foot.is_empty() {
                block
            } else {
                block.title_bottom(Line::from(Span::styled(
                    format!(" {foot} "),
                    self.theme.style(foot_role),
                )))
            };
            paragraph = paragraph.block(block);
        }
        frame.render_widget(paragraph, area);
        if !has_box && show_hint && !foot.is_empty() && area.height > 1 {
            let hint_area = ratatui::layout::Rect {
                x: area.x,
                y: area.y.saturating_add(1),
                width: area.width,
                height: 1,
            };
            frame.render_widget(
                Paragraph::new(Line::styled(foot, self.theme.style(foot_role))),
                hint_area,
            );
        }

        let border_pad: u16 = if has_box { 1 } else { 0 };
        let x = area
            .x
            .saturating_add(border_pad)
            .saturating_add(u16::try_from(prefix_width).unwrap_or(0))
            .saturating_add(u16::try_from(cursor_cells).unwrap_or(0));
        let y = area.y.saturating_add(border_pad);
        if x < area.x.saturating_add(area.width.saturating_sub(1))
            && y < area.y.saturating_add(area.height)
        {
            frame.set_cursor_position(Position::new(x, y));
        }
    }

    fn editor_cursor_chars(&self) -> usize {
        let width = self.editor.cursor_width();
        let text = self.editor.text();
        let mut seen = 0usize;
        for (index, ch) in text.chars().enumerate() {
            if seen >= width {
                return index;
            }
            seen = seen.saturating_add(unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0));
        }
        text.chars().count()
    }

    fn prompt_foot(&self, width: usize, show_hint: bool) -> (String, Role) {
        if let Some(notice) = &self.notice {
            return (fit(notice, width), Role::Error);
        }
        if let Some(note) = &self.completion_note {
            return (fit(note, width), Role::Accent);
        }
        if show_hint {
            (fit(PROMPT_HINT, width), Role::Muted)
        } else {
            (String::new(), Role::Muted)
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
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;
    use terminus_core::{Channel, ConnectionState, ExitView, FeedLine, ItemView, PresenceView};

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
        view.inventory = vec![ItemView {
            keyword: "chip".into(),
            name: "loyalty chip".into(),
            detail: "warm".into(),
        }];
        view.commands = vec![CommandHint {
            name: "help".into(),
            aliases: vec!["?".into()],
            usage: "help".into(),
            summary: "List commands".into(),
        }];
        view.feed = vec![
            FeedLine {
                at_ms: 1_000,
                channel: Channel::Local,
                speaker: Some("bob".into()),
                text: "the counter is free".into(),
            },
            FeedLine {
                at_ms: 2_000,
                channel: Channel::Local,
                speaker: None,
                text: "nods at the door".into(),
            },
            FeedLine {
                at_ms: 3_000,
                channel: Channel::Global,
                speaker: Some("ada".into()),
                text: "heading up".into(),
            },
            FeedLine {
                at_ms: 4_000,
                channel: Channel::Region,
                speaker: Some("cam".into()),
                text: "north of the gate".into(),
            },
            FeedLine {
                at_ms: 5_000,
                channel: Channel::Private,
                speaker: Some("bob".into()),
                text: "meet at the roof".into(),
            },
            FeedLine {
                at_ms: 6_000,
                channel: Channel::System,
                speaker: None,
                text: "recorded in the journal".into(),
            },
        ];
        view
    }

    fn draw(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let mut lines = Vec::new();
        for y in 0..buffer.area().height {
            let mut line = String::new();
            for x in 0..buffer.area().width {
                line.push_str(buffer[(x, y)].symbol());
            }
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }

    fn fg_of(buffer: &ratatui::buffer::Buffer, needle: &str) -> Option<Color> {
        let width = buffer.area().width;
        let height = buffer.area().height;
        for y in 0..height {
            let mut row = String::new();
            for x in 0..width {
                row.push_str(buffer[(x, y)].symbol());
            }
            if let Some(index) = row.find(needle) {
                return Some(buffer[(u16::try_from(index).unwrap_or(0), y)].fg);
            }
        }
        None
    }

    #[test]
    fn renders_location_activity_and_prompt() {
        let mut app = App::play(Theme::undernet(false), sample_view());
        app.handle_key(Key::Char('l'));
        let buffer = draw(&app, 110, 32);
        let text = buffer_text(&buffer);
        assert!(text.contains("The Coffee Shop"), "{text}");
        assert!(text.contains("Undernet"), "{text}");
        assert!(text.contains("connected"), "{text}");
        assert!(text.contains("ada"), "{text}");
        assert!(text.contains("alley"), "{text}");
        assert!(text.contains("bob"), "{text}");
        assert!(text.contains("activity"), "{text}");
        assert!(text.contains("operator"), "{text}");
        assert!(text.contains("loyalty chip"), "{text}");
        assert!(text.contains("[all]"), "{text}");
        assert!(text.contains("[region]"), "{text}");
        assert!(text.contains("[whisper]"), "{text}");
        assert!(text.contains("[note]"), "{text}");
        assert!(text.contains("command") || text.contains('>'), "{text}");
        assert!(text.contains("history"), "{text}");
    }

    #[test]
    fn ascii_theme_uses_plain_borders() {
        let app = App::play(Theme::undernet(true), sample_view());
        let text = buffer_text(&draw(&app, 80, 24));
        assert!(text.contains('+'), "{text}");
        assert!(!text.contains('╭'), "{text}");
        assert!(text.contains("exits"), "{text}");
        assert!(text.contains("alley"), "{text}");
        let rounded = App::play(Theme::undernet(false), sample_view());
        let fancy = buffer_text(&draw(&rounded, 80, 24));
        assert!(fancy.contains('╭'), "{fancy}");
    }

    #[test]
    fn channel_colours_do_not_depend_on_message_text() {
        let app = App::play(Theme::undernet(true), sample_view());
        let buffer = draw(&app, 110, 36);
        assert_eq!(fg_of(&buffer, "[all]"), Some(Color::Cyan));
        assert_eq!(fg_of(&buffer, "[note]"), Some(Color::Yellow));
        assert_eq!(fg_of(&buffer, "[whisper]"), Some(Color::Magenta));
        assert_eq!(fg_of(&buffer, "[region]"), Some(Color::Green));
        let text = buffer_text(&buffer);
        assert!(text.contains("meet at the roof"));
        assert!(!text.contains("[whisper] meet at the roof\n[whisper]"));
    }

    #[test]
    fn narrow_short_and_resize_do_not_panic() {
        let mut view = sample_view();
        view.description = "d".repeat(2_000);
        view.feed.push(FeedLine {
            at_ms: 9_000,
            channel: Channel::System,
            speaker: None,
            text: "m".repeat(2_000),
        });
        view.operator_name = "n".repeat(80);
        let app = App::play(Theme::neutral(), view);
        for (width, height) in [(110, 32), (80, 24), (60, 20), (40, 8), (20, 8), (10, 4)] {
            let text = buffer_text(&draw(&app, width, height));
            if width < 20 || height < 8 {
                assert!(
                    text.contains("Too small") || text.contains("small"),
                    "{width}x{height}: {text}"
                );
            } else {
                assert!(!text.contains("too small"), "{width}x{height}: {text}");
            }
        }
        let wide = buffer_text(&draw(&app, 110, 32));
        let narrow = buffer_text(&draw(&app, 60, 24));
        assert!(wide.contains("operator"), "{wide}");
        assert!(!narrow.contains("operator"), "{narrow}");
    }

    #[test]
    fn history_completion_scroll_and_masked_password() {
        let mut app = App::play(Theme::neutral(), sample_view());
        for ch in "look".chars() {
            app.handle_key(Key::Char(ch));
        }
        assert!(
            matches!(app.handle_key(Key::Enter), Effect::SubmitCommand(line) if line == "look")
        );
        app.handle_key(Key::Up);
        assert_eq!(app.editor.text(), "look");
        app.handle_key(Key::Down);
        for ch in "he".chars() {
            app.handle_key(Key::Char(ch));
        }
        app.handle_key(Key::Tab);
        assert_eq!(app.editor.text(), "help ");
        app.handle_key(Key::PageUp);
        let scrolled = buffer_text(&draw(&app, 80, 16));
        assert!(
            scrolled.contains("activity")
                || scrolled.contains("quiet")
                || scrolled.contains("[note]")
        );

        let mut login = App::login();
        login.set_theme(Theme::undernet(true));
        for ch in "ada".chars() {
            login.handle_key(Key::Char(ch));
        }
        login.handle_key(Key::Tab);
        for ch in "s3cret!".chars() {
            login.handle_key(Key::Char(ch));
        }
        let text = buffer_text(&draw(&login, 80, 24));
        assert!(!text.contains("s3cret"), "{text}");
        assert!(text.contains("*******"), "{text}");
        assert!(text.contains("pass>"), "{text}");
    }
}
