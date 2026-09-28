//! `edit`: a small full-screen text editor in the spirit of nano.
//!
//! The file is loaded completely into memory as a list of lines (at most `MAX_FILE_BYTES`), edited
//! there and written back as a whole with Ctrl+S. The screen has three parts: a title bar with the
//! file name, the text with a line number gutter, and a status bar with messages, key hints and
//! the cursor position. It uses the terminal's font scale and colors.
//!
//! Every frame is described as rows of cells first; a row is only drawn again when its cells
//! differ from what is already on screen, so typing redraws a single line and blinking the cursor
//! redraws nothing else.
//!
//! Line endings: a file whose line breaks are all CRLF is saved with CRLF again, anything else
//! with LF. Like nano, a saved file always ends with a line break unless it is empty; the empty
//! line after the last line break is where the cursor goes to append.

use super::{theme, Shell};
use crate::drivers::display::font::{GLYPH_HEIGHT, GLYPH_WIDTH};
use crate::drivers::display::{Canvas, Display, Rect};
use crate::drivers::keyboard::{self, Key, KeyEvent};
use crate::drivers::timer;
use crate::fs::{self, FsError};
use crate::memory;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Largest file the editor opens. The text is held as `char`s (4 bytes each), so this keeps a
/// buffer well inside the 32 MiB kernel heap.
const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// Heap that must still be free once a file is loaded, for drawing, editing and saving.
const MIN_FREE_HEAP_BYTES: usize = 1024 * 1024;
/// Tab stops are this many columns apart; the Tab key inserts spaces up to the next one.
const TAB_WIDTH: usize = 4;
const CURSOR_BLINK_NANOSECONDS: u64 = 530_000_000;
/// Same text metrics as the terminal (see terminal.rs), in font pixels.
const LINE_SPACING: usize = 2;
const MARGIN: usize = 6;
/// Extra space above and below the text of the title and status bars, in font pixels.
const BAR_PADDING: usize = 3;
/// Smallest width of the line number column, in digits.
const MIN_GUTTER_DIGITS: usize = 3;

/// Characters typed with AltGr on the Swiss German layout.
const ALT_GR_CHARACTERS: &str = "@#|~[]{}\\";

const BAR_BACKGROUND: u32 = 0xFF1C2130;
const CURRENT_LINE_BACKGROUND: u32 = 0xFF161A23;
const MATCH_BACKGROUND: u32 = theme::YELLOW;

/// The `edit` command.
pub fn edit(shell: &mut Shell, arguments: &[&str]) {
    let [argument] = arguments else {
        shell.print_error("usage: edit <file>");
        return;
    };
    if shell.is_capturing() {
        shell.abort_redirect();
        shell.print_error("edit: the editor's screen cannot be redirected into a file");
        return;
    }
    let path = fs::normalize(&shell.cwd, argument);
    if fs::path::names_directory(argument) {
        // `edit name/` never means the file `name`.
        let problem = match fs::with_volume(|volume| volume.metadata(&path)) {
            Ok(metadata) if metadata.is_dir => "is a directory",
            Ok(_) => "not a directory",
            Err(_) => "no such directory",
        };
        shell.print_error(&format!("edit: {}: {}", argument, problem));
        return;
    }
    let document = match Document::open(&path) {
        Ok(document) => document,
        Err(message) => {
            shell.print_error(&format!("edit: {}: {}", argument, message));
            return;
        }
    };

    let scale = shell.terminal.scale();
    let display = shell.terminal.display_mut();
    let mut editor = Editor::new(path, document, Layout::new(display, scale));
    editor.run(display);

    // Give the screen back to the shell exactly as it was.
    shell.terminal.redraw();
    shell.terminal.flush();
}

// ---------------------------------------------------------------------------------------------
// Loading and saving
// ---------------------------------------------------------------------------------------------

/// The text of a file as the editor holds it.
struct Document {
    /// Never empty; a file ending with a line break has an empty last line.
    lines: Vec<Vec<char>>,
    crlf: bool,
    is_new: bool,
    /// A notice to show once the editor is open, e.g. about replaced invalid UTF-8.
    notice: Option<(String, u32)>,
}

impl Document {
    /// Loads `path`, or starts an empty document if it does not exist yet but its directory does.
    /// Errors are messages for the user.
    fn open(path: &str) -> Result<Self, String> {
        let metadata = match fs::with_volume(|volume| volume.metadata(path)) {
            Ok(metadata) => metadata,
            Err(FsError::Fs(fat32::Error::NotFound)) => return Self::new_file(path),
            Err(error) => return Err(format!("{}", error)),
        };
        if metadata.is_dir {
            return Err(String::from("is a directory"));
        }
        if metadata.size > MAX_FILE_BYTES {
            return Err(format!(
                "too large to edit ({} bytes; the editor opens files up to {} MiB)",
                metadata.size,
                MAX_FILE_BYTES >> 20
            ));
        }

        let bytes = fs::with_volume(|volume| volume.read_file(path)).map_err(|error| format!("{}", error))?;
        if bytes.contains(&0) {
            return Err(String::from("binary file (it contains NUL bytes); use hexdump to look at it"));
        }
        let decoded = text::decode(&bytes).ok_or_else(not_enough_memory)?;
        drop(bytes);
        let (used, total) = memory::heap_usage();
        if total - used < MIN_FREE_HEAP_BYTES {
            return Err(not_enough_memory());
        }
        let notice = if decoded.lossy {
            Some((String::from("invalid UTF-8 was replaced with \u{FFFD}; saving writes the replacement"), theme::YELLOW))
        } else if metadata.read_only {
            Some((String::from("this file has the read-only attribute"), theme::YELLOW))
        } else {
            None
        };
        Ok(Self { lines: decoded.lines, crlf: decoded.crlf, is_new: false, notice })
    }

