//! Built-in Applications for Pursuit OS
//!
//! This module contains full-featured native apps that are implemented
//! directly in Rust rather than using .pa files. These apps have full
//! access to system functionality.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::vec;
use alloc::format;
use crate::drivers::display::screen::Screen;
use crate::drivers::filesystem::FILESYSTEM;
use super::widgets::{Rect, draw_filled_rect, draw_text, draw_char, draw_rect_border};
use super::theme::*;

// ============================================================================
// SYNTAX HIGHLIGHTING COLORS
// ============================================================================

const COLOR_KEYWORD: u32 = 0xFFFF79C6;     // Pink for keywords
const COLOR_STRING: u32 = 0xFFF1FA8C;      // Yellow for strings
const COLOR_COMMENT: u32 = 0xFF6272A4;     // Gray-blue for comments
const COLOR_NUMBER: u32 = 0xFFBD93F9;      // Purple for numbers
const COLOR_FUNCTION: u32 = 0xFF50FA7B;    // Green for functions
const COLOR_TYPE: u32 = 0xFF8BE9FD;        // Cyan for types
const COLOR_OPERATOR: u32 = 0xFFFF5555;    // Red for operators
const COLOR_NORMAL: u32 = 0xFFF8F8F2;      // White for normal text
const COLOR_LINE_NUM: u32 = 0xFF6272A4;    // Gray for line numbers
const COLOR_CURSOR_LINE: u32 = 0xFF44475A; // Highlight current line
const COLOR_SELECTION: u32 = 0xFF44475A;   // Selection background

// Keywords for different languages
const RUST_KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "const", "static", "if", "else", "match", "for", "while",
    "loop", "return", "break", "continue", "struct", "enum", "impl", "trait",
    "pub", "use", "mod", "crate", "self", "super", "where", "as", "in", "ref",
    "true", "false", "Some", "None", "Ok", "Err", "Self", "async", "await", "move",
];

const PA_KEYWORDS: &[&str] = &[
    "app", "vbox", "hbox", "label", "button", "textbox", "panel", "spacer",
    "script", "var", "func", "if", "else", "while", "return", "true", "false",
];

// ============================================================================
// CODE EDITOR
// ============================================================================

/// A text buffer that stores the content being edited
#[derive(Clone)]
pub struct TextBuffer {
    pub lines: Vec<String>,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub scroll_y: usize,
    pub filename: String,
    pub modified: bool,
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_line: 0,
            cursor_col: 0,
            scroll_y: 0,
            filename: String::from("untitled.txt"),
            modified: false,
        }
    }

    pub fn from_content(content: &str, filename: &str) -> Self {
        let lines: Vec<String> = content.lines().map(String::from).collect();
        Self {
            lines: if lines.is_empty() { vec![String::new()] } else { lines },
            cursor_line: 0,
            cursor_col: 0,
            scroll_y: 0,
            filename: String::from(filename),
            modified: false,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if c == '\n' {
            // Split line at cursor
            let current_line = &self.lines[self.cursor_line];
            let rest = current_line[self.cursor_col..].to_string();
            self.lines[self.cursor_line] = current_line[..self.cursor_col].to_string();
            self.lines.insert(self.cursor_line + 1, rest);
            self.cursor_line += 1;
            self.cursor_col = 0;
        } else {
            let line = &mut self.lines[self.cursor_line];
            if self.cursor_col >= line.len() {
                line.push(c);
            } else {
                line.insert(self.cursor_col, c);
            }
            self.cursor_col += 1;
        }
        self.modified = true;
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.lines[self.cursor_line].remove(self.cursor_col - 1);
            self.cursor_col -= 1;
            self.modified = true;
        } else if self.cursor_line > 0 {
            // Merge with previous line
            let current = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.lines[self.cursor_line].push_str(&current);
            self.modified = true;
        }
    }

    pub fn delete(&mut self) {
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col < line_len {
            self.lines[self.cursor_line].remove(self.cursor_col);
            self.modified = true;
        } else if self.cursor_line < self.lines.len() - 1 {
            // Merge with next line
            let next = self.lines.remove(self.cursor_line + 1);
            self.lines[self.cursor_line].push_str(&next);
            self.modified = true;
        }
    }

    pub fn move_cursor(&mut self, dx: i32, dy: i32) {
        // Vertical movement
        if dy < 0 && self.cursor_line > 0 {
            self.cursor_line -= 1;
        } else if dy > 0 && self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
        }
        
        // Clamp cursor column to line length
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col > line_len {
            self.cursor_col = line_len;
        }
        
        // Horizontal movement
        if dx < 0 && self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if dx > 0 && self.cursor_col < line_len {
            self.cursor_col += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor_col = 0;
    }

    pub fn end(&mut self) {
        self.cursor_col = self.lines[self.cursor_line].len();
    }

    pub fn to_string(&self) -> String {
        self.lines.join("\n")
    }
}

/// Determines the type of token for syntax highlighting
#[derive(Clone, Copy, PartialEq)]
pub enum TokenType {
    Normal,
    Keyword,
    String,
    Comment,
    Number,
    Function,
    Type,
    Operator,
}

/// Syntax highlighter for code
pub struct SyntaxHighlighter {
    language: Language,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Language {
    Rust,
    PursuitApp,
    Plain,
}

impl SyntaxHighlighter {
    pub fn new(filename: &str) -> Self {
        let language = if filename.ends_with(".rs") {
            Language::Rust
        } else if filename.ends_with(".pa") {
            Language::PursuitApp
        } else {
            Language::Plain
        };
        Self { language }
    }

    pub fn get_token_color(&self, text: &str, pos: usize) -> u32 {
        if self.language == Language::Plain {
            return COLOR_NORMAL;
        }

        let keywords = match self.language {
            Language::Rust => RUST_KEYWORDS,
            Language::PursuitApp => PA_KEYWORDS,
            Language::Plain => &[],
        };

        // Check if we're in a comment
        if let Some(comment_start) = text[..pos].rfind("//") {
            if !text[comment_start..pos].contains('\n') {
                return COLOR_COMMENT;
            }
        }

        // Check if we're in a string
        let before = &text[..pos];
        let quote_count = before.matches('"').count() - before.matches("\\\"").count();
        if quote_count % 2 == 1 {
            return COLOR_STRING;
        }

        // Get the current word
        let start = text[..pos].rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = text[pos..].find(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| pos + i)
            .unwrap_or(text.len());
        
        let word = &text[start..end];

        // Check for keywords
        if keywords.contains(&word) {
            return COLOR_KEYWORD;
        }

        // Check for numbers
        if word.chars().all(|c| c.is_ascii_digit() || c == '.') && !word.is_empty() {
            return COLOR_NUMBER;
        }

        // Check for types (capitalized words)
        if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            return COLOR_TYPE;
        }

        // Check for functions (followed by '(')
        if text[end..].starts_with('(') {
            return COLOR_FUNCTION;
        }

        COLOR_NORMAL
    }

    /// Colorize a line of text, returning (char, color) pairs
    pub fn colorize_line(&self, line: &str) -> Vec<(char, u32)> {
        if self.language == Language::Plain {
            return line.chars().map(|c| (c, COLOR_NORMAL)).collect();
        }

        let mut result = Vec::with_capacity(line.len());
        let mut in_string = false;
        let mut in_comment = false;
        let mut word_start = 0;

        let keywords = match self.language {
            Language::Rust => RUST_KEYWORDS,
            Language::PursuitApp => PA_KEYWORDS,
            Language::Plain => &[],
        };

        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];

