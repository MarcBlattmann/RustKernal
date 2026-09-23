mod calculator;
mod commands;
mod editor;
mod fs_commands;
mod line_editor;
mod parse;
mod terminal;
pub mod theme;

use crate::drivers::display::Display;
use crate::drivers::keyboard::{self, Key, KeyEvent};
use crate::drivers::timer;
use crate::fs::{self, FsError};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use line_editor::LineEditor;
use parse::{CurrentWord, Redirect};
pub use terminal::Terminal;

const CURSOR_BLINK_NANOSECONDS: u64 = 530_000_000;
const MAX_HISTORY: usize = 100;
/// Most names listed when a Tab completion is ambiguous.
const MAX_COMPLETION_LISTING: usize = 120;
/// Largest output a command may redirect into a file. The output is collected on the 32 MiB kernel
/// heap before it is written, so this stays well below that.
const MAX_CAPTURE_BYTES: usize = 4 * 1024 * 1024;

pub struct Shell {
    pub terminal: Terminal,
    pub history: Vec<String>,
    /// Canonical absolute path of the working directory.
    pub cwd: String,
    /// Where `cd -` goes back to.
    pub previous_cwd: String,
    editor: LineEditor,
    /// Output of the running command while it is redirected into a file.
    capture: Option<Vec<u8>>,
    capture_overflowed: bool,
    /// Prompt of a question a command asks (see `read_input`); `None` at the command prompt.
    input_prompt: Option<String>,
    input_cancelled: bool,
}

/// Starts the interactive shell on `display`. Never returns.
pub fn run(display: Display) -> ! {
    let mut shell = Shell {
        terminal: Terminal::new(display),
        history: Vec::new(),
        cwd: String::from("/"),
        previous_cwd: String::from("/"),
        editor: LineEditor::new(),
        capture: None,
        capture_overflowed: false,
        input_prompt: None,
        input_cancelled: false,
    };
    commands::print_banner(&mut shell);

    loop {
        shell.prompt();
        let line = shell.read_line();
        shell.execute(&line);
    }
}

impl Shell {
    /// Whether output currently goes into a file instead of the screen.
    pub fn is_capturing(&self) -> bool {
        self.capture.is_some()
    }

    fn capture_bytes(&mut self, bytes: &[u8]) -> bool {
        let Some(buffer) = &mut self.capture else { return false };
        // try_reserve: running out of heap aborts the redirect instead of the kernel.
        if buffer.len() + bytes.len() > MAX_CAPTURE_BYTES || buffer.try_reserve(bytes.len()).is_err() {
            self.capture_overflowed = true;
        } else {
            buffer.extend_from_slice(bytes);
        }
        true
    }

    pub fn print(&mut self, text: &str) {
        if !self.capture_bytes(text.as_bytes()) {
            self.terminal.write(text);
        }
    }

    pub fn print_colored(&mut self, text: &str, color: u32) {
        if !self.capture_bytes(text.as_bytes()) {
            self.terminal.write_colored(text, color);
        }
    }

    pub fn println(&mut self, text: &str) {
        self.print(text);
        self.print("\n");
    }

    pub fn println_colored(&mut self, text: &str, color: u32) {
        self.print_colored(text, color);
        self.print("\n");
    }

    /// Prints "label   value" with the label highlighted, for info style output.
    pub fn print_field(&mut self, label: &str, value: &str) {
        self.print_colored(&format!("  {:<12}", label), theme::ACCENT);
        self.println(value);
    }

    /// Prints an error message in red. Errors always go to the screen, also while the output is
    /// redirected into a file.
    pub fn print_error(&mut self, text: &str) {
        if self.terminal.position().1 != 0 {
            self.terminal.write_char('\n');
        }
        self.terminal.write_colored(text, theme::RED);
        self.terminal.write_char('\n');
    }

