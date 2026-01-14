//! Widgets Module - Reusable UI components

use crate::drivers::display::screen::Screen;
use super::theme::*;

/// Rectangle structure for bounds
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: usize, height: usize) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if point is inside rectangle
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x 
            && px < self.x + self.width as i32
            && py >= self.y 
            && py < self.y + self.height as i32
    }
    
    /// Get right edge
    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }
    
    /// Get bottom edge
    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
    
    /// Check if this rectangle intersects with another
    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }
    
    /// Create a union of two rectangles (bounding box)
    pub fn union(&self, other: &Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Rect::new(x, y, (right - x) as usize, (bottom - y) as usize)
    }
}

/// Draw a filled rectangle
pub fn draw_filled_rect(screen: &mut Screen, rect: &Rect, color: u32) {
    for py in 0..rect.height {
        for px in 0..rect.width {
            let x = rect.x as usize + px;
            let y = rect.y as usize + py;
            if x < screen.width() && y < screen.height() {
                screen.write_pixel(x, y, color);
            }
        }
    }
}

/// Draw a rectangle border
pub fn draw_rect_border(screen: &mut Screen, rect: &Rect, color: u32, thickness: usize) {
    // Top border
    for t in 0..thickness {
        for px in 0..rect.width {
            let x = rect.x as usize + px;
            let y = rect.y as usize + t;
            if x < screen.width() && y < screen.height() {
                screen.write_pixel(x, y, color);
            }
        }
    }
    
    // Bottom border
    for t in 0..thickness {
        for px in 0..rect.width {
            let x = rect.x as usize + px;
            let y = rect.y as usize + rect.height - 1 - t;
            if x < screen.width() && y < screen.height() {
                screen.write_pixel(x, y, color);
            }
        }
    }
    
    // Left border
    for t in 0..thickness {
        for py in 0..rect.height {
            let x = rect.x as usize + t;
            let y = rect.y as usize + py;
            if x < screen.width() && y < screen.height() {
                screen.write_pixel(x, y, color);
            }
        }
    }
    
    // Right border
    for t in 0..thickness {
        for py in 0..rect.height {
            let x = rect.x as usize + rect.width - 1 - t;
            let y = rect.y as usize + py;
            if x < screen.width() && y < screen.height() {
                screen.write_pixel(x, y, color);
            }
        }
    }
}

/// Draw XOR rectangle outline (for drag preview)
/// Drawing twice at the same position erases it (XOR property)
pub fn draw_xor_outline(screen: &mut Screen, rect: &Rect) {
    let sw = screen.width();
    let sh = screen.height();
    
    // Top edge
    for px in 0..rect.width {
        let x = (rect.x + px as i32) as usize;
        let y = rect.y as usize;
        if x < sw && y < sh && rect.y >= 0 {
            let old = screen.read_pixel(x, y);
            screen.write_pixel(x, y, old ^ 0x00FFFFFF);
        }
    }
    
    // Bottom edge
    let bottom_y = rect.y + rect.height as i32 - 1;
    if rect.height > 1 && bottom_y >= 0 {
        for px in 0..rect.width {
            let x = (rect.x + px as i32) as usize;
            let y = bottom_y as usize;
            if x < sw && y < sh {
                let old = screen.read_pixel(x, y);
                screen.write_pixel(x, y, old ^ 0x00FFFFFF);
            }
        }
    }
    
    // Left edge (excluding corners)
    for py in 1..rect.height.saturating_sub(1) {
        let x = rect.x as usize;
        let y = (rect.y + py as i32) as usize;
        if x < sw && y < sh && rect.x >= 0 {
            let old = screen.read_pixel(x, y);
            screen.write_pixel(x, y, old ^ 0x00FFFFFF);
        }
    }
    
    // Right edge (excluding corners)
    let right_x = rect.x + rect.width as i32 - 1;
    if rect.width > 1 && right_x >= 0 {
        for py in 1..rect.height.saturating_sub(1) {
            let x = right_x as usize;
            let y = (rect.y + py as i32) as usize;
            if x < sw && y < sh {
                let old = screen.read_pixel(x, y);
                screen.write_pixel(x, y, old ^ 0x00FFFFFF);
            }
        }
    }
}