    fn new_file(path: &str) -> Result<Self, String> {
        // Refuse a name that could never be saved (FAT forbids it) before any text is typed.
        if let Err(error) = fat32::validate_name(fs::path::file_name(path)) {
            return Err(format!("{}", error));
        }
        let parent = fs::path::parent(path);
        match fs::with_volume(|volume| volume.metadata(parent)) {
            Ok(metadata) if metadata.is_dir => {}
            Ok(_) => return Err(format!("{} is not a directory", parent)),
            Err(FsError::Fs(fat32::Error::NotFound)) => return Err(format!("directory {} does not exist", parent)),
            Err(error) => return Err(format!("{}", error)),
        }
        Ok(Self { lines: vec![Vec::new()], crlf: false, is_new: true, notice: None })
    }
}

fn not_enough_memory() -> String {
    String::from("not enough free memory to edit this file")
}

// ---------------------------------------------------------------------------------------------
// Screen layout
// ---------------------------------------------------------------------------------------------

/// Where rows and cells are on screen. Row 0 is the title bar, rows 1..=text_rows hold text and
/// the last row is the status bar.
struct Layout {
    scale: usize,
    width: usize,
    height: usize,
    cell_width: usize,
    cell_height: usize,
    margin: usize,
    bar_height: usize,
    columns: usize,
    text_rows: usize,
}

impl Layout {
    fn new(display: &Display, scale: usize) -> Self {
        let (width, height) = (display.width(), display.height());
        let cell_width = GLYPH_WIDTH * scale;
        let cell_height = (GLYPH_HEIGHT + LINE_SPACING) * scale;
        let margin = MARGIN * scale;
        let bar_height = cell_height + 2 * BAR_PADDING * scale;
        let columns = (width.saturating_sub(2 * margin) / cell_width).max(1);
        let text_rows = (height.saturating_sub(2 * bar_height) / cell_height).max(1);
        Self { scale, width, height, cell_width, cell_height, margin, bar_height, columns, text_rows }
    }

    fn screen_rows(&self) -> usize {
        self.text_rows + 2
    }

    fn status_row(&self) -> usize {
        self.text_rows + 1
    }

    /// The whole band of a screen row (full display width) and the top of its text cells.
    fn row_geometry(&self, row: usize) -> (Rect, usize) {
        let (top, height, cells_top) = if row == 0 {
            (0, self.bar_height, BAR_PADDING * self.scale)
        } else if row == self.status_row() {
            let top = self.height.saturating_sub(self.bar_height);
            (top, self.bar_height, top + BAR_PADDING * self.scale)
        } else {
            let top = self.bar_height + (row - 1) * self.cell_height;
            (top, self.cell_height, top)
        };
        (Rect::new(0, top as i32, self.width as i32, height as i32), cells_top)
    }
}

// ---------------------------------------------------------------------------------------------
// Frame description
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    character: char,
    foreground: u32,
    background: u32,
}

/// One screen row: its background and a cell per text column.
#[derive(Clone, PartialEq, Eq)]
struct Row {
    background: u32,
    cells: Vec<Cell>,
}

impl Row {
    fn new(columns: usize, background: u32) -> Self {
        Self { background, cells: vec![Cell { character: ' ', foreground: theme::FOREGROUND, background }; columns] }
    }

    /// Writes `text` from `column` on, clipped at the row end. Returns the column after it.
    fn put(&mut self, column: usize, text: &str, foreground: u32) -> usize {
        let mut column = column;
        for character in text.chars() {
            self.set(column, character, foreground);
            column += 1;
        }
        column
    }

    fn set(&mut self, column: usize, character: char, foreground: u32) {
        if let Some(cell) = self.cells.get_mut(column) {
            cell.character = character;
            cell.foreground = foreground;
        }
    }

    /// Writes `text` so that it ends at the last column.
    fn put_right(&mut self, text: &str, foreground: u32) -> usize {
        let start = self.cells.len().saturating_sub(text.chars().count());
        self.put(start, text, foreground);
        start
    }

