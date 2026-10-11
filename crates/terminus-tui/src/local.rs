//! Play a [`Game`] directly in the local terminal. No network and no SSH.

use std::io::{self, Write};

use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use terminus_core::{Game, InputEvent, Key, SessionIdentity};

use tokio::sync::mpsc;

use crate::app::{App, Effect};
use crate::remote::Input;
use crate::theme::Theme;
use crate::TuiError;

pub async fn run_local<G>(
    mut game: G,
    identity: SessionIdentity,
    theme: Theme,
) -> Result<(), TuiError>
where
    G: Game,
{
    let session_id = identity.session_id;
    game.connect(identity)?;
    let view = game.view(session_id)?;
    let mut app = App::play(theme, view);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let _restore = Restore;
    terminal.hide_cursor()?;

    let mut events = EventStream::new();
    loop {
        terminal.draw(|frame| app.render(frame))?;
        let Some(event) = events.next().await else {
            break;
        };
        let event = event?;
        match event {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                let effect = app.handle_key(map_key(key.code, key.modifiers));
                match effect {
                    Effect::SubmitCommand(line) => match game.command(session_id, &line) {
                        Ok(outcome) => {
                            app.set_view(game.view(session_id)?);
                            if outcome.quit {
                                break;
                            }
                        }
                        Err(err) => app.set_notice(err.to_string()),
                    },
                    Effect::Quit => break,
                    Effect::None | Effect::SubmitAuth { .. } => {}
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn map_key(code: KeyCode, modifiers: KeyModifiers) -> Key {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return match code {
            KeyCode::Char('c') => Key::CtrlC,
            KeyCode::Char('d') => Key::CtrlD,
            KeyCode::Char('l') => Key::CtrlL,
            KeyCode::Char('r') => Key::CtrlR,
            KeyCode::Char('u') => Key::CtrlU,
            KeyCode::Char('w') => Key::CtrlW,
            _ => Key::Esc,
        };
    }
    match code {
        KeyCode::Char(ch) => Key::Char(ch),
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Esc => Key::Esc,
        _ => Key::Esc,
    }
}

/// Read the local terminal and forward keys. Used by `coffee-server play`.
pub fn spawn_crossterm_input() -> mpsc::UnboundedReceiver<Input> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        let mut events = EventStream::new();
        while let Some(event) = events.next().await {
            let Ok(event) = event else {
                break;
            };
            let input = match event {
                Event::Key(key) if key.kind != KeyEventKind::Release => {
                    Input::Event(InputEvent::Key(map_key(key.code, key.modifiers)))
                }
                Event::Resize(cols, rows) => Input::Event(InputEvent::Resize { cols, rows }),
                _ => continue,
            };
            if tx.send(input).is_err() {
                break;
            }
        }
    });
    rx
}

struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
        let _ = stdout.flush();
    }
}
