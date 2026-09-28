use super::terminal::Terminal;
use super::theme;
use alloc::string::String;
use alloc::vec::Vec;

/// The line being typed at the prompt: editing, cursor movement and history browsing.
pub struct LineEditor {
    buffer: Vec<char>,
    cursor: usize,
    /// Where the input starts on screen, right after the prompt.
    start: (usize, usize),
    drawn_length: usize,
    history_position: Option<usize>,
    draft: Vec<char>,
}

impl LineEditor {
    pub fn new() -> Self {
        Self { buffer: Vec::new(), cursor: 0, start: (0, 0), drawn_length: 0, history_position: None, draft: Vec::new() }
    }

    /// Starts a fresh line at the current terminal position.
    pub fn begin(&mut self, terminal: &Terminal) {
        self.buffer.clear();
        self.cursor = 0;
        self.drawn_length = 0;
        self.history_position = None;
        self.start = terminal.position();
    }

    /// Continues editing `text` at the current terminal position (after reprinting the prompt).
    pub fn resume(&mut self, terminal: &mut Terminal, text: &[char], cursor: usize) {
        self.begin(terminal);
        self.buffer.extend_from_slice(text);
        self.cursor = cursor.min(self.buffer.len());
        self.render(terminal);
    }

    pub fn text(&self) -> String {
        self.buffer.iter().collect()
    }

    pub fn chars(&self) -> &[char] {
        &self.buffer
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_text(&mut self, terminal: &mut Terminal, text: &str) {
        self.buffer = text.chars().collect();
        self.cursor = self.buffer.len();
        self.render(terminal);
    }

    pub fn insert(&mut self, terminal: &mut Terminal, character: char) {
        self.buffer.insert(self.cursor, character);
        self.cursor += 1;
        self.render(terminal);
    }

    /// Replaces the characters `start..end` with `text` and puts the cursor `back` characters
    /// before the end of the inserted text (used by tab completion).
    pub fn replace_range(&mut self, terminal: &mut Terminal, start: usize, end: usize, text: &str, back: usize) {
        let end = end.min(self.buffer.len());
        let start = start.min(end);
        self.buffer.splice(start..end, text.chars());
        self.cursor = start + text.chars().count().saturating_sub(back);
        self.render(terminal);
    }

    pub fn backspace(&mut self, terminal: &mut Terminal) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
            self.render(terminal);
        }
    }

    pub fn delete(&mut self, terminal: &mut Terminal) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
            self.render(terminal);
        }
    }

    pub fn move_to(&mut self, terminal: &mut Terminal, position: usize) {
        self.cursor = position.min(self.buffer.len());
        terminal.set_position(self.screen_position(terminal, self.cursor));
    }

    pub fn move_left(&mut self, terminal: &mut Terminal) {
        self.move_to(terminal, self.cursor.saturating_sub(1));
    }

    pub fn move_right(&mut self, terminal: &mut Terminal) {
        self.move_to(terminal, self.cursor + 1);
    }

    /// Moves the terminal cursor behind the input, e.g. before printing command output.
    pub fn move_to_end(&mut self, terminal: &mut Terminal) {
        self.move_to(terminal, self.buffer.len());
    }

    /// Steps through `history`; `older` goes back in time. The unfinished line is kept as a draft.
    pub fn browse_history(&mut self, terminal: &mut Terminal, history: &[String], older: bool) {
        if history.is_empty() {
            return;
        }
        let position = match (self.history_position, older) {
            (None, true) => {
                self.draft = self.buffer.clone();
                Some(history.len() - 1)
            }
            (None, false) => return,
            (Some(0), true) => Some(0),
            (Some(position), true) => Some(position - 1),
            (Some(position), false) if position + 1 < history.len() => Some(position + 1),
            (Some(_), false) => None,
        };

        self.history_position = position;
        self.buffer = match position {
            Some(position) => history[position].chars().collect(),
            None => core::mem::take(&mut self.draft),
        };
        self.cursor = self.buffer.len();
        self.render(terminal);
    }

    fn screen_position(&self, terminal: &Terminal, index: usize) -> (usize, usize) {
        let offset = self.start.1 + index;
        (self.start.0 + offset / terminal.columns(), offset % terminal.columns())
    }

    fn render(&mut self, terminal: &mut Terminal) {
        terminal.set_position(self.start);
        let text: String = self.buffer.iter().collect();
        terminal.write_colored(&text, theme::FOREGROUND);
        for _ in self.buffer.len()..self.drawn_length {
            terminal.write_char(' ');
        }
        self.drawn_length = self.buffer.len();
        terminal.set_position(self.screen_position(terminal, self.cursor));
    }
}