    fn highlight(&mut self, column: usize, foreground: u32, background: u32) {
        if let Some(cell) = self.cells.get_mut(column) {
            cell.foreground = foreground;
            cell.background = background;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The editor
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum PromptKind {
    Find,
    GoToLine,
}

/// A question asked in the status bar (search text, line number).
struct Prompt {
    kind: PromptKind,
    input: Vec<char>,
}

#[derive(PartialEq, Eq)]
enum Flow {
    Continue,
    Quit,
}

struct Editor {
    path: String,
    lines: Vec<Vec<char>>,
    crlf: bool,
    is_new: bool,
    modified: bool,
    /// Cursor as (line, character index in that line).
    row: usize,
    column: usize,
    /// Screen column the cursor tries to stay in while moving up and down.
    preferred_column: usize,
    /// First line and first screen column shown.
    top: usize,
    left: usize,
    layout: Layout,
    /// What is on screen now, per screen row; `None` forces a redraw.
    drawn: Vec<Option<Row>>,
    cursor_visible: bool,
    message: Option<(String, u32)>,
    prompt: Option<Prompt>,
    /// Set by a quit request with unsaved changes; a second request then quits.
    quit_armed: bool,
    /// Lines removed with Ctrl+K; consecutive cuts collect several lines.
    clipboard: Vec<Vec<char>>,
    last_key_was_cut: bool,
    last_search: Vec<char>,
    /// Search match to highlight as (line, first character, length).
    found: Option<(usize, usize, usize)>,
}

impl Editor {
    fn new(path: String, document: Document, layout: Layout) -> Self {
        let screen_rows = layout.screen_rows();
        Self {
            path,
            lines: document.lines,
            crlf: document.crlf,
            is_new: document.is_new,
            modified: false,
            row: 0,
            column: 0,
            preferred_column: 0,
            top: 0,
            left: 0,
            layout,
            drawn: vec![None; screen_rows],
            cursor_visible: true,
            message: document.notice,
            prompt: None,
            quit_armed: false,
            clipboard: Vec::new(),
            last_key_was_cut: false,
            last_search: Vec::new(),
            found: None,
        }
    }

    /// Handles keys until the user quits. Keys that arrive together are all applied before the
    /// next frame is drawn, so fast typing never waits on the screen.
    fn run(&mut self, display: &mut Display) {
        display.clear(theme::BACKGROUND);
        self.render(display);
        let mut next_blink = timer::uptime_nanoseconds() + CURSOR_BLINK_NANOSECONDS;

        loop {
            let mut pressed = false;
            while let Some(event) = keyboard::read_key() {
                pressed = true;
                if self.handle_key(event) == Flow::Quit {
                    return;
                }
            }

            let now = timer::uptime_nanoseconds();
            if pressed {
                self.cursor_visible = true;
                next_blink = now + CURSOR_BLINK_NANOSECONDS;
                self.render(display);
            } else if now >= next_blink {
                self.cursor_visible = !self.cursor_visible;
                next_blink = now + CURSOR_BLINK_NANOSECONDS;
                self.render(display);
            } else {
                x86_64::instructions::hlt();
            }
        }
    }

    // --- keys ------------------------------------------------------------------------------

    fn handle_key(&mut self, event: KeyEvent) -> Flow {
        if self.prompt.is_some() {
            self.handle_prompt_key(event);
            return Flow::Continue;
        }

        let is_quit = matches!(event.key, Key::Escape) || (event.ctrl && is_char(event.key, 'q'));
        let is_cut = event.ctrl && is_char(event.key, 'k');
        if !is_quit {
            self.quit_armed = false;
        }
        if !is_cut {
            self.last_key_was_cut = false;
        }
        self.message = None;
        self.found = None;

        if event.key == Key::Escape {
            return self.request_quit();
        }
        if event.ctrl {
            match event.key {
                // Some keyboards and hosts send AltGr as Ctrl+Alt; these characters only come
                // from AltGr on the Swiss layout, so they are typed text, not shortcuts.
                Key::Char(character) if ALT_GR_CHARACTERS.contains(character) => self.insert(character),
                Key::Char(character) => match character.to_ascii_lowercase() {
                    'q' => return self.request_quit(),
                    's' => self.save(),
                    'f' => self.open_prompt(PromptKind::Find),
                    'g' => self.open_prompt(PromptKind::GoToLine),
                    'k' => self.cut_line(),
                    'u' => self.paste(),
                    _ => {}
                },
                Key::Home => self.move_to(0, 0),
                Key::End => self.move_to(self.lines.len() - 1, usize::MAX),
                Key::Left => self.word_left(),
                Key::Right => self.word_right(),
                _ => self.handle_plain_key(event.key),
            }
        } else {
            self.handle_plain_key(event.key);
        }
        self.scroll_to_cursor();
        Flow::Continue
    }

    fn handle_plain_key(&mut self, key: Key) {
        match key {
            Key::Char(character) if !character.is_control() => self.insert(character),
            Key::Char(_) | Key::Escape => {}
            Key::Enter => self.split_line(),
            Key::Tab => {
                let spaces = TAB_WIDTH - self.cursor_screen_column() % TAB_WIDTH;
                (0..spaces).for_each(|_| self.insert(' '));
            }
            Key::Backspace => self.backspace(),
            Key::Delete => self.delete(),
            Key::Left => self.move_left(),
            Key::Right => self.move_right(),
            Key::Up => self.move_vertically(-1),
            Key::Down => self.move_vertically(1),
            Key::PageUp => self.page(-1),
            Key::PageDown => self.page(1),
            Key::Home => self.smart_home(),
            Key::End => self.move_to(self.row, usize::MAX),
        }
    }

    fn request_quit(&mut self) -> Flow {
        if !self.modified || self.quit_armed {
            return Flow::Quit;
        }
        self.quit_armed = true;
        self.message = Some((
            String::from("Unsaved changes! Press Ctrl+Q (or Esc) again to quit without saving, Ctrl+S to save"),
            theme::YELLOW,
        ));
        Flow::Continue
    }

    // --- editing ---------------------------------------------------------------------------

    fn changed(&mut self) {
        self.modified = true;
        self.preferred_column = self.cursor_screen_column();
    }

    /// Every edit that grows the text reserves its memory first; when the heap is full the edit
    /// is refused with this message instead of the allocation failing (which would panic).
    fn out_of_memory(&mut self) {
        self.message = Some((String::from("out of memory: this edit was not made (Ctrl+S still saves)"), theme::RED));
    }

    fn insert(&mut self, character: char) {
        if self.lines[self.row].try_reserve(1).is_err() {
            return self.out_of_memory();
        }
        self.lines[self.row].insert(self.column, character);
        self.column += 1;
        self.changed();
    }

    /// Enter: the rest of the line moves onto a new line below.
    fn split_line(&mut self) {
        let rest_length = self.lines[self.row].len() - self.column;
        let mut rest = Vec::new();
        if self.lines.try_reserve(1).is_err() || rest.try_reserve_exact(rest_length).is_err() {
            return self.out_of_memory();
        }
        rest.extend(self.lines[self.row].drain(self.column..));
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.column = 0;
        self.changed();
    }

    fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
            self.lines[self.row].remove(self.column);
        } else if self.row > 0 {
            let length = self.lines[self.row].len();
            if self.lines[self.row - 1].try_reserve(length).is_err() {
                return self.out_of_memory();
            }
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.column = self.lines[self.row].len();
            self.lines[self.row].extend(line);
        } else {
            return;
        }
        self.changed();
    }

    fn delete(&mut self) {
        if self.column < self.lines[self.row].len() {
            self.lines[self.row].remove(self.column);
        } else if self.row + 1 < self.lines.len() {
            let length = self.lines[self.row + 1].len();
            if self.lines[self.row].try_reserve(length).is_err() {
                return self.out_of_memory();
            }
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].extend(next);
        } else {
            return;
        }
        self.changed();
    }