            // Check for comment start
            if !in_string && i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
                in_comment = true;
            }

            if in_comment {
                result.push((c, COLOR_COMMENT));
                i += 1;
                continue;
            }

            // Check for string
            if c == '"' && (i == 0 || chars[i - 1] != '\\') {
                in_string = !in_string;
                result.push((c, COLOR_STRING));
                i += 1;
                continue;
            }

            if in_string {
                result.push((c, COLOR_STRING));
                i += 1;
                continue;
            }

            // Check for operators
            if "+-*/%=<>!&|^~".contains(c) {
                result.push((c, COLOR_OPERATOR));
                i += 1;
                continue;
            }

            // Check for numbers
            if c.is_ascii_digit() {
                result.push((c, COLOR_NUMBER));
                i += 1;
                continue;
            }

            // Check for word boundaries
            if c.is_alphanumeric() || c == '_' {
                // Find the whole word
                let word_end = (i..chars.len())
                    .find(|&j| !chars[j].is_alphanumeric() && chars[j] != '_')
                    .unwrap_or(chars.len());
                
                let word: String = chars[i..word_end].iter().collect();
                let color = if keywords.contains(&word.as_str()) {
                    COLOR_KEYWORD
                } else if word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    COLOR_TYPE
                } else if word_end < chars.len() && chars[word_end] == '(' {
                    COLOR_FUNCTION
                } else {
                    COLOR_NORMAL
                };

                for j in i..word_end {
                    result.push((chars[j], color));
                }
                i = word_end;
                continue;
            }

            // Default
            result.push((c, COLOR_NORMAL));
            i += 1;
        }

        result
    }
}

/// The Code Editor application state
pub struct CodeEditor {
    pub buffer: TextBuffer,
    pub highlighter: SyntaxHighlighter,
    pub visible_lines: usize,
    pub line_number_width: usize,
}

impl CodeEditor {
    pub fn new() -> Self {
        let buffer = TextBuffer::new();
        let highlighter = SyntaxHighlighter::new(&buffer.filename);
        Self {
            buffer,
            highlighter,
            visible_lines: 20,
            line_number_width: 4,
        }
    }

    pub fn open_file(&mut self, filename: &str, content: &str) {
        self.buffer = TextBuffer::from_content(content, filename);
        self.highlighter = SyntaxHighlighter::new(filename);
    }

    pub fn save_file(&mut self) -> bool {
        let content = self.buffer.to_string();
        let mut fs = FILESYSTEM.lock();
        
        // Create file if it doesn't exist
        if fs.get_file_info(&self.buffer.filename).is_none() {
            fs.create_file(self.buffer.filename.clone());
        }
        
        if fs.write_file(&self.buffer.filename, content.as_bytes()) {
            self.buffer.modified = false;
            true
        } else {
            false
        }
    }

    pub fn handle_key(&mut self, key: char, ctrl: bool) {
        if ctrl {
            match key {
                's' | 'S' => { self.save_file(); }
                _ => {}
            }
        } else {
            match key {
                '\x08' | '\u{0008}' => self.buffer.backspace(), // Backspace
                '\x7f' => self.buffer.delete(),    // Delete
                '\n' | '\r' => self.buffer.insert_char('\n'),
                _ if (key as u32) >= 32 => self.buffer.insert_char(key), // All printable chars
                _ => {}
            }
        }
    }

    pub fn handle_special_key(&mut self, key: SpecialKey) {
        match key {
            SpecialKey::Up => self.buffer.move_cursor(0, -1),
            SpecialKey::Down => self.buffer.move_cursor(0, 1),
            SpecialKey::Left => self.buffer.move_cursor(-1, 0),
            SpecialKey::Right => self.buffer.move_cursor(1, 0),
            SpecialKey::Home => self.buffer.home(),
            SpecialKey::End => self.buffer.end(),
            SpecialKey::PageUp => {
                for _ in 0..self.visible_lines {
                    self.buffer.move_cursor(0, -1);
                }
            }
            SpecialKey::PageDown => {
                for _ in 0..self.visible_lines {
                    self.buffer.move_cursor(0, 1);
                }
            }
            _ => {} // Ignore other special keys
        }
        self.update_scroll();
    }

    fn update_scroll(&mut self) {
        // Ensure cursor is visible
        if self.buffer.cursor_line < self.buffer.scroll_y {
            self.buffer.scroll_y = self.buffer.cursor_line;
        } else if self.buffer.cursor_line >= self.buffer.scroll_y + self.visible_lines {
            self.buffer.scroll_y = self.buffer.cursor_line - self.visible_lines + 1;
        }
    }

    /// Render the editor to a screen region
    pub fn render(&self, screen: &mut Screen, bounds: &Rect) {
        let line_height = 10;
        let char_width = 8;
        let gutter_width = (self.line_number_width + 1) * char_width;
        
        // Background
        draw_filled_rect(screen, bounds, 0xFF1E1E2E);
        
        // Draw visible lines
        let start_line = self.buffer.scroll_y;
        let end_line = (start_line + self.visible_lines).min(self.buffer.lines.len());
        
        for (i, line_idx) in (start_line..end_line).enumerate() {
            let y = bounds.y as usize + i * line_height + 2;
            
            // Highlight current line
            if line_idx == self.buffer.cursor_line {
                let line_rect = Rect::new(
                    bounds.x,
                    y as i32 - 1,
                    bounds.width,
                    line_height,
                );
                draw_filled_rect(screen, &line_rect, COLOR_CURSOR_LINE);
            }
            
            // Line number
            let line_num = format!("{:>width$}", line_idx + 1, width = self.line_number_width);
            draw_text(screen, bounds.x as usize + 4, y, &line_num, COLOR_LINE_NUM);
            
            // Line content with syntax highlighting
            let line = &self.buffer.lines[line_idx];
            let colored = self.highlighter.colorize_line(line);
            
            let text_x = bounds.x as usize + gutter_width;
            for (col, (c, color)) in colored.iter().enumerate() {
                let x = text_x + col * char_width;
                if x < bounds.x as usize + bounds.width - char_width {
                    draw_char(screen, x, y, *c, *color);
                }
            }
            
            // Draw cursor
            if line_idx == self.buffer.cursor_line {
                let cursor_x = text_x + self.buffer.cursor_col * char_width;
                let cursor_rect = Rect::new(cursor_x as i32, y as i32, 2, line_height - 2);
                draw_filled_rect(screen, &cursor_rect, COLOR_FOREGROUND);
            }
        }
        
        // Status bar
        let status_y = bounds.y + bounds.height as i32 - 16;
        let status_rect = Rect::new(bounds.x, status_y, bounds.width, 16);
        draw_filled_rect(screen, &status_rect, 0xFF313244);
        
        let modified_marker = if self.buffer.modified { " *" } else { "" };
        let status = format!(
            " {}{} | Ln {}, Col {}",
            self.buffer.filename,
            modified_marker,
            self.buffer.cursor_line + 1,
            self.buffer.cursor_col + 1
        );
        draw_text(screen, bounds.x as usize + 4, (status_y + 4) as usize, &status, COLOR_FOREGROUND);
    }
    
