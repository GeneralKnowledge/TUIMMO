//! Decode raw terminal bytes into [`Key`] values.
//!
//! SSH sessions deliver bytes, not crossterm events. Sequences that are still
//! incomplete stay buffered.

use terminus_core::Key;

#[derive(Debug, Default)]
pub struct InputDecoder {
    pending: Vec<u8>,
}

impl InputDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<Key> {
        self.pending.extend_from_slice(bytes);
        let mut keys = Vec::new();
        loop {
            match self.pull() {
                Pull::Key(key) => keys.push(key),
                Pull::Skip => continue,
                Pull::NeedMore => break,
            }
        }
        keys
    }

    fn pull(&mut self) -> Pull {
        if self.pending.is_empty() {
            return Pull::NeedMore;
        }
        if self.pending[0] == 0x1b {
            return self.pull_escape();
        }
        let byte = self.pending[0];
        match byte {
            b'\r' => {
                self.pending.remove(0);
                if self.pending.first() == Some(&b'\n') {
                    self.pending.remove(0);
                }
                Pull::Key(Key::Enter)
            }
            b'\n' => {
                self.pending.remove(0);
                Pull::Key(Key::Enter)
            }
            0x7f | 0x08 => {
                self.pending.remove(0);
                Pull::Key(Key::Backspace)
            }
            0x03 => {
                self.pending.remove(0);
                Pull::Key(Key::CtrlC)
            }
            0x04 => {
                self.pending.remove(0);
                Pull::Key(Key::CtrlD)
            }
            0x15 => {
                self.pending.remove(0);
                Pull::Key(Key::CtrlU)
            }
            0x0c => {
                self.pending.remove(0);
                Pull::Key(Key::CtrlL)
            }
            0x12 => {
                self.pending.remove(0);
                Pull::Key(Key::CtrlR)
            }
            0x17 => {
                self.pending.remove(0);
                Pull::Key(Key::CtrlW)
            }
            b'\t' => {
                self.pending.remove(0);
                Pull::Key(Key::Tab)
            }
            byte if byte < 0x20 => {
                self.pending.remove(0);
                Pull::Skip
            }
            _ => self.pull_char(),
        }
    }

    fn pull_escape(&mut self) -> Pull {
        let rest = &self.pending[1..];
        if rest.is_empty() {
            return Pull::NeedMore;
        }
        if rest[0] == b'[' || rest[0] == b'O' {
            if let Some((key, len)) = match_csi(&self.pending) {
                self.pending.drain(..len);
                return Pull::Key(key);
            }
            if self.pending.len() < 8 && csi_prefix(&self.pending) {
                return Pull::NeedMore;
            }
        }
        self.pending.remove(0);
        Pull::Key(Key::Esc)
    }

    fn pull_char(&mut self) -> Pull {
        match std::str::from_utf8(&self.pending) {
            Ok(text) => {
                let Some(ch) = text.chars().next() else {
                    return Pull::NeedMore;
                };
                let len = ch.len_utf8();
                self.pending.drain(..len);
                if ch.is_control() {
                    Pull::Skip
                } else {
                    Pull::Key(Key::Char(ch))
                }
            }
            Err(err) => {
                let valid = err.valid_up_to();
                if valid > 0 {
                    let Ok(text) = std::str::from_utf8(&self.pending[..valid]) else {
                        self.pending.drain(..valid);
                        return Pull::Skip;
                    };
                    let Some(ch) = text.chars().next() else {
                        return Pull::NeedMore;
                    };
                    let len = ch.len_utf8();
                    self.pending.drain(..len);
                    Pull::Key(Key::Char(ch))
                } else if err.error_len().is_none() && self.pending.len() < 4 {
                    Pull::NeedMore
                } else {
                    self.pending.remove(0);
                    Pull::Skip
                }
            }
        }
    }
}

enum Pull {
    Key(Key),
    Skip,
    NeedMore,
}

fn csi_prefix(buf: &[u8]) -> bool {
    matches!(
        buf,
        [0x1b]
            | [0x1b, b'[']
            | [0x1b, b'O']
            | [0x1b, b'[', b'0'..=b'9']
            | [0x1b, b'[', b'0'..=b'9', b'0'..=b'9']
    )
}

fn match_csi(buf: &[u8]) -> Option<(Key, usize)> {
    let key = match buf {
        [0x1b, b'[', b'A', ..] | [0x1b, b'O', b'A', ..] => Key::Up,
        [0x1b, b'[', b'B', ..] | [0x1b, b'O', b'B', ..] => Key::Down,
        [0x1b, b'[', b'C', ..] | [0x1b, b'O', b'C', ..] => Key::Right,
        [0x1b, b'[', b'D', ..] | [0x1b, b'O', b'D', ..] => Key::Left,
        [0x1b, b'[', b'H', ..] | [0x1b, b'O', b'H', ..] => Key::Home,
        [0x1b, b'[', b'F', ..] | [0x1b, b'O', b'F', ..] => Key::End,
        [0x1b, b'[', b'Z', ..] => Key::BackTab,
        [0x1b, b'[', b'3', b'~', ..] => return Some((Key::Delete, 4)),
        [0x1b, b'[', b'5', b'~', ..] => return Some((Key::PageUp, 4)),
        [0x1b, b'[', b'6', b'~', ..] => return Some((Key::PageDown, 4)),
        [0x1b, b'[', b'1', b'~', ..] => return Some((Key::Home, 4)),
        [0x1b, b'[', b'4', b'~', ..] => return Some((Key::End, 4)),
        _ => return None,
    };
    Some((key, 3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_text_arrows_and_split_escape() {
        let mut dec = InputDecoder::new();
        assert_eq!(dec.push(b"ab\r"), vec![Key::Char('a'), Key::Char('b'), Key::Enter]);
        assert!(dec.push(&[0x1b, b'[']).is_empty());
        assert_eq!(dec.push(b"A"), vec![Key::Up]);
        assert_eq!(dec.push(&[0x1b, b'[', b'5', b'~']), vec![Key::PageUp]);
        assert_eq!(dec.push(b"\r\n"), vec![Key::Enter]);
    }
}