    /// Ctrl+K: removes the current line into the clipboard. Cuts in a row collect their lines,
    /// so Ctrl+U puts the whole block back.
    fn cut_line(&mut self) {
        if !self.last_key_was_cut {
            self.clipboard.clear();
        }
        self.last_key_was_cut = true;
        if self.clipboard.try_reserve(1).is_err() {
            return self.out_of_memory();
        }
        if self.lines.len() == 1 {
            if self.lines[0].is_empty() {
                return;
            }
            self.clipboard.push(core::mem::take(&mut self.lines[0]));
        } else {
            self.clipboard.push(self.lines.remove(self.row));
            self.row = self.row.min(self.lines.len() - 1);
        }
        self.column = 0;
        self.changed();
    }

    /// Ctrl+U: inserts the cut lines above the current line.
    fn paste(&mut self) {
        if self.clipboard.is_empty() {
            self.message = Some((String::from("nothing to paste (Ctrl+K cuts the current line)"), theme::MUTED));
            return;
        }
        let Some(copies) = text::try_clone_lines(&self.clipboard) else { return self.out_of_memory() };
        if self.lines.try_reserve(copies.len()).is_err() {
            return self.out_of_memory();
        }
        let pasted = copies.len();
        let at = self.row;
        self.lines.splice(at..at, copies);
        self.row += pasted;
        self.column = 0;
        self.changed();
    }

    // --- cursor movement -------------------------------------------------------------------

    fn cursor_screen_column(&self) -> usize {
        text::screen_column(&self.lines[self.row], self.column)
    }

    /// Moves to a line and character index (clamped); the new column becomes the preferred one.
    fn move_to(&mut self, row: usize, column: usize) {
        self.row = row.min(self.lines.len() - 1);
        self.column = column.min(self.lines[self.row].len());
        self.preferred_column = self.cursor_screen_column();
    }

    fn move_left(&mut self) {
        if self.column > 0 {
            self.move_to(self.row, self.column - 1);
        } else if self.row > 0 {
            self.move_to(self.row - 1, usize::MAX);
        }
    }

    fn move_right(&mut self) {
        if self.column < self.lines[self.row].len() {
            self.move_to(self.row, self.column + 1);
        } else if self.row + 1 < self.lines.len() {
            self.move_to(self.row + 1, 0);
        }
    }

    fn word_left(&mut self) {
        if self.column == 0 {
            return self.move_left();
        }
        let column = text::previous_word_start(&self.lines[self.row], self.column);
        self.move_to(self.row, column);
    }

    fn word_right(&mut self) {
        if self.column == self.lines[self.row].len() {
            return self.move_right();
        }
        let column = text::next_word_end(&self.lines[self.row], self.column);
        self.move_to(self.row, column);
    }

    /// Up and Down keep the cursor in the same screen column where the lines allow it.
    fn move_vertically(&mut self, lines: isize) {
        let last = self.lines.len() as isize - 1;
        let row = (self.row as isize + lines).clamp(0, last) as usize;
        self.row = row;
        self.column = text::index_at_screen_column(&self.lines[row], self.preferred_column);
    }

    /// PageUp / PageDown move the view and the cursor by one screen of text.
    fn page(&mut self, direction: isize) {
        let distance = self.layout.text_rows.saturating_sub(1).max(1);
        let max_top = self.lines.len().saturating_sub(self.layout.text_rows);
        self.top = if direction < 0 { self.top.saturating_sub(distance) } else { (self.top + distance).min(max_top) };
        self.move_vertically(direction * distance as isize);
    }