    /// Prints a warning in yellow; like errors it bypasses redirection.
    pub fn print_warning(&mut self, text: &str) {
        if self.terminal.position().1 != 0 {
            self.terminal.write_char('\n');
        }
        self.terminal.write_colored(text, theme::YELLOW);
        self.terminal.write_char('\n');
    }

    /// Writes file contents: unchanged into a redirect, otherwise as text with every
    /// non-printable byte shown as `.`.
    pub fn print_bytes(&mut self, bytes: &[u8]) {
        if self.capture_bytes(bytes) {
            return;
        }
        let mut text = String::with_capacity(bytes.len());
        let mut previous_carriage_return = false;
        for chunk in bytes.utf8_chunks() {
            for character in chunk.valid().chars() {
                if previous_carriage_return && character != '\n' {
                    text.push('.');
                }
                previous_carriage_return = character == '\r';
                match character {
                    '\r' => {}
                    '\n' | '\t' => text.push(character),
                    c if c.is_control() => text.push('.'),
                    c => text.push(c),
                }
            }
            for _ in chunk.invalid() {
                if core::mem::take(&mut previous_carriage_return) {
                    text.push('.');
                }
                text.push('.');
            }
        }
        if previous_carriage_return {
            text.push('.');
        }
        self.terminal.write(&text);
    }

    /// Asks the user a question from inside a command and returns the answer, or `None` when it
    /// was cancelled with Ctrl+C.
    pub fn read_input(&mut self, prompt: &str) -> Option<String> {
        self.input_prompt = Some(String::from(prompt));
        self.input_cancelled = false;
        self.prompt();
        let line = self.read_line();
        self.input_prompt = None;
        self.terminal.write_char('\n');
        if self.input_cancelled { None } else { Some(line) }
    }

