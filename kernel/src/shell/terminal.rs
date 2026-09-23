use super::theme;
use crate::drivers::display::font::{GLYPH_HEIGHT, GLYPH_WIDTH};
use crate::drivers::display::{Canvas, Display, Rect};
use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

const MAX_SCROLLBACK_ROWS: usize = 2000;
const TAB_WIDTH: usize = 4;
/// Screen width per unit of text scale, so text stays readable at high resolutions.
const PIXELS_PER_TEXT_SCALE: usize = 800;
const LINE_SPACING: usize = 2;
const MARGIN: usize = 6;

#[derive(Clone, Copy)]
struct Cell {
    character: char,
    color: u32,
}

const BLANK: Cell = Cell { character: ' ', color: theme::FOREGROUND };

/// A text terminal: a grid of colored characters with scrollback, drawn onto the display.
///
/// Rows are addressed by absolute number, counted since boot, so positions stay valid while old
/// rows are dropped from the scrollback. Writing only updates the grid; `flush` redraws the rows
/// that changed and presents the frame.
pub struct Terminal {
    display: Display,
    scale: usize,
    cell_width: usize,
    cell_height: usize,
    origin_x: usize,
    origin_y: usize,
    columns: usize,
    rows: usize,
    lines: VecDeque<Vec<Cell>>,
    dropped_rows: usize,
    cursor_row: usize,
    cursor_column: usize,
    color: u32,
    scrolled_back: usize,
    cursor_visible: bool,
    dirty_rows: Vec<usize>,
    full_redraw: bool,
    drawn_view_top: usize,
}

impl Terminal {
    pub fn new(display: Display) -> Self {
        let scale = (display.width() / PIXELS_PER_TEXT_SCALE).max(1);
        let cell_width = GLYPH_WIDTH * scale;
        let cell_height = (GLYPH_HEIGHT + LINE_SPACING) * scale;
        let margin = MARGIN * scale;
        let columns = ((display.width() - 2 * margin) / cell_width).max(1);
        let rows = ((display.height() - 2 * margin) / cell_height).max(1);

        let mut lines = VecDeque::new();
        lines.push_back(vec![BLANK; columns]);

        Self {
            display,
            scale,
            cell_width,
            cell_height,
            origin_x: margin,
            origin_y: margin,
            columns,
            rows,
            lines,
            dropped_rows: 0,
            cursor_row: 0,
            cursor_column: 0,
            color: theme::FOREGROUND,
            scrolled_back: 0,
            cursor_visible: false,
            dirty_rows: Vec::new(),
            full_redraw: true,
            drawn_view_top: 0,
        }
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn scale(&self) -> usize {
        self.scale
    }

    pub fn display(&self) -> &Display {
        &self.display
    }

    pub fn display_mut(&mut self) -> &mut Display {
        self.full_redraw = true;
        &mut self.display
    }

    pub fn set_color(&mut self, color: u32) {
        self.color = color;
    }

    /// Cursor position as (absolute row, column).
    pub fn position(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_column)
    }

    pub fn set_position(&mut self, (row, column): (usize, usize)) {
        self.show_latest();
        self.dirty_rows.push(self.cursor_row);
        self.cursor_row = row.max(self.dropped_rows);
        self.cursor_column = column.min(self.columns);
        self.ensure_row(self.cursor_row);
        self.dirty_rows.push(self.cursor_row);
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        if self.cursor_visible != visible {
            self.cursor_visible = visible;
            self.dirty_rows.push(self.cursor_row);
        }
    }

    pub fn write(&mut self, text: &str) {
        for character in text.chars() {
            self.write_char(character);
        }
    }

    pub fn write_colored(&mut self, text: &str, color: u32) {
        let previous = core::mem::replace(&mut self.color, color);
        self.write(text);
        self.color = previous;
    }

    pub fn write_char(&mut self, character: char) {
        self.show_latest();
        match character {
            '\n' => self.new_line(),
            '\r' => self.set_position((self.cursor_row, 0)),
            '\t' => {
                let spaces = TAB_WIDTH - self.cursor_column % TAB_WIDTH;
                for _ in 0..spaces {
                    self.write_char(' ');
                }
            }
            _ => {
                if self.cursor_column >= self.columns {
                    self.new_line();
                }
                let (row, column, color) = (self.cursor_row, self.cursor_column, self.color);
                if let Some(line) = self.line_mut(row) {
                    line[column] = Cell { character, color };
                }
                self.dirty_rows.push(row);
                self.cursor_column += 1;
            }
        }
    }