    /// Home goes to the first non-blank character, or to the line start if already there.
    fn smart_home(&mut self) {
        let indent = self.lines[self.row].iter().take_while(|character| character.is_whitespace()).count();
        let column = if self.column == indent { 0 } else { indent };
        self.move_to(self.row, column);
    }

    /// After a jump (search, go to line) the target line is shown in the middle of the screen
    /// unless it is already visible.
    fn center_if_hidden(&mut self) {
        let rows = self.layout.text_rows;
        if self.row < self.top || self.row >= self.top + rows {
            let max_top = self.lines.len().saturating_sub(rows);
            self.top = self.row.saturating_sub(rows / 2).min(max_top);
        }
    }

    /// Scrolls the view so the cursor is visible. Horizontally it jumps a quarter screen at a
    /// time, and the last text column stays free for the "line continues" marker.
    fn scroll_to_cursor(&mut self) {
        let rows = self.layout.text_rows;
        if self.row < self.top {
            self.top = self.row;
        } else if self.row >= self.top + rows {
            self.top = self.row + 1 - rows;
        }

        let width = self.text_columns();
        let usable = width.saturating_sub(1).max(1);
        let jump = (width / 4).max(1);
        let column = self.cursor_screen_column();
        if column < self.left {
            self.left = column.saturating_sub(jump);
        } else if column >= self.left + usable {
            self.left = column + 1 + jump.min(usable - 1) - usable;
        }
    }

    // --- prompts ---------------------------------------------------------------------------

    fn open_prompt(&mut self, kind: PromptKind) {
        self.prompt = Some(Prompt { kind, input: Vec::new() });
    }

    fn handle_prompt_key(&mut self, event: KeyEvent) {
        let Some(prompt) = self.prompt.as_mut() else { return };
        let cancel = matches!(event.key, Key::Escape) || (event.ctrl && (is_char(event.key, 'c') || is_char(event.key, 'q')));
        if cancel {
            self.prompt = None;
            self.message = Some((String::from("cancelled"), theme::MUTED));
            return;
        }
        match event.key {
            Key::Char(character) if !event.ctrl && !character.is_control() => prompt.input.push(character),
            Key::Backspace => {
                prompt.input.pop();
            }
            Key::Enter => {
                let Some(prompt) = self.prompt.take() else { return };
                match prompt.kind {
                    PromptKind::Find => self.find(prompt.input),
                    PromptKind::GoToLine => self.go_to_line(&prompt.input),
                }
                self.scroll_to_cursor();
            }
            _ => {}
        }
    }

    /// Searches forward from the cursor, case-insensitively, wrapping around at the end. An empty
    /// search repeats the previous one.
    fn find(&mut self, input: Vec<char>) {
        if !input.is_empty() {
            self.last_search = input;
        }
        if self.last_search.is_empty() {
            return;
        }
        let needle = self.last_search.clone();
        let label: String = needle.iter().collect();
        match text::find(&self.lines, &needle, self.row, self.column + 1) {
            Some((row, column, wrapped)) => {
                self.move_to(row, column);
                self.center_if_hidden();
                self.found = Some((row, column, needle.len()));
                if wrapped {
                    self.message = Some((String::from("search wrapped around to the top"), theme::MUTED));
                }
            }
            None => self.message = Some((format!("\"{}\" not found", label), theme::RED)),
        }
    }

    fn go_to_line(&mut self, input: &[char]) {
        let text: String = input.iter().collect();
        match text.trim().parse::<usize>() {
            Ok(line) if line >= 1 => {
                self.move_to(line - 1, 0);
                self.center_if_hidden();
            }
            _ => self.message = Some((format!("not a line number: {}", text.trim()), theme::RED)),
        }
    }

    // --- saving ----------------------------------------------------------------------------

    /// Ctrl+S: writes the whole file and makes it durable on disk right away.
    fn save(&mut self) {
        let Some(bytes) = text::encode(&self.lines, self.crlf) else {
            self.message = Some((String::from("save failed: out of memory"), theme::RED));
            return;
        };
        let result = fs::with_volume(|volume| volume.write_file(&self.path, &bytes)).and_then(|()| fs::sync());
        match result {
            Ok(()) => {
                self.modified = false;
                self.is_new = false;
                let unit = if bytes.len() == 1 { "byte" } else { "bytes" };
                self.message = Some((format!("saved {} {} to {}", bytes.len(), unit, self.path), theme::GREEN));
            }
            Err(error) => self.message = Some((format!("save failed: {}", error), theme::RED)),
        }
    }

    // --- drawing ---------------------------------------------------------------------------

    fn gutter_width(&self) -> usize {
        digit_count(self.lines.len()).max(MIN_GUTTER_DIGITS) + 1
    }

    fn text_columns(&self) -> usize {
        self.layout.columns.saturating_sub(self.gutter_width()).max(1)
    }

