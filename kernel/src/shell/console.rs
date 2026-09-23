use crate::drivers::display::font::{GLYPH_HEIGHT, GLYPH_WIDTH};
use crate::drivers::display::{Canvas, Display, Rect};

const BACKGROUND_COLOR: u32 = 0xFF000000;
const LINE_SPACING: usize = 2;
/// Screen width per unit of text scale, so text stays readable at high resolutions.
const PIXELS_PER_TEXT_SCALE: usize = 800;

pub struct Console {
    pub display: Display,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub text_color: u32,
    scale: usize,
}

impl Console {
    pub fn new(mut display: Display) -> Self {
        display.clear(BACKGROUND_COLOR);
        display.present();
        let scale = (display.width() / PIXELS_PER_TEXT_SCALE).max(1);

        let console = Console {
            display,
            cursor_x: 0,
            cursor_y: 0,
            text_color: 0xFFFFFFFF,
            scale,
        };

        return console;
    }

    fn char_width(&self) -> usize {
        GLYPH_WIDTH * self.scale
    }

    fn char_height(&self) -> usize {
        GLYPH_HEIGHT * self.scale
    }

    fn line_height(&self) -> usize {
        (GLYPH_HEIGHT + LINE_SPACING) * self.scale
    }

    pub fn print(&mut self, text: &str) {
        for c in text.chars() {
            self.print_char(c);
        }
        self.display.present();
    }

    fn new_line(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += self.line_height();

        if self.cursor_y + self.char_height() > self.display.height() {
            self.scroll_up();
        }
    }

    fn print_char(&mut self, c: char) {
        if c == '\n' {
            self.new_line();
            return;
        }

        if self.cursor_x + self.char_width() > self.display.width() {
            self.new_line();
        }

        self.clear_cell();
        self.display.draw_char(self.cursor_x as i32, self.cursor_y as i32, c, self.text_color, self.scale as i32);
        self.cursor_x += self.char_width();
    }

    fn clear_cell(&mut self) {
        let cell = Rect::new(self.cursor_x as i32, self.cursor_y as i32, self.char_width() as i32, self.char_height() as i32);
        self.display.fill_rect(cell, BACKGROUND_COLOR);
    }

    pub fn clear(&mut self) {
        self.display.clear(BACKGROUND_COLOR);
        self.display.present();
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    pub fn backspace(&mut self) {
        if self.cursor_x < self.char_width() {
            return;
        }

        self.cursor_x -= self.char_width();
        self.clear_cell();
        self.display.present();
    }

    fn scroll_up(&mut self) {
        let scroll_distance = self.line_height();
        self.display.scroll_up(scroll_distance, BACKGROUND_COLOR);
        self.cursor_y = self.cursor_y.saturating_sub(scroll_distance);
    }
}