    fn prompt(&mut self) {
        if self.terminal.position().1 != 0 {
            self.terminal.write_char('\n');
        }
        match &self.input_prompt {
            Some(prompt) => {
                let prompt = prompt.clone();
                self.terminal.write_colored(&prompt, theme::YELLOW);
            }
            None => {
                self.terminal.write_colored("rustkernel", theme::GREEN);
                self.terminal.write_colored(":", theme::MUTED);
                let cwd = self.cwd.clone();
                self.terminal.write_colored(&cwd, theme::ACCENT);
                self.terminal.write_colored(" $ ", theme::MUTED);
            }
        }
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
                let visible = (timer::uptime_nanoseconds() / CURSOR_BLINK_NANOSECONDS).is_multiple_of(2);
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
                    self.input_cancelled = true;
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

        let at_command_prompt = self.input_prompt.is_none();
        match event.key {
            Key::Enter => {
                editor.move_to_end(terminal);
                let line = editor.text();
                let trimmed = line.trim();
                if at_command_prompt && !trimmed.is_empty() && self.history.last().map(String::as_str) != Some(trimmed) {
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
            Key::Up if at_command_prompt => editor.browse_history(terminal, &self.history, true),
            Key::Down if at_command_prompt => editor.browse_history(terminal, &self.history, false),
            Key::PageUp => terminal.scroll_view((terminal.rows() / 2) as isize),
            Key::PageDown => terminal.scroll_view(-((terminal.rows() / 2) as isize)),
            Key::Escape => editor.set_text(terminal, ""),
            Key::Tab if at_command_prompt => self.complete(),
            Key::Up | Key::Down | Key::Tab => {}
        }
        None
    }

    /// Tab completion: command names for the first word (and after `help`), drive names after
    /// `mount`/`format`, file and directory paths everywhere else.
    fn complete(&mut self) {
        let before_cursor: String = self.editor.chars()[..self.editor.cursor()].iter().collect();
        let word = parse::current_word(&before_cursor);
        let command = parse::current_word(before_cursor.split_whitespace().next().unwrap_or("")).text;

        let candidates: Vec<(String, bool)> = if word.index == 0 && !word.is_redirect_target {
            commands::COMMANDS.iter().map(|command| (String::from(command.name), false)).collect()
        } else if word.index == 1 && !word.is_redirect_target && command == "help" {
            commands::COMMANDS.iter().map(|command| (String::from(command.name), false)).collect()
        } else if word.index == 1 && !word.is_redirect_target && matches!(command.as_str(), "mount" | "format") {
            fs_commands::drive_names(command == "mount").into_iter().map(|name| (name, false)).collect()
        } else {
            let directories_only = !word.is_redirect_target && matches!(command.as_str(), "cd" | "rmdir");
            return self.complete_path(&word, directories_only);
        };
        self.apply_completion(&word, "", &word.text, candidates);
    }

    fn complete_path(&mut self, word: &CurrentWord, directories_only: bool) {
        let (directory, partial) = match word.text.rfind('/') {
            Some(index) => (&word.text[..=index], &word.text[index + 1..]),
            None => ("", word.text.as_str()),
        };
        let listed = fs::normalize(&self.cwd, if directory.is_empty() { "." } else { directory });
        let Ok(entries) = fs::with_volume(|volume| volume.read_dir(&listed)) else { return };
        let candidates = entries
            .into_iter()
            .filter(|entry| entry.is_dir || !directories_only)
            .map(|entry| (entry.name, entry.is_dir))
            .collect();
        let (directory, partial) = (String::from(directory), String::from(partial));
        self.apply_completion(word, &directory, &partial, candidates);
    }

    /// Completes `partial` (the part of `word` after `directory`) from `candidates`
    /// (name, is_directory). Names match case-insensitively, as FAT does.
    fn apply_completion(&mut self, word: &CurrentWord, directory: &str, partial: &str, candidates: Vec<(String, bool)>) {
        let lowercase_partial = partial.to_lowercase();
        let mut matches: Vec<(String, bool)> =
            candidates.into_iter().filter(|(name, _)| name.to_lowercase().starts_with(&lowercase_partial)).collect();
        if matches.is_empty() {
            return;
        }
        matches.sort_by_key(|(name, _)| name.to_lowercase());
        matches.dedup_by(|a, b| a.0 == b.0);

        let first: Vec<char> = matches[0].0.chars().collect();
        let common_length = matches.iter().fold(first.len(), |length, (name, _)| {
            let same = first.iter().zip(name.chars()).take_while(|(a, b)| a.to_lowercase().eq(b.to_lowercase())).count();
            length.min(same)
        });
        let partial_length = partial.chars().count();

        let (completed, is_dir, finished) = if matches.len() == 1 {
            (matches[0].0.clone(), matches[0].1, !matches[0].1)
        } else if common_length > partial_length {
            (first[..common_length].iter().collect::<String>(), false, false)
        } else {
            self.list_completions(&matches);
            return;
        };

        let mut text = String::from(directory);
        text.push_str(&completed);
        if is_dir {
            text.push('/');
        }
        let mut replacement = match word.open_quote {
            Some('\'') => format!("'{}{}", text, if finished { "'" } else { "" }),
            Some(_) => parse::double_quote(&text, finished),
            None if parse::needs_quotes(&text) => parse::double_quote(&text, finished),
            None => text,
        };
        if finished {
            replacement.push(' ');
        }
        self.editor.replace_before_cursor(&mut self.terminal, word.start, &replacement);
    }

    /// Ambiguous completion: lists the options and redraws the prompt with the same input.
    fn list_completions(&mut self, matches: &[(String, bool)]) {
        let (text, cursor) = (self.editor.chars().to_vec(), self.editor.cursor());
        self.editor.move_to_end(&mut self.terminal);
        self.terminal.write_char('\n');
        let width = matches.iter().map(|(name, is_dir)| name.chars().count() + *is_dir as usize).max().unwrap_or(0) + 2;
        let width = width.max(12);
        let per_row = (self.terminal.columns() / width).max(1);
        for (index, (name, is_dir)) in matches.iter().take(MAX_COMPLETION_LISTING).enumerate() {
            let shown = if *is_dir { format!("{}/", name) } else { name.clone() };
            let color = if *is_dir { theme::ACCENT } else { theme::FOREGROUND };
            let padded = if (index + 1) % per_row == 0 { shown } else { format!("{:<width$}", shown, width = width) };
            self.terminal.write_colored(&padded, color);
            if (index + 1) % per_row == 0 {
                self.terminal.write_char('\n');
            }
        }
        if matches.len() > MAX_COMPLETION_LISTING {
            if self.terminal.position().1 != 0 {
                self.terminal.write_char('\n');
            }
            self.terminal.write_colored(&format!("... and {} more", matches.len() - MAX_COMPLETION_LISTING), theme::MUTED);
        }
        if self.terminal.position().1 != 0 {
            self.terminal.write_char('\n');
        }
        self.prompt();
        self.editor.resume(&mut self.terminal, &text, cursor);
    }

    fn execute(&mut self, line: &str) {
        self.terminal.write_char('\n');
        let parsed = match parse::parse(line) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.print_error(&format!("rustkernel: {}", error));
                return;
            }
        };
        let Some((name, arguments)) = parsed.words.split_first() else {
            if parsed.redirect.is_some() {
                self.print_error("rustkernel: missing command before '>'");
            }
            return;
        };

        match commands::find(name) {
            Some(command) => {
                let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
                match &parsed.redirect {
                    Some(redirect) => self.run_redirected(command.run, &arguments, redirect),
                    None => (command.run)(self, &arguments),
                }
            }
            None => {
                self.print_colored("command not found: ", theme::RED);
                self.println(name);
                if let Some(suggestion) = commands::suggest(name) {
                    self.println_colored(&format!("did you mean '{}'?", suggestion), theme::MUTED);
                } else {
                    self.println_colored("type 'help' to list commands", theme::MUTED);
                }
            }
        }
        self.after_command();
    }