/// Simple 8x8 font for basic characters
const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 8;

/// Get font bitmap for a character
fn get_char_bitmap(c: char) -> [u8; 8] {
    match c {
        'A' => [0x18, 0x3C, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x00],
        'B' => [0x7C, 0x66, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'C' => [0x3C, 0x66, 0x60, 0x60, 0x60, 0x66, 0x3C, 0x00],
        'D' => [0x78, 0x6C, 0x66, 0x66, 0x66, 0x6C, 0x78, 0x00],
        'E' => [0x7E, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'F' => [0x7E, 0x60, 0x7C, 0x60, 0x60, 0x60, 0x60, 0x00],
        'G' => [0x3C, 0x66, 0x60, 0x6E, 0x66, 0x66, 0x3C, 0x00],
        'H' => [0x66, 0x66, 0x7E, 0x66, 0x66, 0x66, 0x66, 0x00],
        'I' => [0x3C, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'J' => [0x1E, 0x0C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38, 0x00],
        'K' => [0x66, 0x6C, 0x78, 0x70, 0x78, 0x6C, 0x66, 0x00],
        'L' => [0x60, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7E, 0x00],
        'M' => [0x63, 0x77, 0x7F, 0x6B, 0x63, 0x63, 0x63, 0x00],
        'N' => [0x66, 0x76, 0x7E, 0x7E, 0x6E, 0x66, 0x66, 0x00],
        'O' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'P' => [0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60, 0x60, 0x00],
        'Q' => [0x3C, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x0E, 0x00],
        'R' => [0x7C, 0x66, 0x66, 0x7C, 0x78, 0x6C, 0x66, 0x00],
        'S' => [0x3C, 0x66, 0x60, 0x3C, 0x06, 0x66, 0x3C, 0x00],
        'T' => [0x7E, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00],
        'U' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'V' => [0x66, 0x66, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'W' => [0x63, 0x63, 0x63, 0x6B, 0x7F, 0x77, 0x63, 0x00],
        'X' => [0x66, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x66, 0x00],
        'Y' => [0x66, 0x66, 0x66, 0x3C, 0x18, 0x18, 0x18, 0x00],
        'Z' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x7E, 0x00],
        'a' => [0x00, 0x00, 0x3C, 0x06, 0x3E, 0x66, 0x3E, 0x00],
        'b' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x7C, 0x00],
        'c' => [0x00, 0x00, 0x3C, 0x60, 0x60, 0x60, 0x3C, 0x00],
        'd' => [0x06, 0x06, 0x3E, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'e' => [0x00, 0x00, 0x3C, 0x66, 0x7E, 0x60, 0x3C, 0x00],
        'f' => [0x1C, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x30, 0x00],
        'g' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'h' => [0x60, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'i' => [0x18, 0x00, 0x38, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'j' => [0x0C, 0x00, 0x0C, 0x0C, 0x0C, 0x0C, 0x6C, 0x38],
        'k' => [0x60, 0x60, 0x66, 0x6C, 0x78, 0x6C, 0x66, 0x00],
        'l' => [0x38, 0x18, 0x18, 0x18, 0x18, 0x18, 0x3C, 0x00],
        'm' => [0x00, 0x00, 0x66, 0x7F, 0x7F, 0x6B, 0x63, 0x00],
        'n' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x66, 0x66, 0x00],
        'o' => [0x00, 0x00, 0x3C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        'p' => [0x00, 0x00, 0x7C, 0x66, 0x66, 0x7C, 0x60, 0x60],
        'q' => [0x00, 0x00, 0x3E, 0x66, 0x66, 0x3E, 0x06, 0x06],
        'r' => [0x00, 0x00, 0x7C, 0x66, 0x60, 0x60, 0x60, 0x00],
        's' => [0x00, 0x00, 0x3E, 0x60, 0x3C, 0x06, 0x7C, 0x00],
        't' => [0x30, 0x30, 0x7C, 0x30, 0x30, 0x30, 0x1C, 0x00],
        'u' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x66, 0x3E, 0x00],
        'v' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3C, 0x18, 0x00],
        'w' => [0x00, 0x00, 0x63, 0x6B, 0x7F, 0x7F, 0x36, 0x00],
        'x' => [0x00, 0x00, 0x66, 0x3C, 0x18, 0x3C, 0x66, 0x00],
        'y' => [0x00, 0x00, 0x66, 0x66, 0x66, 0x3E, 0x06, 0x3C],
        'z' => [0x00, 0x00, 0x7E, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '0' => [0x3C, 0x66, 0x6E, 0x76, 0x66, 0x66, 0x3C, 0x00],
        '1' => [0x18, 0x38, 0x18, 0x18, 0x18, 0x18, 0x7E, 0x00],
        '2' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x30, 0x7E, 0x00],
        '3' => [0x3C, 0x66, 0x06, 0x1C, 0x06, 0x66, 0x3C, 0x00],
        '4' => [0x0C, 0x1C, 0x3C, 0x6C, 0x7E, 0x0C, 0x0C, 0x00],
        '5' => [0x7E, 0x60, 0x7C, 0x06, 0x06, 0x66, 0x3C, 0x00],
        '6' => [0x3C, 0x60, 0x7C, 0x66, 0x66, 0x66, 0x3C, 0x00],
        '7' => [0x7E, 0x06, 0x0C, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x3C, 0x66, 0x66, 0x3C, 0x66, 0x66, 0x3C, 0x00],
        '9' => [0x3C, 0x66, 0x66, 0x3E, 0x06, 0x66, 0x3C, 0x00],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30],
        ':' => [0x00, 0x18, 0x18, 0x00, 0x00, 0x18, 0x18, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x7E, 0x00, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xFF],
        '/' => [0x02, 0x06, 0x0C, 0x18, 0x30, 0x60, 0x40, 0x00],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00],
        '?' => [0x3C, 0x66, 0x06, 0x0C, 0x18, 0x00, 0x18, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    }
}

/// Draw a single character
pub fn draw_char(screen: &mut Screen, x: usize, y: usize, c: char, color: u32) {
    let bitmap = get_char_bitmap(c);
    
    for row in 0..FONT_HEIGHT {
        for col in 0..FONT_WIDTH {
            if (bitmap[row] >> (7 - col)) & 1 == 1 {
                let px = x + col;
                let py = y + row;
                if px < screen.width() && py < screen.height() {
                    screen.write_pixel(px, py, color);
                }
            }
        }
    }
}

/// Draw text string
pub fn draw_text(screen: &mut Screen, x: usize, y: usize, text: &str, color: u32) {
    let mut cx = x;
    for c in text.chars() {
        draw_char(screen, cx, y, c, color);
        cx += FONT_WIDTH;
    }
}

/// Draw a button (rectangle with border and centered text)
pub fn draw_button(screen: &mut Screen, rect: &Rect, text: &str) {
    // Background
    draw_filled_rect(screen, rect, COLOR_BUTTON_BG);
    
    // Border
    draw_rect_border(screen, rect, COLOR_BUTTON_BORDER, 1);
    
    // Centered text
    let text_width = text.len() * FONT_WIDTH;
    let text_x = rect.x as usize + (rect.width.saturating_sub(text_width)) / 2;
    let text_y = rect.y as usize + (rect.height.saturating_sub(FONT_HEIGHT)) / 2;
    draw_text(screen, text_x, text_y, text, COLOR_BUTTON_TEXT);
}

/// Draw X button (close button)
pub fn draw_close_button(screen: &mut Screen, x: usize, y: usize, size: usize) {
    let rect = Rect::new(x as i32, y as i32, size, size);
    draw_filled_rect(screen, &rect, COLOR_BUTTON_BG);
    draw_rect_border(screen, &rect, COLOR_BUTTON_BORDER, 1);
    
    // Draw X
    let padding = 3;
    for i in 0..(size - padding * 2) {
        let px1 = x + padding + i;
        let py1 = y + padding + i;
        let px2 = x + size - padding - 1 - i;
        let py2 = y + padding + i;
        
        if px1 < screen.width() && py1 < screen.height() {
            screen.write_pixel(px1, py1, COLOR_BUTTON_TEXT);
        }
        if px2 < screen.width() && py2 < screen.height() {
            screen.write_pixel(px2, py2, COLOR_BUTTON_TEXT);
        }
    }
}