    /// Render ONLY the current cursor line - used for efficient keyboard updates
    pub fn render_cursor_line(&self, screen: &mut Screen, bounds: &Rect) {
        let line_height = 10;
        let char_width = 8;
        let gutter_width = (self.line_number_width + 1) * char_width;
        
        // Calculate which visual line the cursor is on
        let cursor_visual = self.buffer.cursor_line.saturating_sub(self.buffer.scroll_y);
        if cursor_visual >= self.visible_lines {
            return; // Cursor not visible
        }
        
        let y = bounds.y as usize + cursor_visual * line_height + 2;
        
        // Clear the entire line
        let line_rect = Rect::new(
            bounds.x,
            y as i32 - 1,
            bounds.width,
            line_height,
        );
        draw_filled_rect(screen, &line_rect, COLOR_CURSOR_LINE);
        
        // Line number
        let line_num = format!("{:>width$}", self.buffer.cursor_line + 1, width = self.line_number_width);
        draw_text(screen, bounds.x as usize + 4, y, &line_num, COLOR_LINE_NUM);
        
        // Line content with syntax highlighting
        let line = &self.buffer.lines[self.buffer.cursor_line];
        let colored = self.highlighter.colorize_line(line);
        
        let text_x = bounds.x as usize + gutter_width;
        for (col, (c, color)) in colored.iter().enumerate() {
            let x = text_x + col * char_width;
            if x < bounds.x as usize + bounds.width - char_width {
                draw_char(screen, x, y, *c, *color);
            }
        }
        
        // Draw cursor
        let cursor_x = text_x + self.buffer.cursor_col * char_width;
        let cursor_rect = Rect::new(cursor_x as i32, y as i32, 2, line_height - 2);
        draw_filled_rect(screen, &cursor_rect, COLOR_FOREGROUND);
        
        // Update status bar
        let status_y = bounds.y + bounds.height as i32 - 16;
        let status_rect = Rect::new(bounds.x, status_y, bounds.width, 16);
        draw_filled_rect(screen, &status_rect, 0xFF313244);
        
        let modified_marker = if self.buffer.modified { " *" } else { "" };
        let status = format!(
            " {}{} | Ln {}, Col {}",
            self.buffer.filename,
            modified_marker,
            self.buffer.cursor_line + 1,
            self.buffer.cursor_col + 1
        );
        draw_text(screen, bounds.x as usize + 4, (status_y + 4) as usize, &status, COLOR_FOREGROUND);
    }
}

// Use SpecialKey from keyboard driver
pub use crate::drivers::keyboard::SpecialKey;

// ============================================================================
// FILE EXPLORER
// ============================================================================

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
}

/// Context menu state for file explorer
#[derive(Clone, Copy, PartialEq)]
pub enum ContextMenuOption {
    NewFile,
    NewFolder,
    Delete,
    Rename,
    Refresh,
}

/// Action result from file explorer click
#[derive(Clone)]
pub enum ExplorerAction {
    None,
    OpenFile(String),      // Open file in code editor
    NavigateToDir(String), // Navigate into directory
}

pub struct FileExplorer {
    pub current_path: String,
    pub entries: Vec<FileEntry>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub visible_items: usize,
    /// Context menu state
    pub context_menu_visible: bool,
    pub context_menu_x: i32,
    pub context_menu_y: i32,
    pub context_menu_selected: usize,
    /// Input mode for creating new file/folder
    pub input_mode: Option<InputMode>,
    pub input_buffer: String,
    /// Track last click for double-click detection
    last_click_index: Option<usize>,
    click_count: u8,
}

#[derive(Clone, Copy, PartialEq)]
pub enum InputMode {
    NewFile,
    NewFolder,
    Rename,
}

impl FileExplorer {
    pub fn new() -> Self {
        let mut explorer = Self {
            current_path: String::from("/"),
            entries: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            visible_items: 12,
            context_menu_visible: false,
            context_menu_x: 0,
            context_menu_y: 0,
            context_menu_selected: 0,
            input_mode: None,
            input_buffer: String::new(),
            last_click_index: None,
            click_count: 0,
        };
        explorer.refresh();
        explorer
    }

    pub fn refresh(&mut self) {
        self.entries.clear();
        
        // Add ".." to go up a directory (except at root)
        if self.current_path != "/" {
            self.entries.push(FileEntry {
                name: String::from(".."),
                is_dir: true,
                size: 0,
            });
        }
        
        let fs = FILESYSTEM.lock();
        
        // List files in the current directory
        let files = if self.current_path == "/" {
            fs.list_files()
        } else {
            fs.list_directory(&self.current_path)
        };
        
        // Add directories first
        for (name, is_dir) in files.iter() {
            if *is_dir {
                self.entries.push(FileEntry {
                    name: name.clone(),
                    is_dir: true,
                    size: 0,
                });
            }
        }
        
        // Then files
        for (name, is_dir) in files.iter() {
            if !*is_dir {
                let full_path = if self.current_path == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", self.current_path, name)
                };
                let size = fs.get_file_info(&full_path).map(|(s, _)| s).unwrap_or(0);
                self.entries.push(FileEntry {
                    name: name.clone(),
                    is_dir: false,
                    size,
                });
            }
        }

        // Add embedded apps from kernel only at root
        if self.current_path == "/" {
            self.entries.push(FileEntry {
                name: String::from("about.pa"),
                is_dir: false,
                size: 0,
            });
        }

        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
        
        // Reset scroll
        self.scroll_offset = 0;
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.context_menu_visible {
            // Navigate context menu
            if delta < 0 && self.context_menu_selected > 0 {
                self.context_menu_selected -= 1;
            } else if delta > 0 && self.context_menu_selected < 4 {
                self.context_menu_selected += 1;
            }
            return;
        }
        
        if delta < 0 && self.selected_index > 0 {
            self.selected_index -= 1;
        } else if delta > 0 && self.selected_index < self.entries.len().saturating_sub(1) {
            self.selected_index += 1;
        }
        
