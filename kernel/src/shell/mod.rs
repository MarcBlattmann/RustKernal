mod calculator;
mod commands;
mod line_editor;
mod terminal;
pub mod theme;

use crate::drivers::display::Display;
use crate::drivers::keyboard::{self, Key, KeyEvent};
use crate::drivers::timer;
use alloc::string::String;
use alloc::vec::Vec;
use line_editor::LineEditor;
pub use terminal::Terminal;

const CURSOR_BLINK_NANOSECONDS: u64 = 530_000_000;
const MAX_HISTORY: usize = 100;

pub struct Shell {
    pub terminal: Terminal,
    pub history: Vec<String>,
    editor: LineEditor,
}

/// Starts the interactive shell on `display`. Never returns.
pub fn run(display: Display) -> ! {
    let mut shell = Shell { terminal: Terminal::new(display), history: Vec::new(), editor: LineEditor::new() };
    commands::print_banner(&mut shell);

    loop {
        shell.prompt();
        let line = shell.read_line();
        shell.execute(&line);
    }
}

impl Shell {
    pub fn print(&mut self, text: &str) {
        self.terminal.write(text);
    }

    pub fn print_colored(&mut self, text: &str, color: u32) {
        self.terminal.write_colored(text, color);
    }

    pub fn println(&mut self, text: &str) {
        self.terminal.write(text);
        self.terminal.write_char('\n');
    }

    pub fn println_colored(&mut self, text: &str, color: u32) {
        self.terminal.write_colored(text, color);
        self.terminal.write_char('\n');
    }

    /// Prints "label   value" with the label highlighted, for info style output.
    pub fn print_field(&mut self, label: &str, value: &str) {
        self.print_colored(&alloc::format!("  {:<12}", label), theme::ACCENT);
        self.println(value);
    }

    fn prompt(&mut self) {
        if self.terminal.position().1 != 0 {
            self.terminal.write_char('\n');
        }
        self.terminal.write_colored("rustkernel", theme::GREEN);
        self.terminal.write_colored(" $ ", theme::MUTED);
        self.editor.begin(&self.terminal);
    }

    fn read_line(&mut self) -> String {
        let mut next_blink = timer::uptime_nanoseconds() + CURSOR_BLINK_NANOSECONDS;
        self.terminal.set_cursor_visible(true);
        self.terminal.flush();

        loop {
            if let Some(event) = keyboard::read_key() {
                let submitted = self.handle_key(event);
                self.terminal.set_cursor_visible(submitted.is_none());
                self.terminal.flush();
                if let Some(line) = submitted {
                    return line;
                }
                next_blink = timer::uptime_nanoseconds() + CURSOR_BLINK_NANOSECONDS;
            } else if timer::uptime_nanoseconds() >= next_blink {
                let visible = (timer::uptime_nanoseconds() / CURSOR_BLINK_NANOSECONDS) % 2 == 0;
                self.terminal.set_cursor_visible(visible);
                self.terminal.flush();
                next_blink += CURSOR_BLINK_NANOSECONDS;
            } else {
                x86_64::instructions::hlt();
            }
        }
    }

    /// Applies one key press to the input line. Returns the line once it is submitted.
    fn handle_key(&mut self, event: KeyEvent) -> Option<String> {
        let terminal = &mut self.terminal;
        let editor = &mut self.editor;

        if event.ctrl {
            match event.key {
                Key::Char('c') | Key::Char('C') => {
                    editor.move_to_end(terminal);
                    terminal.write_colored("^C", theme::MUTED);
                    return Some(String::new());
                }
                Key::Char('l') | Key::Char('L') => {
                    let (text, cursor) = (editor.chars().to_vec(), editor.cursor());
                    terminal.clear();
                    self.prompt();
                    self.editor.resume(&mut self.terminal, &text, cursor);
                }
                Key::Char('u') | Key::Char('U') => editor.set_text(terminal, ""),
                _ => {}
            }
            return None;
        }

        match event.key {
            Key::Enter => {
                editor.move_to_end(terminal);
                let line = editor.text();
                let trimmed = line.trim();
                if !trimmed.is_empty() && self.history.last().map(String::as_str) != Some(trimmed) {
                    self.history.push(trimmed.into());
                    if self.history.len() > MAX_HISTORY {
                        self.history.remove(0);
                    }
                }
                return Some(line);
            }
            Key::Char(character) => editor.insert(terminal, character),
            Key::Backspace => editor.backspace(terminal),
            Key::Delete => editor.delete(terminal),
            Key::Left => editor.move_left(terminal),
            Key::Right => editor.move_right(terminal),
            Key::Home => editor.move_to(terminal, 0),
            Key::End => editor.move_to_end(terminal),
            Key::Up if event.shift => terminal.scroll_view(1),
            Key::Down if event.shift => terminal.scroll_view(-1),
            Key::Up => editor.browse_history(terminal, &self.history, true),
            Key::Down => editor.browse_history(terminal, &self.history, false),
            Key::PageUp => terminal.scroll_view((terminal.rows() / 2) as isize),
            Key::PageDown => terminal.scroll_view(-((terminal.rows() / 2) as isize)),
            Key::Escape => editor.set_text(terminal, ""),
            Key::Tab => self.complete(),
        }
        None
    }

    /// Tab completion for command names (also after `help`).
    fn complete(&mut self) {
        let before_cursor: String = self.editor.chars()[..self.editor.cursor()].iter().collect();
        let words: Vec<&str> = before_cursor.split(' ').collect();
        let completing_command = words.len() == 1;
        let completing_help_topic = words.len() == 2 && words[0] == "help";
        if !completing_command && !completing_help_topic {
            return;
        }

        let partial = *words.last().unwrap_or(&"");
        let matches: Vec<&str> = commands::COMMANDS.iter().map(|command| command.name).filter(|name| name.starts_with(partial)).collect();
        if matches.is_empty() {
            return;
        }

        let common = matches.iter().fold(matches[0], |common, name| {
            let length = common.chars().zip(name.chars()).take_while(|(a, b)| a == b).count();
            &common[..length]
        });

        if matches.len() == 1 || common.len() > partial.len() {
            let suffix = if matches.len() == 1 { " " } else { "" };
            for character in common[partial.len()..].chars().chain(suffix.chars()) {
                self.editor.insert(&mut self.terminal, character);
            }
            return;
        }

        // Ambiguous: list the options and redraw the prompt with the same input.
        let (text, cursor) = (self.editor.chars().to_vec(), self.editor.cursor());
        self.editor.move_to_end(&mut self.terminal);
        self.terminal.write_char('\n');
        for name in &matches {
            self.terminal.write_colored(&alloc::format!("{:<12}", name), theme::ACCENT);
        }
        self.terminal.write_char('\n');
        self.prompt();
        self.editor.resume(&mut self.terminal, &text, cursor);
    }

    fn execute(&mut self, line: &str) {
        self.terminal.write_char('\n');
        let arguments: Vec<&str> = line.split_whitespace().collect();
        let Some((&name, arguments)) = arguments.split_first() else { return };

        match commands::find(name) {
            Some(command) => (command.run)(self, arguments),
            None => {
                self.print_colored("command not found: ", theme::RED);
                self.println(name);
                if let Some(suggestion) = commands::suggest(name) {
                    self.println_colored(&alloc::format!("did you mean '{}'?", suggestion), theme::MUTED);
                } else {
                    self.println_colored("type 'help' to list commands", theme::MUTED);
                }
            }
        }
    }
}
