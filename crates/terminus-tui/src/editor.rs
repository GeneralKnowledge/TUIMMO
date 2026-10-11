//! Single-line editor with history.

use std::collections::VecDeque;

use unicode_width::UnicodeWidthStr;

const HISTORY_CAP: usize = 100;

#[derive(Clone, Debug, Default)]
pub struct LineEditor {
    buf: String,
    /// Cursor position in characters.
    cursor: usize,
    history: VecDeque<String>,
    /// Index into history while browsing. `None` means the live draft.
    browsing: Option<usize>,
    draft: String,
}

impl LineEditor {
    pub fn insert(&mut self, ch: char) {
        self.stop_browsing_on_edit();
        let idx = self.byte_index();
        self.buf.insert(idx, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        self.stop_browsing_on_edit();
        if self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        let idx = self.byte_index();
        self.buf.remove(idx);
    }

    pub fn delete(&mut self) {
        self.stop_browsing_on_edit();
        if self.cursor >= self.buf.chars().count() {
            return;
        }
        let idx = self.byte_index();
        self.buf.remove(idx);
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        let len = self.buf.chars().count();
        if self.cursor < len {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.buf.chars().count();
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
        self.browsing = None;
        self.draft.clear();
    }

    pub fn delete_word(&mut self) {
        self.stop_browsing_on_edit();
        if self.cursor == 0 {
            return;
        }
        let chars: Vec<char> = self.buf.chars().collect();
        let mut index = self.cursor;
        while index > 0 && chars[index - 1].is_whitespace() {
            index -= 1;
        }
        while index > 0 && !chars[index - 1].is_whitespace() {
            index -= 1;
        }
        let start = chars.iter().take(index).collect::<String>().len();
        let end = chars.iter().take(self.cursor).collect::<String>().len();
        self.buf.replace_range(start..end, "");
        self.cursor = index;
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.browsing {
            None => {
                self.draft = self.buf.clone();
                self.browsing = Some(self.history.len() - 1);
            }
            Some(0) => {}
            Some(index) => self.browsing = Some(index - 1),
        }
        self.load_browse();
    }

    pub fn history_down(&mut self) {
        let Some(index) = self.browsing else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.browsing = None;
            self.buf = self.draft.clone();
        } else {
            self.browsing = Some(index + 1);
            self.load_browse();
            return;
        }
        self.cursor = self.buf.chars().count();
    }

    pub fn take(&mut self) -> String {
        let line = self.buf.trim().to_string();
        if !line.is_empty() {
            if self.history.back() != Some(&line) {
                self.history.push_back(line.clone());
            }
            while self.history.len() > HISTORY_CAP {
                self.history.pop_front();
            }
        }
        self.buf.clear();
        self.cursor = 0;
        self.browsing = None;
        self.draft.clear();
        line
    }

    pub fn text(&self) -> &str {
        &self.buf
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.buf = text.into();
        self.cursor = self.buf.chars().count();
        self.browsing = None;
    }

    /// Column of the cursor within the buffer, in terminal cells.
    pub fn cursor_width(&self) -> usize {
        self.buf.chars().take(self.cursor).collect::<String>().width()
    }

    fn load_browse(&mut self) {
        if let Some(index) = self.browsing {
            if let Some(line) = self.history.get(index) {
                self.buf = line.clone();
                self.cursor = self.buf.chars().count();
            }
        }
    }

    fn stop_browsing_on_edit(&mut self) {
        self.browsing = None;
    }

    fn byte_index(&self) -> usize {
        self.buf.char_indices().nth(self.cursor).map(|(idx, _)| idx).unwrap_or(self.buf.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_and_recalls_history() {
        let mut editor = LineEditor::default();
        for ch in "look".chars() {
            editor.insert(ch);
        }
        assert_eq!(editor.take(), "look");
        editor.history_up();
        assert_eq!(editor.text(), "look");
        editor.history_down();
        assert_eq!(editor.text(), "");
        editor.insert('a');
        editor.left();
        editor.insert('b');
        assert_eq!(editor.text(), "ba");
    }
}