    /// Runs a command with its output going into a file (`>` replaces, `>>` appends).
    fn run_redirected(&mut self, run: fn(&mut Shell, &[&str]), arguments: &[&str], redirect: &Redirect) {
        let path = fs::normalize(&self.cwd, &redirect.path);
        // Check the target first, like a shell opening the file before running the command.
        match fs::with_volume(|volume| volume.metadata(&path)) {
            Ok(metadata) if metadata.is_dir => {
                self.print_error(&format!("rustkernel: {}: is a directory", redirect.path));
                return;
            }
            Ok(_) | Err(FsError::Fs(fat32::Error::NotFound)) => {}
            Err(error) => {
                self.print_error(&format!("rustkernel: {}: {}", redirect.path, error));
                return;
            }
        }

        self.capture = Some(Vec::new());
        self.capture_overflowed = false;
        run(self, arguments);
        let output = self.capture.take().unwrap_or_default();
        if self.capture_overflowed {
            self.print_error(&format!(
                "rustkernel: {}: output larger than {} MiB or out of memory; nothing was written",
                redirect.path,
                MAX_CAPTURE_BYTES >> 20
            ));
            return;
        }
        let result = fs::with_volume(|volume| {
            if redirect.append { volume.append_file(&path, &output) } else { volume.write_file(&path, &output) }
        });
        if let Err(error) = result {
            self.print_error(&format!("rustkernel: {}: {}", redirect.path, error));
        }
    }

    /// Makes the command's changes durable and keeps the working directory valid.
    fn after_command(&mut self) {
        if let Err(error) = fs::sync() {
            self.print_error(&format!("sync: {}", error));
        }
        // If the working directory was removed or moved, fall back to its nearest ancestor.
        let mut cwd = self.cwd.clone();
        while cwd != "/" && !matches!(fs::with_volume(|volume| volume.metadata(&cwd)), Ok(metadata) if metadata.is_dir) {
            cwd = String::from(fs::path::parent(&cwd));
        }
        self.cwd = cwd;
    }
}
