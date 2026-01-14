//! Cursor Module - Clean cursor management with save/restore
//!
//! Features:
//! - Saves background before drawing cursor
//! - Restores background when cursor moves
//! - Smooth, flicker-free movement

use crate::drivers::display::screen::Screen;
use super::theme::COLOR_CURSOR;

/// Cursor dimensions
const CURSOR_WIDTH: usize = 12;
const CURSOR_HEIGHT: usize = 19;

/// Cursor bitmap (1 = white, 0 = transparent)
const CURSOR_SHAPE: [[u8; CURSOR_WIDTH]; CURSOR_HEIGHT] = [
    [1,0,0,0,0,0,0,0,0,0,0,0],
    [1,1,0,0,0,0,0,0,0,0,0,0],
    [1,1,1,0,0,0,0,0,0,0,0,0],
    [1,1,1,1,0,0,0,0,0,0,0,0],
    [1,1,1,1,1,0,0,0,0,0,0,0],
    [1,1,1,1,1,1,0,0,0,0,0,0],
    [1,1,1,1,1,1,1,0,0,0,0,0],
    [1,1,1,1,1,1,1,1,0,0,0,0],
    [1,1,1,1,1,1,1,1,1,0,0,0],
    [1,1,1,1,1,1,1,1,1,1,0,0],
    [1,1,1,1,1,1,1,1,1,1,1,0],
    [1,1,1,1,1,1,1,1,1,1,1,1],
    [1,1,1,1,1,1,1,0,0,0,0,0],
    [1,1,1,0,1,1,1,1,0,0,0,0],
    [1,1,0,0,1,1,1,1,0,0,0,0],
    [1,0,0,0,0,1,1,1,1,0,0,0],
    [0,0,0,0,0,1,1,1,1,0,0,0],
    [0,0,0,0,0,0,1,1,1,1,0,0],
    [0,0,0,0,0,0,1,1,1,1,0,0],
];

/// Saved background under cursor
static mut SAVED_BG: [[u32; CURSOR_WIDTH]; CURSOR_HEIGHT] = [[0; CURSOR_WIDTH]; CURSOR_HEIGHT];

/// Last drawn cursor position (where the saved background is from)
static mut DRAWN_X: i32 = -100;
static mut DRAWN_Y: i32 = -100;

/// Screen bounds
static mut SCREEN_W: usize = 0;
static mut SCREEN_H: usize = 0;

/// Whether cursor is currently visible on screen
static mut CURSOR_VISIBLE: bool = false;

/// Initialize cursor system
pub fn init(screen_width: usize, screen_height: usize) {
    unsafe {
        SCREEN_W = screen_width;
        SCREEN_H = screen_height;
        DRAWN_X = -100;
        DRAWN_Y = -100;
        CURSOR_VISIBLE = false;
    }
}

/// Save background at given position
fn save_background_at(screen: &Screen, x: i32, y: i32) {
    unsafe {
        for row in 0..CURSOR_HEIGHT {
            for col in 0..CURSOR_WIDTH {
                let px = x as usize + col;
                let py = y as usize + row;
                
                if px < SCREEN_W && py < SCREEN_H {
                    SAVED_BG[row][col] = screen.read_pixel(px, py);
                } else {
                    SAVED_BG[row][col] = 0;
                }
            }
        }
    }
}

/// Restore background at the last drawn position
fn restore_background(screen: &mut Screen) {
    unsafe {
        let old_x = DRAWN_X;
        let old_y = DRAWN_Y;
        
        for row in 0..CURSOR_HEIGHT {
            for col in 0..CURSOR_WIDTH {
                if CURSOR_SHAPE[row][col] == 1 {
                    let px = old_x as usize + col;
                    let py = old_y as usize + row;
                    
                    if px < SCREEN_W && py < SCREEN_H {
                        screen.write_pixel(px, py, SAVED_BG[row][col]);
                    }
                }
            }
        }
    }
}

/// Draw cursor at given position
fn draw_cursor_at(screen: &mut Screen, x: i32, y: i32) {
    unsafe {
        for row in 0..CURSOR_HEIGHT {
            for col in 0..CURSOR_WIDTH {
                if CURSOR_SHAPE[row][col] == 1 {
                    let px = x as usize + col;
                    let py = y as usize + row;
                    
                    if px < SCREEN_W && py < SCREEN_H {
                        screen.write_pixel(px, py, COLOR_CURSOR);
                    }
                }
            }
        }
        DRAWN_X = x;
        DRAWN_Y = y;
        CURSOR_VISIBLE = true;
    }
}

/// Draw cursor at position (initial draw)
pub fn draw_at(screen: &mut Screen, x: i32, y: i32) {
    save_background_at(screen, x, y);
    draw_cursor_at(screen, x, y);
}

/// Hide cursor (restore background)
pub fn hide(screen: &mut Screen) {
    unsafe {
        if CURSOR_VISIBLE {
            restore_background(screen);
            CURSOR_VISIBLE = false;
        }
    }
}

/// Show cursor at position (after hide)
pub fn show_at(screen: &mut Screen, x: i32, y: i32) {
    save_background_at(screen, x, y);
    draw_cursor_at(screen, x, y);
}

/// Move cursor to new position smoothly
/// Returns true if cursor was moved
pub fn move_to(screen: &mut Screen, new_x: i32, new_y: i32) -> bool {
    unsafe {
        // Check if position actually changed
        if CURSOR_VISIBLE && new_x == DRAWN_X && new_y == DRAWN_Y {
            return false;
        }
        
        // Restore old background if visible
        if CURSOR_VISIBLE {
            restore_background(screen);
        }
        
        // Save new background and draw
        save_background_at(screen, new_x, new_y);
        draw_cursor_at(screen, new_x, new_y);
        
        true
    }
}