        // Update scroll
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + self.visible_items {
            self.scroll_offset = self.selected_index - self.visible_items + 1;
        }
    }

    pub fn get_selected(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected_index)
    }
    
    /// Handle keyboard input
    pub fn handle_key(&mut self, key: char) {
        // If in input mode, handle text input
        if let Some(mode) = self.input_mode {
            match key {
                '\x08' => {
                    self.input_buffer.pop();
                }
                '\n' | '\r' => {
                    // Confirm input
                    if !self.input_buffer.is_empty() {
                        match mode {
                            InputMode::NewFile => {
                                self.create_file(&self.input_buffer.clone());
                            }
                            InputMode::NewFolder => {
                                self.create_directory(&self.input_buffer.clone());
                            }
                            InputMode::Rename => {
                                self.rename_selected(&self.input_buffer.clone());
                            }
                        }
                    }
                    self.input_mode = None;
                    self.input_buffer.clear();
                }
                c if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' => {
                    if self.input_buffer.len() < 32 {
                        self.input_buffer.push(c);
                    }
                }
                _ => {}
            }
            return;
        }
        
        // Handle context menu
        if self.context_menu_visible {
            match key {
                '\n' | '\r' => {
                    self.execute_context_menu_action();
                }
                _ => {
                    self.context_menu_visible = false;
                }
            }
            return;
        }
        
        // Normal mode shortcuts
        match key {
            '\n' | '\r' => {
                // Enter - activate selected (open file or enter folder)
                self.activate_selected();
            }
            '\x08' => {
                // Backspace - go up one directory
                self.navigate_up();
            }
            'n' | 'N' => {
                // New file
                self.input_mode = Some(InputMode::NewFile);
                self.input_buffer.clear();
            }
            'f' | 'F' => {
                // New folder
                self.input_mode = Some(InputMode::NewFolder);
                self.input_buffer.clear();
            }
            'd' | 'D' => {
                // Delete selected
                self.delete_selected();
            }
            'r' | 'R' => {
                // Refresh
                self.refresh();
            }
            'm' | 'M' => {
                // Show context menu
                self.show_context_menu(100, 100);
            }
            _ => {}
        }
    }
    
    /// Handle special keys
    pub fn handle_special_key(&mut self, key: SpecialKey) {
        if self.input_mode.is_some() {
            // Escape cancels input
            if matches!(key, SpecialKey::Escape) {
                self.input_mode = None;
                self.input_buffer.clear();
            }
            return;
        }
        
        if self.context_menu_visible {
            match key {
                SpecialKey::Up => {
                    if self.context_menu_selected > 0 {
                        self.context_menu_selected -= 1;
                    }
                }
                SpecialKey::Down => {
                    if self.context_menu_selected < 4 {
                        self.context_menu_selected += 1;
                    }
                }
                SpecialKey::Escape => {
                    self.context_menu_visible = false;
                }
                _ => {}
            }
            return;
        }
        
        match key {
            SpecialKey::Up => self.move_selection(-1),
            SpecialKey::Down => self.move_selection(1),
            SpecialKey::PageUp => {
                for _ in 0..5 {
                    self.move_selection(-1);
                }
            }
            SpecialKey::PageDown => {
                for _ in 0..5 {
                    self.move_selection(1);
                }
            }
            SpecialKey::Home => {
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            SpecialKey::End => {
                if !self.entries.is_empty() {
                    self.selected_index = self.entries.len() - 1;
                    if self.selected_index >= self.visible_items {
                        self.scroll_offset = self.selected_index - self.visible_items + 1;
                    }
                }
            }
            SpecialKey::Delete => {
                self.delete_selected();
            }
            _ => {}
        }
    }
    
    /// Show context menu at position
    pub fn show_context_menu(&mut self, x: i32, y: i32) {
        self.context_menu_visible = true;
        self.context_menu_x = x;
        self.context_menu_y = y;
        self.context_menu_selected = 0;
    }
    
    /// Execute selected context menu action
    fn execute_context_menu_action(&mut self) {
        self.context_menu_visible = false;
        match self.context_menu_selected {
            0 => { // New File
                self.input_mode = Some(InputMode::NewFile);
                self.input_buffer.clear();
            }
            1 => { // New Folder
                self.input_mode = Some(InputMode::NewFolder);
                self.input_buffer.clear();
            }
            2 => { // Delete
                self.delete_selected();
            }
            3 => { // Rename
                if let Some(entry) = self.get_selected() {
                    self.input_buffer = entry.name.clone();
                    self.input_mode = Some(InputMode::Rename);
                }
            }
            4 => { // Refresh
                self.refresh();
            }
            _ => {}
        }
    }
    
    /// Handle mouse click, returns (needs_redraw, action)
    pub fn handle_click(&mut self, x: i32, y: i32, bounds: &Rect, right_button: bool) -> (bool, ExplorerAction) {
        // Close context menu if clicking outside
        if self.context_menu_visible {
            let menu_rect = Rect::new(
                self.context_menu_x,
                self.context_menu_y,
                120,
                100
            );
            if !menu_rect.contains(x, y) {
                self.context_menu_visible = false;
                return (true, ExplorerAction::None);
            }
            // Check if clicking on menu item
            let rel_y = y - self.context_menu_y;
            if rel_y >= 0 && rel_y < 100 {
                self.context_menu_selected = (rel_y / 20) as usize;
                self.execute_context_menu_action();
                return (true, ExplorerAction::None);
            }
        }
        
        // Right click shows context menu
        if right_button {
            self.show_context_menu(x, y);
            return (true, ExplorerAction::None);
        }
        
        // Click in file list area
        let list_y = bounds.y + 28;
        let item_height = 20;
        
        if y >= list_y && y < bounds.y + bounds.height as i32 - 20 {
            let rel_y = y - list_y;
            let clicked_index = self.scroll_offset + (rel_y / item_height) as usize;
            if clicked_index < self.entries.len() {
                // Check for double-click (same item clicked twice)
                if self.last_click_index == Some(clicked_index) {
                    self.click_count += 1;
                    if self.click_count >= 2 {
                        // Double-click - open file or navigate to folder
                        self.click_count = 0;
                        self.last_click_index = None;
                        return (true, self.activate_selected());
                    }
                } else {
                    // Single click on new item - select it
                    self.selected_index = clicked_index;
                    self.last_click_index = Some(clicked_index);
                    self.click_count = 1;
                }
                return (true, ExplorerAction::None);
            }
        }
        
        // Click elsewhere resets double-click tracking
        self.last_click_index = None;
        self.click_count = 0;
        
        (false, ExplorerAction::None)
    }
    
    /// Navigate into a directory
    pub fn navigate_to(&mut self, path: &str) {
        self.current_path = String::from(path);
        self.refresh();
    }
    
    /// Go up one directory level
    pub fn navigate_up(&mut self) {
        if self.current_path != "/" {
            // Find last slash and truncate
            if let Some(pos) = self.current_path.rfind('/') {
                if pos == 0 {
                    self.current_path = String::from("/");
                } else {
                    self.current_path = self.current_path[..pos].to_string();
                }
                self.refresh();
            }
        }
    }
    
    /// Activate the selected item (Enter key or double-click)
    pub fn activate_selected(&mut self) -> ExplorerAction {
        if let Some(entry) = self.entries.get(self.selected_index) {
            let name = entry.name.clone();
            let is_dir = entry.is_dir;
            
            // Handle ".." to go up
            if name == ".." {
                self.navigate_up();
                return ExplorerAction::NavigateToDir(self.current_path.clone());
            }
            
            if is_dir {
                // Navigate into directory
                let new_path = if self.current_path == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", self.current_path, name)
                };
                self.navigate_to(&new_path);
                ExplorerAction::NavigateToDir(new_path)
            } else {
                // Open file in code editor - return full path
                let full_path = if self.current_path == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", self.current_path, name)
                };
                ExplorerAction::OpenFile(full_path)
            }
        } else {
            ExplorerAction::None
        }
    }

    pub fn create_file(&mut self, name: &str) -> bool {
        // Create file with full path based on current directory
        let full_path = if self.current_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", self.current_path, name)
        };
        
        let mut fs = FILESYSTEM.lock();
        if fs.create_file(full_path) {
            drop(fs);
            self.refresh();
            true
        } else {
            false
        }
    }

    pub fn create_directory(&mut self, name: &str) -> bool {
        // Create directory with full path based on current directory
        let full_path = if self.current_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", self.current_path, name)
        };
        
        let mut fs = FILESYSTEM.lock();
        if fs.create_directory(full_path) {
            drop(fs);
            self.refresh();
            true
        } else {
            false
        }
    }

    pub fn delete_selected(&mut self) -> bool {
        if let Some(entry) = self.get_selected() {
            // Skip ".." entry
            if entry.name == ".." {
                return false;
            }
            
            // Build full path for deletion
            let full_path = if self.current_path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", self.current_path, entry.name)
            };
            
            let mut fs = FILESYSTEM.lock();
            if fs.delete_file(&full_path) {
                drop(fs);
                self.refresh();
                return true;
            }
        }
        false
    }
    
    pub fn rename_selected(&mut self, new_name: &str) -> bool {
        // For now, we'll implement this as delete + create
        // A proper implementation would modify the filesystem entry
        if let Some(entry) = self.get_selected() {
            let old_name = entry.name.clone();
            let is_dir = entry.is_dir;
            
            let mut fs = FILESYSTEM.lock();
            
            // Get content if it's a file
            let content = if !is_dir {
                fs.read_file(&old_name)
            } else {
                None
            };
            
            // Delete old
            if fs.delete_file(&old_name) {
                // Create new
                if is_dir {
                    fs.create_directory(String::from(new_name));
                } else {
                    fs.create_file(String::from(new_name));
                    if let Some(data) = content {
                        fs.write_file(new_name, &data);
                    }
                }
                drop(fs);
                self.refresh();
                return true;
            }
        }
        false
    }

    pub fn render(&self, screen: &mut Screen, bounds: &Rect) {
        let item_height = 20;
        let icon_width = 24;
        
        // Background
        draw_filled_rect(screen, bounds, 0xFF1E1F29);
        
        // Header
        let header_rect = Rect::new(bounds.x, bounds.y, bounds.width, 24);
        draw_filled_rect(screen, &header_rect, 0xFF313244);
        draw_text(screen, bounds.x as usize + 8, bounds.y as usize + 6, &self.current_path, COLOR_FOREGROUND);
        
        // Shortcuts hint in header
        let hint = "N:New F:Folder D:Del R:Refresh";
        let hint_x = bounds.x as usize + bounds.width - hint.len() * 8 - 8;
        draw_text(screen, hint_x, bounds.y as usize + 6, hint, 0xFF6C7086);
        
        // File list
        let list_y = bounds.y + 28;
        let visible_end = (self.scroll_offset + self.visible_items).min(self.entries.len());
        
        for (i, entry) in self.entries[self.scroll_offset..visible_end].iter().enumerate() {
            let y = list_y + (i as i32 * item_height as i32);
            let item_rect = Rect::new(bounds.x + 4, y, bounds.width - 8, item_height);
            
            // Selection highlight
            if self.scroll_offset + i == self.selected_index {
                draw_filled_rect(screen, &item_rect, 0xFF45475A);
            }
            
            // Icon (folder or file)
            let icon = if entry.is_dir { "[D]" } else { "[F]" };
            let icon_color = if entry.is_dir { 0xFFF1FA8C } else { 0xFF89B4FA };
            draw_text(screen, bounds.x as usize + 8, y as usize + 4, icon, icon_color);
            
            // Name
            draw_text(screen, bounds.x as usize + 8 + icon_width, y as usize + 4, &entry.name, COLOR_FOREGROUND);
            
            // Size for files
            if !entry.is_dir {
                let size_str = format_size(entry.size);
                let size_x = bounds.x as usize + bounds.width - 60;
                draw_text(screen, size_x, y as usize + 4, &size_str, COLOR_LINE_NUM);
            }
        }
        
        // Status bar
        let status_y = bounds.y + bounds.height as i32 - 20;
        let status_rect = Rect::new(bounds.x, status_y, bounds.width, 20);
        draw_filled_rect(screen, &status_rect, 0xFF313244);
        
        let status = format!("{} items | Arrow keys to navigate", self.entries.len());
        draw_text(screen, bounds.x as usize + 8, (status_y + 4) as usize, &status, COLOR_FOREGROUND);
        
        // Input mode overlay
        if let Some(mode) = self.input_mode {
            let prompt = match mode {
                InputMode::NewFile => "New file name:",
                InputMode::NewFolder => "New folder name:",
                InputMode::Rename => "Rename to:",
            };
            
            // Draw input box at bottom
            let input_y = status_y - 24;
            let input_rect = Rect::new(bounds.x + 4, input_y, bounds.width - 8, 22);
            draw_filled_rect(screen, &input_rect, 0xFF45475A);
            draw_rect_border(screen, &input_rect, 0xFF89B4FA, 1);
            
            let input_text = format!("{} {}_", prompt, self.input_buffer);
            draw_text(screen, bounds.x as usize + 8, (input_y + 5) as usize, &input_text, COLOR_FOREGROUND);
        }
        
        // Context menu
        if self.context_menu_visible {
            self.render_context_menu(screen);
        }
    }
    
    fn render_context_menu(&self, screen: &mut Screen) {
        let menu_width = 120;
        let item_height = 20;
        let menu_items = ["New File", "New Folder", "Delete", "Rename", "Refresh"];
        let menu_height = menu_items.len() * item_height;
        
        let menu_rect = Rect::new(self.context_menu_x, self.context_menu_y, menu_width, menu_height);
        
        // Background
        draw_filled_rect(screen, &menu_rect, 0xFF313244);
        draw_rect_border(screen, &menu_rect, 0xFF6C7086, 1);
        
        // Menu items
        for (i, item) in menu_items.iter().enumerate() {
            let y = self.context_menu_y + (i as i32 * item_height as i32);
            
            // Highlight selected
            if i == self.context_menu_selected {
                let item_rect = Rect::new(self.context_menu_x + 1, y, menu_width - 2, item_height);
                draw_filled_rect(screen, &item_rect, 0xFF45475A);
            }
            
            draw_text(screen, (self.context_menu_x + 8) as usize, (y + 4) as usize, item, COLOR_FOREGROUND);
        }
    }
}