    /// Draws every row that changed since the last frame and presents it.
    fn render(&mut self, display: &mut Display) {
        let mut changed = false;
        for screen_row in 0..self.layout.screen_rows() {
            let row = self.build_row(screen_row);
            if self.drawn[screen_row].as_ref() != Some(&row) {
                self.draw_row(display, screen_row, &row);
                self.drawn[screen_row] = Some(row);
                changed = true;
            }
        }
        if changed {
            display.present();
        }
    }

    fn draw_row(&self, display: &mut Display, screen_row: usize, row: &Row) {
        let layout = &self.layout;
        let (band, cells_top) = layout.row_geometry(screen_row);
        display.fill_rect(band, row.background);
        let glyph_y = (cells_top + LINE_SPACING * layout.scale / 2) as i32;
        for (column, cell) in row.cells.iter().enumerate() {
            let x = (layout.margin + column * layout.cell_width) as i32;
            if cell.background != row.background {
                let area = Rect::new(x, cells_top as i32, layout.cell_width as i32, layout.cell_height as i32);
                display.fill_rect(area, cell.background);
            }
            if cell.character != ' ' {
                display.draw_char(x, glyph_y, cell.character, cell.foreground, layout.scale as i32);
            }
        }
    }

    fn build_row(&self, screen_row: usize) -> Row {
        if screen_row == 0 {
            self.title_bar()
        } else if screen_row == self.layout.status_row() {
            self.status_bar()
        } else {
            self.text_row(self.top + screen_row - 1)
        }
    }

    fn title_bar(&self) -> Row {
        let mut row = Row::new(self.layout.columns, BAR_BACKGROUND);
        let count = self.lines.len();
        let details = format!("{}  {} line{}", if self.crlf { "CRLF" } else { "LF" }, count, if count == 1 { "" } else { "s" });
        let limit = row.put_right(&details, theme::MUTED).saturating_sub(2);

        let (marker, marker_color) = if self.modified {
            ("  [modified]", theme::YELLOW)
        } else if self.is_new {
            ("  [new file]", theme::GREEN)
        } else {
            ("", theme::MUTED)
        };
        // The marker matters more than the end of a very long path.
        let path_limit = limit.saturating_sub(marker.len());
        let mut column = put_clipped(&mut row, 0, "edit ", theme::ACCENT, path_limit);
        column = put_clipped(&mut row, column, &self.path, theme::FOREGROUND, path_limit);
        put_clipped(&mut row, column, marker, marker_color, limit);
        row
    }

    fn status_bar(&self) -> Row {
        let mut row = Row::new(self.layout.columns, BAR_BACKGROUND);
        let position = format!(
            "Ln {}/{}, Col {}",
            self.row + 1,
            self.lines.len(),
            self.cursor_screen_column() + 1
        );
        let position_start = row.put_right(&position, theme::FOREGROUND);
        // Everything on the left stops two columns before the position.
        let limit = position_start.saturating_sub(2);

        if let Some(prompt) = &self.prompt {
            let question = match prompt.kind {
                PromptKind::Find if self.last_search.is_empty() => String::from("Find: "),
                PromptKind::Find => format!("Find [{}]: ", self.last_search.iter().collect::<String>()),
                PromptKind::GoToLine => String::from("Go to line: "),
            };
            let question_end = put_clipped(&mut row, 0, &question, theme::YELLOW, limit);
            // Show the end of a long input, where the typing happens.
            let room = limit.saturating_sub(question_end + 1);
            let shown = &prompt.input[prompt.input.len().saturating_sub(room)..];
            let mut column = question_end;
            for &character in shown {
                row.set(column, character, theme::FOREGROUND);
                column += 1;
            }
            if self.cursor_visible && column < limit {
                row.highlight(column, theme::BACKGROUND, theme::CURSOR);
            }
        } else if let Some((message, color)) = &self.message {
            put_clipped(&mut row, 0, message, *color, limit);
        } else {
            let hints = [("^S", "Save"), ("^Q", "Quit"), ("^F", "Find"), ("^G", "Line"), ("^K", "Cut line"), ("^U", "Paste")];
            let mut column = 0;
            for (key, action) in hints {
                if column + key.len() + action.len() + 1 > limit {
                    break;
                }
                column = row.put(column, key, theme::ACCENT);
                column = row.put(column + 1, action, theme::MUTED) + 2;
            }
        }
        row
    }

    fn text_row(&self, line_index: usize) -> Row {
        let is_current = line_index == self.row;
        let background = if is_current { CURRENT_LINE_BACKGROUND } else { theme::BACKGROUND };
        let mut row = Row::new(self.layout.columns, background);
        let Some(line) = self.lines.get(line_index) else { return row };

        let gutter = self.gutter_width();
        let number = format!("{:>width$}", line_index + 1, width = gutter - 1);
        row.put(0, &number, if is_current { theme::FOREGROUND } else { theme::MUTED });

        let width = self.text_columns();
        let visible = self.left..self.left + width;
        let found = self.found.filter(|(row, _, _)| *row == line_index);
        let mut screen_column = 0;
        for (index, &character) in line.iter().enumerate() {
            let span = text::advance(screen_column, character) - screen_column;
            for offset in 0..span {
                let column = screen_column + offset;
                if !visible.contains(&column) {
                    continue;
                }
                let cell = gutter + column - self.left;
                let shown = if character == '\t' || offset > 0 { ' ' } else { character };
                row.set(cell, shown, theme::FOREGROUND);
                if found.is_some_and(|(_, start, length)| (start..start + length).contains(&index)) {
                    row.highlight(cell, theme::BACKGROUND, MATCH_BACKGROUND);
                }
            }
            screen_column += span;
            if screen_column >= visible.end {
                break;
            }
        }
        // The line goes on past the right edge.
        if text::screen_width(line) > visible.end {
            row.put(gutter + width - 1, ">", theme::MUTED);
        }

        if is_current && self.cursor_visible && self.prompt.is_none() {
            let column = self.cursor_screen_column();
            if visible.contains(&column) {
                row.highlight(gutter + column - self.left, theme::BACKGROUND, theme::CURSOR);
            }
        }
        row
    }
}