    fn new_line(&mut self) {
        self.dirty_rows.push(self.cursor_row);
        self.cursor_row += 1;
        self.cursor_column = 0;
        self.ensure_row(self.cursor_row);
    }

    /// Removes all text and moves the cursor to the top left.
    pub fn clear(&mut self) {
        self.dropped_rows += self.lines.len();
        self.lines.clear();
        self.lines.push_back(vec![BLANK; self.columns]);
        self.cursor_row = self.dropped_rows;
        self.cursor_column = 0;
        self.scrolled_back = 0;
        self.full_redraw = true;
    }

    /// Scrolls the view into the scrollback (positive) or back towards the newest text (negative).
    pub fn scroll_view(&mut self, rows: isize) {
        let latest_top = self.latest_view_top();
        let maximum = latest_top - self.dropped_rows;
        let scrolled = (self.scrolled_back as isize + rows).clamp(0, maximum as isize) as usize;
        if scrolled != self.scrolled_back {
            self.scrolled_back = scrolled;
            self.full_redraw = true;
        }
    }

    /// Forces the next `flush` to redraw everything, e.g. after something else drew on screen.
    pub fn redraw(&mut self) {
        self.full_redraw = true;
    }

    /// Draws every change since the last flush and shows it.
    pub fn flush(&mut self) {
        let view_top = self.view_top();
        if view_top != self.drawn_view_top {
            self.full_redraw = true;
        }

        if self.full_redraw {
            self.display.clear(theme::BACKGROUND);
            for screen_row in 0..self.rows {
                self.draw_row(screen_row, view_top + screen_row);
            }
        } else {
            let mut rows = core::mem::take(&mut self.dirty_rows);
            rows.sort_unstable();
            rows.dedup();
            let visible = view_top..view_top + self.rows;
            for row in rows.iter().copied().filter(|row| visible.contains(row)) {
                self.draw_row(row - view_top, row);
            }
            rows.clear();
            self.dirty_rows = rows;
        }

        self.dirty_rows.clear();
        self.full_redraw = false;
        self.drawn_view_top = view_top;
        self.display.present();
    }

    fn draw_row(&mut self, screen_row: usize, row: usize) {
        let y = (self.origin_y + screen_row * self.cell_height) as i32;
        let row_area = Rect::new(self.origin_x as i32, y, (self.columns * self.cell_width) as i32, self.cell_height as i32);
        self.display.fill_rect(row_area, theme::BACKGROUND);

        let Some(index) = row.checked_sub(self.dropped_rows) else { return };
        let Some(line) = self.lines.get(index) else { return };
        let glyph_y = y + (LINE_SPACING * self.scale / 2) as i32;

        for (column, cell) in line.iter().enumerate() {
            if cell.character != ' ' {
                let x = (self.origin_x + column * self.cell_width) as i32;
                self.display.draw_char(x, glyph_y, cell.character, cell.color, self.scale as i32);
            }
        }

        if self.cursor_visible && self.scrolled_back == 0 && row == self.cursor_row {
            let column = self.cursor_column.min(self.columns - 1);
            let x = (self.origin_x + column * self.cell_width) as i32;
            self.display.fill_rect(Rect::new(x, y, self.cell_width as i32, self.cell_height as i32), theme::CURSOR);
            let character = line[column].character;
            self.display.draw_char(x, glyph_y, character, theme::BACKGROUND, self.scale as i32);
        }
    }

    fn line_mut(&mut self, row: usize) -> Option<&mut Vec<Cell>> {
        let index = row.checked_sub(self.dropped_rows)?;
        self.lines.get_mut(index)
    }

    fn last_row(&self) -> usize {
        self.dropped_rows + self.lines.len() - 1
    }

    fn ensure_row(&mut self, row: usize) {
        while self.last_row() < row {
            self.lines.push_back(vec![BLANK; self.columns]);
        }
        while self.lines.len() > MAX_SCROLLBACK_ROWS {
            self.lines.pop_front();
            self.dropped_rows += 1;
        }
    }

    fn latest_view_top(&self) -> usize {
        let bottom = self.last_row().max(self.cursor_row);
        (bottom + 1).saturating_sub(self.rows).max(self.dropped_rows)
    }

    fn view_top(&self) -> usize {
        self.latest_view_top().saturating_sub(self.scrolled_back).max(self.dropped_rows)
    }

    fn show_latest(&mut self) {
        if self.scrolled_back != 0 {
            self.scrolled_back = 0;
            self.full_redraw = true;
        }
    }
}

impl fmt::Write for Terminal {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.write(text);
        Ok(())
    }
}