fn format_size(bytes: u32) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{} MB", bytes / (1024 * 1024))
    }
}

// ============================================================================
// TERMINAL EMULATOR
// ============================================================================

pub struct TerminalEmulator {
    pub lines: Vec<String>,
    pub input_buffer: String,
    pub scroll_offset: usize,
    pub visible_lines: usize,
    pub prompt: String,
    pub history: Vec<String>,
    pub history_index: usize,
}

impl TerminalEmulator {
    pub fn new() -> Self {
        let mut term = Self {
            lines: Vec::new(),
            input_buffer: String::new(),
            scroll_offset: 0,
            visible_lines: 18,
            prompt: String::from("pursuit> "),
            history: Vec::new(),
            history_index: 0,
        };
        term.print_line("Pursuit OS Terminal v0.1");
        term.print_line("Type 'help' for available commands.");
        term.print_line("");
        term
    }

    pub fn print_line(&mut self, text: &str) {
        self.lines.push(String::from(text));
        self.scroll_to_bottom();
    }

    fn scroll_to_bottom(&mut self) {
        if self.lines.len() > self.visible_lines {
            self.scroll_offset = self.lines.len() - self.visible_lines;
        }
    }

    pub fn handle_key(&mut self, key: char) {
        // Test: Push every key to input buffer to see if function is even called
        match key {
            '\x08' | '\u{0008}' => {
                // Backspace - remove one character
                if self.input_buffer.len() > 0 {
                    self.input_buffer.pop();
                }
            }
            '\n' | '\r' => {
                // Execute command
                let cmd = core::mem::take(&mut self.input_buffer);
                self.lines.push(format!("{}{}", self.prompt, cmd));
                
                if !cmd.trim().is_empty() {
                    self.history.push(cmd.clone());
                    self.history_index = self.history.len();
                    self.execute_command(&cmd);
                }
                
                self.scroll_to_bottom();
            }
            _ => {
                // Add any other character
                self.input_buffer.push(key);
            }
        }
    }