fn is_char(key: Key, expected: char) -> bool {
    matches!(key, Key::Char(character) if character.to_ascii_lowercase() == expected)
}

/// Writes `text` from `column`, stopping before `limit`. Returns the column after it.
fn put_clipped(row: &mut Row, column: usize, text: &str, color: u32, limit: usize) -> usize {
    let mut column = column;
    for character in text.chars() {
        if column >= limit {
            break;
        }
        row.set(column, character, color);
        column += 1;
    }
    column
}

/// Number of decimal digits of `value`, for sizing the line number column.
fn digit_count(mut value: usize) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}

// ---------------------------------------------------------------------------------------------
// Text helpers (pure functions, no screen or file system)
// ---------------------------------------------------------------------------------------------

mod text {
    use super::TAB_WIDTH;

    /// Extra line slots reserved when a file is loaded.
    const SPARE_LINES: usize = 1024;
    use alloc::borrow::Cow;
    use alloc::string::String;
    use alloc::vec::Vec;

    pub struct Decoded {
        pub lines: Vec<Vec<char>>,
        pub crlf: bool,
        /// Invalid UTF-8 was replaced with U+FFFD.
        pub lossy: bool,
    }

    /// Splits file contents into lines. The file uses CRLF when it has line breaks and every one
    /// of them is CRLF; then the `\r`s are removed. A final line break leaves an empty last line.
    /// Returns `None` when there is not enough memory.
    pub fn decode(bytes: &[u8]) -> Option<Decoded> {
        let text = String::from_utf8_lossy(bytes);
        let lossy = matches!(text, Cow::Owned(_));
        let breaks = text.matches('\n').count();
        let crlf = breaks > 0 && text.matches("\r\n").count() == breaks;

        let mut lines = Vec::new();
        // Room for some new lines, so pressing Enter in a big file does not copy the whole list.
        lines.try_reserve_exact(breaks + 1 + SPARE_LINES).ok()?;
        for line in text.split('\n') {
            let line = if crlf { line.strip_suffix('\r').unwrap_or(line) } else { line };
            let mut characters = Vec::new();
            characters.try_reserve_exact(line.chars().count()).ok()?;
            characters.extend(line.chars());
            lines.push(characters);
        }
        Some(Decoded { lines, crlf, lossy })
    }

    /// Copies lines, or returns `None` when there is not enough memory.
    pub fn try_clone_lines(lines: &[Vec<char>]) -> Option<Vec<Vec<char>>> {
        let mut copies = Vec::new();
        copies.try_reserve_exact(lines.len()).ok()?;
        for line in lines {
            let mut copy = Vec::new();
            copy.try_reserve_exact(line.len()).ok()?;
            copy.extend_from_slice(line);
            copies.push(copy);
        }
        Some(copies)
    }

