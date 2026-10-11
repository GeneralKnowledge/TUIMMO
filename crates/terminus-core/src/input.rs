//! Terminal input as the engine sees it, independent of crossterm or SSH.

/// A key or a terminal resize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    Key(Key),
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Raw bytes from a transport that has not been decoded yet.
    Bytes(Vec<u8>),
}

/// Keys the command prompt and panel navigation understand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Esc,
    CtrlC,
    CtrlD,
    CtrlL,
    CtrlR,
    CtrlU,
    CtrlW,
}