    pub fn handle_special_key(&mut self, key: SpecialKey) {
        match key {
            SpecialKey::Up => {
                if self.history_index > 0 {
                    self.history_index -= 1;
                    self.input_buffer = self.history[self.history_index].clone();
                }
            }
            SpecialKey::Down => {
                if self.history_index < self.history.len() {
                    self.history_index += 1;
                    if self.history_index < self.history.len() {
                        self.input_buffer = self.history[self.history_index].clone();
                    } else {
                        self.input_buffer.clear();
                    }
                }
            }
            SpecialKey::PageUp => {
                if self.scroll_offset > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(self.visible_lines / 2);
                }
            }
            SpecialKey::PageDown => {
                let max_scroll = self.lines.len().saturating_sub(self.visible_lines);
                self.scroll_offset = (self.scroll_offset + self.visible_lines / 2).min(max_scroll);
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.trim().split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "help" => {
                self.print_line("Available commands:");
                self.print_line("  help          - Show this help");
                self.print_line("  clear         - Clear the screen");
                self.print_line("  ls            - List files");
                self.print_line("  cat <file>    - Display file contents");
                self.print_line("  touch <file>  - Create empty file");
                self.print_line("  rm <file>     - Delete file");
                self.print_line("  mkdir <dir>   - Create directory");
                self.print_line("  echo <text>   - Print text");
                self.print_line("  run <app>     - Run a .pa app");
                self.print_line("  info          - System information");
            }
            "clear" => {
                self.lines.clear();
                self.scroll_offset = 0;
            }
            "ls" => {
                let fs = FILESYSTEM.lock();
                let files = fs.list_files();
                if files.is_empty() {
                    self.print_line("  (empty)");
                } else {
                    for (name, is_dir) in &files {
                        let type_str = if *is_dir { "DIR " } else { "FILE" };
                        self.print_line(&format!("  {} {}", type_str, name));
                    }
                }
            }
            "cat" => {
                if parts.len() < 2 {
                    self.print_line("Usage: cat <filename>");
                } else {
                    let fs = FILESYSTEM.lock();
                    if let Some(content) = fs.read_file(parts[1]) {
                        if let Ok(text) = core::str::from_utf8(&content) {
                            for line in text.lines() {
                                self.print_line(line);
                            }
                        } else {
                            self.print_line("(binary file)");
                        }
                    } else {
                        self.print_line(&format!("File not found: {}", parts[1]));
                    }
                }
            }
            "touch" => {
                if parts.len() < 2 {
                    self.print_line("Usage: touch <filename>");
                } else {
                    let mut fs = FILESYSTEM.lock();
                    if fs.create_file(String::from(parts[1])) {
                        self.print_line(&format!("Created: {}", parts[1]));
                    } else {
                        self.print_line("Failed to create file");
                    }
                }
            }
            "rm" => {
                if parts.len() < 2 {
                    self.print_line("Usage: rm <filename>");
                } else {
                    let mut fs = FILESYSTEM.lock();
                    if fs.delete_file(parts[1]) {
                        self.print_line(&format!("Deleted: {}", parts[1]));
                    } else {
                        self.print_line("Failed to delete file");
                    }
                }
            }
            "mkdir" => {
                if parts.len() < 2 {
                    self.print_line("Usage: mkdir <dirname>");
                } else {
                    let mut fs = FILESYSTEM.lock();
                    if fs.create_directory(String::from(parts[1])) {
                        self.print_line(&format!("Created directory: {}", parts[1]));
                    } else {
                        self.print_line("Failed to create directory");
                    }
                }
            }
            "echo" => {
                let text = parts[1..].join(" ");
                self.print_line(&text);
            }
            "run" => {
                if parts.len() < 2 {
                    self.print_line("Usage: run <app_id>");
                } else {
                    self.print_line(&format!("Opening app: {}", parts[1]));
                    // The window system will need to handle this
                }
            }
            "info" => {
                self.print_line("Pursuit OS v0.1");
                self.print_line("Architecture: x86_64");
                let fs = FILESYSTEM.lock();
                let (total, free, _, used_entries) = fs.get_stats();
                self.print_line(&format!("Disk: {} blocks ({} free)", total, free));
                self.print_line(&format!("Files: {} entries", used_entries));
            }
            _ => {
                self.print_line(&format!("Unknown command: {}", parts[0]));
                self.print_line("Type 'help' for available commands.");
            }
        }
    }

    pub fn render(&self, screen: &mut Screen, bounds: &Rect) {
        let line_height = 12;
        let char_width = 8;
        
        // Background
        draw_filled_rect(screen, bounds, 0xFF0C0C0C);
        
        // Draw output lines
        let start_line = self.scroll_offset;
        let end_line = (start_line + self.visible_lines).min(self.lines.len());
        
        for (i, line_idx) in (start_line..end_line).enumerate() {
            let y = bounds.y as usize + i * line_height + 4;
            let line = &self.lines[line_idx];
            draw_text(screen, bounds.x as usize + 4, y, line, 0xFF00FF00);
        }
        
        // Draw input line
        self.render_input_line(screen, bounds);
    }
    
    /// Render ONLY the input line - used for efficient keyboard updates
    pub fn render_input_line(&self, screen: &mut Screen, bounds: &Rect) {
        let line_height = 12;
        let char_width = 8;
        
        // Draw input line area (clear and redraw just this part)
        let input_y = bounds.y + bounds.height as i32 - line_height as i32 - 4;
        let input_rect = Rect::new(bounds.x, input_y - 2, bounds.width, line_height + 6);
        draw_filled_rect(screen, &input_rect, 0xFF1A1A1A);
        
        let input_text = format!("{}{}", self.prompt, self.input_buffer);
        draw_text(screen, bounds.x as usize + 4, input_y as usize, &input_text, 0xFF00FF00);
        
        // Cursor
        let cursor_x = bounds.x as usize + 4 + (self.prompt.len() + self.input_buffer.len()) * char_width;
        let cursor_rect = Rect::new(cursor_x as i32, input_y, 2, line_height - 2);
        draw_filled_rect(screen, &cursor_rect, 0xFF00FF00);
    }
}

// ============================================================================
// DOCUMENTATION VIEWER
// ============================================================================

pub struct DocViewer {
    pub content: Vec<String>,
    pub scroll_offset: usize,
    pub visible_lines: usize,
    pub current_doc: String,
}

impl DocViewer {
    pub fn new() -> Self {
        let mut viewer = Self {
            content: Vec::new(),
            scroll_offset: 0,
            visible_lines: 20,
            current_doc: String::from("index"),
        };
        viewer.load_doc("index");
        viewer
    }

    pub fn load_doc(&mut self, doc_id: &str) {
        self.current_doc = String::from(doc_id);
        self.content.clear();
        self.scroll_offset = 0;

        let content = match doc_id {
            "index" => DOC_INDEX,
            "create-app" => DOC_CREATE_APP,
            "layouts" => DOC_LAYOUTS,
            "scripting" => DOC_SCRIPTING,
            "elements" => DOC_ELEMENTS,
            "apps" => DOC_APPS,
            "native-apps" => DOC_NATIVE_APPS,
            "examples" => DOC_EXAMPLES,
            _ => "# Not Found\n\nDocumentation page not found.\n\nAvailable pages:\n- index\n- create-app\n- layouts\n- scripting\n- elements\n- apps\n- native-apps\n- examples",
        };

        for line in content.lines() {
            self.content.push(String::from(line));
        }
    }