    /// Joins lines back into file contents, ending with a line break unless the last line is
    /// empty (it then stands for the final line break). Returns `None` when memory runs out.
    pub fn encode(lines: &[Vec<char>], crlf: bool) -> Option<Vec<u8>> {
        let newline: &[u8] = if crlf { b"\r\n" } else { b"\n" };
        let estimate = lines.iter().map(|line| line.len() + newline.len()).sum();
        let mut bytes = Vec::new();
        bytes.try_reserve(estimate).ok()?;
        let mut buffer = [0u8; 4];
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                bytes.try_reserve(newline.len()).ok()?;
                bytes.extend_from_slice(newline);
            }
            for &character in line {
                let encoded = character.encode_utf8(&mut buffer).as_bytes();
                bytes.try_reserve(encoded.len()).ok()?;
                bytes.extend_from_slice(encoded);
            }
        }
        if lines.last().is_some_and(|line| !line.is_empty()) {
            bytes.try_reserve(newline.len()).ok()?;
            bytes.extend_from_slice(newline);
        }
        Some(bytes)
    }

    /// Screen column after `character` when it starts at `column` (tabs jump to the next stop).
    pub fn advance(column: usize, character: char) -> usize {
        if character == '\t' { column + TAB_WIDTH - column % TAB_WIDTH } else { column + 1 }
    }

    /// Screen column of the character at `index`.
    pub fn screen_column(line: &[char], index: usize) -> usize {
        line[..index.min(line.len())].iter().fold(0, |column, &character| advance(column, character))
    }

    pub fn screen_width(line: &[char]) -> usize {
        screen_column(line, line.len())
    }

    /// Index of the character shown at screen column `target` (or the line end).
    pub fn index_at_screen_column(line: &[char], target: usize) -> usize {
        let mut column = 0;
        for (index, &character) in line.iter().enumerate() {
            let next = advance(column, character);
            if next > target {
                return index;
            }
            column = next;
        }
        line.len()
    }

    fn is_word(character: char) -> bool {
        character.is_alphanumeric() || character == '_'
    }

    /// Ctrl+Left: start of the word before `index`.
    pub fn previous_word_start(line: &[char], index: usize) -> usize {
        let mut index = index.min(line.len());
        while index > 0 && !is_word(line[index - 1]) {
            index -= 1;
        }
        while index > 0 && is_word(line[index - 1]) {
            index -= 1;
        }
        index
    }

    /// Ctrl+Right: end of the word at or after `index`.
    pub fn next_word_end(line: &[char], index: usize) -> usize {
        let mut index = index.min(line.len());
        while index < line.len() && !is_word(line[index]) {
            index += 1;
        }
        while index < line.len() && is_word(line[index]) {
            index += 1;
        }
        index
    }

    fn matches_at(line: &[char], column: usize, needle: &[char]) -> bool {
        line.len() >= column + needle.len()
            && line[column..column + needle.len()].iter().zip(needle).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
    }

    /// Finds `needle` (case-insensitive) starting at (`row`, `column`) and wrapping around once.
    /// Returns (line, index, wrapped).
    pub fn find(lines: &[Vec<char>], needle: &[char], row: usize, column: usize) -> Option<(usize, usize, bool)> {
        if needle.is_empty() || lines.is_empty() {
            return None;
        }
        let count = lines.len();
        for step in 0..=count {
            let line_index = (row + step) % count;
            let line = &lines[line_index];
            let wrapped = row + step >= count;
            // The starting line is searched from the cursor first and from its start last.
            let (from, to) = match step {
                0 => (column, line.len()),
                _ if step == count => (0, column.min(line.len() + 1)),
                _ => (0, line.len()),
            };
            let found = (from..to.min(line.len() + 1)).find(|&start| matches_at(line, start, needle));
            if let Some(start) = found {
                return Some((line_index, start, wrapped));
            }
        }
        None
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use alloc::vec;

        fn chars(text: &str) -> Vec<char> {
            text.chars().collect()
        }

        fn lines(text: &[&str]) -> Vec<Vec<char>> {
            text.iter().map(|line| chars(line)).collect()
        }

        #[test]
        fn decode_and_encode_round_trip() {
            for text in ["", "a\n", "a\nb\n", "one\n\ntwo\n", "x\r\ny\r\n"] {
                let decoded = decode(text.as_bytes()).unwrap();
                assert_eq!(encode(&decoded.lines, decoded.crlf).unwrap(), text.as_bytes(), "{:?}", text);
            }
        }

        #[test]
        fn line_endings() {
            let decoded = decode(b"a\r\nb\r\n").unwrap();
            assert!(decoded.crlf);
            assert_eq!(decoded.lines, lines(&["a", "b", ""]));
            // Mixed endings are saved as LF and keep the lone \r as text.
            let mixed = decode(b"a\r\nb\n").unwrap();
            assert!(!mixed.crlf);
            assert_eq!(mixed.lines, lines(&["a\r", "b", ""]));
            // A missing final line break is added on save.
            assert_eq!(encode(&decode(b"no newline").unwrap().lines, false).unwrap(), b"no newline\n");
            assert_eq!(encode(&lines(&[""]), false).unwrap(), b"");
        }

        #[test]
        fn invalid_utf8_is_replaced() {
            let decoded = decode(&[b'a', 0xFF, b'\n']).unwrap();
            assert!(decoded.lossy);
            assert_eq!(decoded.lines[0], vec!['a', '\u{FFFD}']);
            assert!(!decode("ä\n".as_bytes()).unwrap().lossy);
            assert_eq!(try_clone_lines(&decoded.lines).unwrap(), decoded.lines);
        }

        #[test]
        fn tab_columns() {
            let line = chars("a\tb");
            assert_eq!(screen_column(&line, 1), 1);
            assert_eq!(screen_column(&line, 2), 4);
            assert_eq!(screen_width(&line), 5);
            assert_eq!(index_at_screen_column(&line, 0), 0);
            assert_eq!(index_at_screen_column(&line, 2), 1);
            assert_eq!(index_at_screen_column(&line, 4), 2);
            assert_eq!(index_at_screen_column(&line, 99), 3);
        }

        #[test]
        fn words() {
            let line = chars("let x_1 = 42;");
            assert_eq!(next_word_end(&line, 0), 3);
            assert_eq!(next_word_end(&line, 3), 7);
            assert_eq!(previous_word_start(&line, 7), 4);
            assert_eq!(previous_word_start(&line, 13), 10);
        }

        #[test]
        fn search_wraps_and_ignores_case() {
            let text = lines(&["alpha beta", "Gamma", "beta"]);
            assert_eq!(find(&text, &chars("beta"), 0, 0), Some((0, 6, false)));
            assert_eq!(find(&text, &chars("beta"), 0, 7), Some((2, 0, false)));
            assert_eq!(find(&text, &chars("BETA"), 2, 1), Some((0, 6, true)));
            assert_eq!(find(&text, &chars("gamma"), 1, 1), Some((1, 0, true)));
            assert_eq!(find(&text, &chars("delta"), 0, 0), None);
        }
    }
}