    pub fn scroll(&mut self, delta: i32) {
        if delta < 0 && self.scroll_offset > 0 {
            self.scroll_offset -= (-delta as usize).min(self.scroll_offset);
        } else if delta > 0 {
            let max_scroll = self.content.len().saturating_sub(self.visible_lines);
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_scroll);
        }
    }
    
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }
    
    pub fn scroll_to_bottom(&mut self) {
        let max_scroll = self.content.len().saturating_sub(self.visible_lines);
        self.scroll_offset = max_scroll;
    }
    
    pub fn handle_key(&mut self, key: char) {
        match key {
            // Page up/down via keyboard
            'u' | 'U' | 'k' | 'K' => self.scroll(-5),  // Scroll up
            'd' | 'D' | 'j' | 'J' => self.scroll(5),   // Scroll down
            'g' => self.scroll_to_top(),               // Go to top
            'G' => self.scroll_to_bottom(),            // Go to bottom
            // Navigate to different doc pages
            '1' => self.load_doc("index"),
            '2' => self.load_doc("create-app"),
            '3' => self.load_doc("layouts"),
            '4' => self.load_doc("scripting"),
            '5' => self.load_doc("elements"),
            '6' => self.load_doc("apps"),
            '7' => self.load_doc("native-apps"),
            '8' => self.load_doc("examples"),
            _ => {}
        }
    }

    pub fn render(&self, screen: &mut Screen, bounds: &Rect) {
        let line_height = 12;
        
        // Background
        draw_filled_rect(screen, bounds, 0xFF1A1B26);
        
        // Header with page info
        let header_rect = Rect::new(bounds.x, bounds.y, bounds.width, 28);
        draw_filled_rect(screen, &header_rect, 0xFF24283B);
        
        let header_text = format!("Documentation - {}", self.current_doc);
        draw_text(screen, bounds.x as usize + 8, bounds.y as usize + 8, &header_text, COLOR_FOREGROUND);
        
        // Scroll position indicator
        let pos_text = format!("{}/{}", self.scroll_offset + 1, self.content.len().max(1));
        let pos_x = bounds.x as usize + bounds.width - pos_text.len() * 8 - 16;
        draw_text(screen, pos_x, bounds.y as usize + 8, &pos_text, 0xFF6C7086);
        
        // Content
        let content_y = bounds.y + 32;
        let end_line = (self.scroll_offset + self.visible_lines).min(self.content.len());
        
        for (i, line_idx) in (self.scroll_offset..end_line).enumerate() {
            let y = content_y as usize + i * line_height;
            let line = &self.content[line_idx];
            
            // Simple markdown rendering
            let (text, color) = if line.starts_with("# ") {
                (&line[2..], 0xFF7AA2F7) // Heading 1 - blue
            } else if line.starts_with("## ") {
                (&line[3..], 0xFFBB9AF7) // Heading 2 - purple
            } else if line.starts_with("### ") {
                (&line[4..], 0xFF9ECE6A) // Heading 3 - green
            } else if line.starts_with("- ") || line.starts_with("* ") {
                (line.as_str(), 0xFFE0AF68) // List item - yellow
            } else if line.starts_with("```") {
                (line.as_str(), 0xFF565F89) // Code block marker
            } else if line.starts_with("  ") && line.trim().len() > 0 {
                (line.as_str(), 0xFF73DACA) // Indented (code) - cyan
            } else {
                (line.as_str(), COLOR_FOREGROUND)
            };
            
            draw_text(screen, bounds.x as usize + 12, y, text, color);
        }
        
        // Scrollbar
        if self.content.len() > self.visible_lines {
            let scrollbar_height = bounds.height - 60;
            let thumb_height = (self.visible_lines as f32 / self.content.len() as f32 * scrollbar_height as f32) as usize;
            let thumb_height = thumb_height.max(20);
            let thumb_y = (self.scroll_offset as f32 / self.content.len() as f32 * scrollbar_height as f32) as i32;
            
            let track_rect = Rect::new(bounds.x + bounds.width as i32 - 8, content_y, 6, scrollbar_height);
            draw_filled_rect(screen, &track_rect, 0xFF24283B);
            
            let thumb_rect = Rect::new(bounds.x + bounds.width as i32 - 8, content_y + thumb_y, 6, thumb_height);
            draw_filled_rect(screen, &thumb_rect, 0xFF565F89);
        }
        
        // Footer with navigation hints
        let footer_y = bounds.y + bounds.height as i32 - 20;
        let footer_rect = Rect::new(bounds.x, footer_y, bounds.width, 20);
        draw_filled_rect(screen, &footer_rect, 0xFF24283B);
        draw_text(screen, bounds.x as usize + 8, (footer_y + 4) as usize, 
            "Arrows/jk:scroll g/G:top/end 1-8:pages", 0xFF6C7086);
    }
}

// Default documentation content (embedded)
const DOC_INDEX: &str = r#"# Pursuit OS - Developer Guide

Welcome! This guide teaches you how to
create applications for Pursuit OS.

## What is Pursuit OS?

A graphical operating system written
in Rust with a window manager and
desktop environment.

## Two Ways to Build Apps

### 1. Declarative Apps (.pa files)
- XML-like syntax
- No programming required
- Fast to create
- Great for simple apps

### 2. Native Rust Apps
- Full power of Rust
- Custom rendering
- Maximum performance
- For advanced apps

## Getting Started

1. Open the Code Editor app
2. Create a .pa file with app code
3. Save it to /apps/ folder
4. It appears in Start Menu!

## Learn About:

SEE ALSO:
- App Basics (create-app)
- Layouts & Widgets (layouts)
- Scripting (scripting)
- Advanced (native-apps)

## Applications

### Code Editor
Write and edit code
- Line numbers
- Syntax highlighting
- Save/load files

### File Manager
Browse files on disk
- Create folders
- Delete files
- View contents

### Terminal
Command-line interface
- Type 'help' for commands
- ls, mkdir, rm, touch, cat

### Documentation
You're reading it!
Use arrow keys to scroll.
"#;

const DOC_CREATE_APP: &str = r#"# Creating Your First App

## The Easy Way

Create a .pa file in /apps/

### Step 1: Open Code Editor

From Start Menu, launch "Code Editor"

### Step 2: Write Your App

  <app title="Hello" width="300"
       height="200">
    <vbox padding="20">
      <label>Hello, World!</label>
    </vbox>
  </app>

### Step 3: Save

Click File -> Save
Name it: hello.pa
It saves to /apps/ automatically!

### Step 4: Launch

Close Code Editor
Open Start Menu
Click "hello"
Your app opens!

## What Just Happened?

- Pursuit parsed your XML
- Created a window
- Displayed your content
- Added it to Start Menu

## Next Steps

- Add buttons (see: layouts)
- Add interactivity (see: scripting)
- Create complex layouts
- Save data to files

## Tips

- All tags must close: <tag>...</tag>
- Use padding for spacing
- Use vbox for vertical layout
- Use hbox for horizontal layout
"#;

const DOC_SCRIPTING: &str = r#"# PursuitScript - Add Interactivity

PursuitScript makes your apps interactive.

## Variables

Store and modify data:

  <app title="Counter">
    <script>
      var count = 0
      var name = "Item"
      var active = true
    </script>
  </app>

## Functions

Define actions:

  func increment() {
      count = count + 1
  }

  func reset() {
      count = 0
  }

  func multiply(n) {
      count = count * n
  }

## Button Events

Respond to clicks:

  <button on_click="increment()">
    Add One
  </button>

Event Names:
- on_click() for buttons
- on_submit() for textbox

## Control Flow

### If/Else

  if count > 10 {
      count = 0
  } else {
      count = count + 1
  }

### While Loops

  while count < 100 {
      count = count + 1
  }

## Operators

Math: + - * / %
Compare: == != < > <= >=
Logic: && || !

## String Operations

  var msg = "Hello"
  msg = msg + " World"

## Example: Counter App

  <app title="Counter">
    <script>
      var count = 0
      
      func increment() {
          count = count + 1
      }
      
      func reset() {
          count = 0
      }
    </script>
    
    <vbox padding="20" gap="10">
      <label>Count: {count}</label>
      <button on_click="increment()">
        +1
      </button>
      <button on_click="reset()">
        Reset
      </button>
    </vbox>
  </app>

Notice the {count} in label!
It updates when count changes.
"#;

const DOC_ELEMENTS: &str = r#"# UI Elements & Widgets

## Label

Display text:

  <label>Hello World!</label>
  <label>Count: {variable}</label>

Use {variable} for dynamic content
that updates when script changes it.

## Button

Clickable buttons with actions:

  <button on_click="myFunc()">
    Click Me
  </button>

The on_click value is a function
from your <script> section.

## TextBox

Input field for user text:

  <textbox/>

Access the value in script:
  var input = textbox.value

## Panel

Container with visible border:

  <panel padding="10">
    <label>Inside panel</label>
  </panel>

Use panels to group related content.

## Spacer

Empty space for layout:

  <hbox>
    <button>Left</button>
    <spacer/>
    <button>Right</button>
  </hbox>

The button with spacer between
gets pushed to opposite edges.

## Sizing Widgets

Set width and height:

  <button width="100" height="30">
    Wide Button
  </button>

If not set, uses defaults.

## Positioning

Absolute positioning:

  <label x="50" y="100">
    Custom Position
  </label>

Usually not needed with boxes!
"#;

const DOC_LAYOUTS: &str = r#"# Layouts & Positioning

Good layouts make good apps.

## VBox (Vertical)

Stack items top to bottom:

  <vbox padding="10" gap="5">
    <label>First</label>
    <label>Second</label>
    <label>Third</label>
  </vbox>

- padding: space inside container
- gap: space between items

Use VBox for:
- Forms
- Menus
- Lists
- Anything vertical

## HBox (Horizontal)

Stack items left to right:

  <hbox padding="10" gap="5">
    <button>Left</button>
    <button>Middle</button>
    <button>Right</button>
  </hbox>

Use HBox for:
- Button rows
- Tool bars
- Side-by-side items

## Combining Boxes

Make complex layouts:

  <vbox>
    <hbox>
      <button>File</button>
      <button>Edit</button>
    </hbox>
    
    <panel>
      <label>Content</label>
    </panel>
    
    <hbox>
      <label>Status</label>
      <spacer/>
      <label>Ready</label>
    </hbox>
  </vbox>

## Spacer for Alignment

Push items to edges:

  <hbox>
    <label>Left</label>
    <spacer/>
    <label>Right</label>
  </hbox>

Spacer takes remaining space.

## Centering

Center content:

  <hbox>
    <spacer/>
    <label>Centered</label>
    <spacer/>
  </hbox>

Put spacers on both sides.

## Padding vs Gap

- padding: space around container
- gap: space between items

  <vbox padding="20" gap="5">
    •20px margin outside
    •5px between each item
  </vbox>

## Best Practices

- Start with one VBox
- Add HBoxes for rows
- Use Spacers for spacing
- Nest containers logically
- Keep layouts simple
"#;

const DOC_APPS: &str = r#"# Complete App Structure

## Full App Template

  <app title="My App"
       width="400"
       height="300"
       x="100"
       y="100">
    
    <script>
      var myVar = 0
      
      func myFunction() {
        myVar = myVar + 1
      }
    </script>
    
    <vbox padding="15" gap="10">
      <label>Hello World</label>
    </vbox>
  </app>

## App Attributes

### Required
- title: Window title text
- width: Width in pixels
- height: Height in pixels

### Optional
- x: Starting X position (default: 100)
- y: Starting Y position (default: 100)

## Script Section

Place BEFORE layout:

  <script>
    var count = 0
    
    func increment() {
      count = count + 1
    }
  </script>

Variables available everywhere.
Functions called by buttons.

## Layout Section

Define UI after script:

  <vbox>
    <label>Value: {count}</label>
    <button on_click="increment()">
      Add 1
    </button>
  </vbox>

## Complete Example: Counter

  <app title="Counter" width="300"
       height="250">
    <script>
      var counter = 0
      
      func add() {
        counter = counter + 1
      }
      
      func subtract() {
        counter = counter - 1
      }
      
      func reset() {
        counter = 0
      }
    </script>
    
    <vbox padding="20" gap="15">
      <label>Counter App</label>
      <label>Value: {counter}</label>
      
      <hbox gap="5">
        <button on_click="subtract()">
          -1
        </button>
        <button on_click="reset()">
          Reset
        </button>
        <button on_click="add()">
          +1
        </button>
      </hbox>
    </vbox>
  </app>

## Saving Your App

1. Write the code above
2. Open Code Editor
3. File -> New
4. Paste the code
5. File -> Save
6. Name it counter.pa
7. Saved to /apps/counter.pa
8. Appears in Start Menu!

Click it to launch your app!
"#;

const DOC_NATIVE_APPS: &str = r#"# Native Rust Apps

For advanced applications.

## When to Use Native Apps

Use native apps when you need:
- Maximum performance
- Custom graphics
- System access
- Complex logic

## Builtin Apps

Pursuit OS includes native apps:

### Code Editor
- Syntax highlighting
- Multiple files
- Save/load
- Built-in Rust

### File Manager
- Browse directories
- Create files/folders
- Delete files
- Full file access

### Terminal
- Command line
- Built-in commands
- Shell scripting
- System access

### Documentation
- This viewer!
- Multiple pages
- Scrollable
- Formatted text

## Creating Native Apps

Requires Rust knowledge.
Edit kernel/src/gui/builtin_apps.rs

Add struct:

  pub struct MyApp {
      data: String,
  }

Implement methods:

  impl MyApp {
    pub fn new() -> Self {
      MyApp {
        data: String::new()
      }
    }
    
    pub fn render(&mut self, 
        screen: &mut Screen,
        bounds: &Rect) {
      // Custom drawing
    }
    
    pub fn handle_key(
        &mut self, key: char) {
      // Key handling
    }
  }

## Integration

Add to app.rs:
  pub fn create_myapp() -> AppDef

Add to start_menu.rs:
  "myapp" => "My App"

Rebuild with cargo run!

## Limitations

- Must rebuild entire OS
- Need Rust knowledge
- Complex debugging
- Slower iteration

Use .pa files when possible!
"#;

const DOC_EXAMPLES: &str = r#"# Example Applications

## 1. Simple To-Do List

  <app title="To-Do" width="400"
       height="300">
    <script>
      var tasks = ""
      
      func addTask() {
        tasks = tasks + "• New\n"
      }
      
      func clear() {
        tasks = ""
      }
    </script>
    
    <vbox padding="15" gap="10">
      <button on_click="addTask()">
        Add Task
      </button>
      <label>{tasks}</label>
      <button on_click="clear()">
        Clear All
      </button>
    </vbox>
  </app>

## 2. Simple Calculator

  <app title="Calc" width="300"
       height="350">
    <script>
      var result = 0
      var display = "0"
      
      func add(n) {
        result = result + n
        display = result
      }
      
      func equals() {
        display = result
      }
    </script>
    
    <vbox padding="10" gap="5">
      <label>{display}</label>
      <hbox gap="5">
        <button on_click="add(5)">5</button>
        <button on_click="add(10)">10</button>
      </hbox>
      <button on_click="equals()">
        =
      </button>
    </vbox>
  </app>

## 3. Color Picker

  <app title="Colors" width="300"
       height="200">
    <script>
      var color = "Red"
      
      func pick(c) {
        color = c
      }
    </script>
    
    <vbox padding="20" gap="10">
      <label>Color: {color}</label>
      <hbox gap="5">
        <button on_click="pick(Red)">
          Red
        </button>
        <button on_click="pick(Blue)">
          Blue
        </button>
        <button on_click="pick(Green)">
          Green
        </button>
      </hbox>
    </vbox>
  </app>

## 4. Greeting App

  <app title="Greet" width="350"
       height="200">
    <script>
      var greeting = "Hello"
      var name = ""
      
      func greet() {
        greeting = "Hello, " + name
      }
    </script>
    
    <vbox padding="20" gap="15">
      <label>What's your name?</label>
      <textbox/>
      <button on_click="greet()">
        Say Hello
      </button>
      <label>{greeting}</label>
    </vbox>
  </app>

## Try These!

1. Type the code in Code Editor
2. Save as .pa file
3. Close and open from Start Menu
4. Experiment!

Modify them:
- Add more buttons
- Change text
- Add variables
- Create new layouts
"#;
