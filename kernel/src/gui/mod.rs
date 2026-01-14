//! GUI Module - Clean, modular graphical user interface
//!
//! Architecture:
//! - `cursor`: Hardware-style cursor with save/restore
//! - `window`: Window management and rendering
//! - `desktop`: Desktop environment and event handling
//! - `widgets`: Reusable UI components
//! - `app`: Declarative app builder system (HTML-like)

pub mod cursor;
pub mod window;
pub mod desktop;
pub mod widgets;
pub mod theme;
pub mod app;

use crate::drivers::display::screen::Screen;
use crate::drivers::mouse;

/// Main GUI entry point
pub fn run_gui(screen: &mut Screen) {
    // Initialize mouse
    mouse::init(screen.width() as u32, screen.height() as u32);
    
    // Initialize cursor subsystem
    cursor::init(screen.width(), screen.height());
    
    let mut desktop = desktop::Desktop::new(screen.width(), screen.height());
    
    // Initial render
    desktop.render(screen);
    
    // Get initial mouse position and draw cursor there
    let (mx, my) = mouse::get_position();
    cursor::draw_at(screen, mx, my);
    
    // Main loop
    loop {
        // Poll mouse input multiple times for responsiveness
        for _ in 0..5 {
            mouse::poll();
        }
        
        // Get mouse state
        let (mx, my) = mouse::get_position();
        let (left_pressed, _, _) = mouse::get_buttons();
        
        // Check dragging state BEFORE handling input
        let was_dragging = desktop.is_dragging();
        
        // Always hide cursor before any screen operations
        cursor::hide(screen);
        
        // Handle input - returns (needs_redraw, drag_just_started)
        let (needs_redraw, drag_started) = desktop.handle_input(screen, mx, my, left_pressed);
        
        // If drag just started, draw the initial XOR outline now (cursor is hidden)
        if drag_started {
            desktop.draw_initial_outline(screen);
            // Don't draw cursor during drag
        } else if was_dragging && !desktop.is_dragging() {
            // Drag just ended - do partial redraw if possible
            if needs_redraw {
                // Get dirty regions and do efficient partial redraw
                let dirty_rects = desktop.take_dirty_rects();
                if dirty_rects.is_empty() {
                    // Fallback to full redraw if no dirty rects tracked
                    desktop.render(screen);
                } else {
                    // Clear and redraw only dirty regions
                    desktop.render_dirty(screen, &dirty_rects);
                }
            }
            cursor::draw_at(screen, mx, my);
        } else if desktop.is_dragging() {
            // Still dragging - don't draw cursor
        } else {
            // Normal operation - not dragging
            if needs_redraw {
                // Get dirty regions for partial redraw
                let dirty_rects = desktop.take_dirty_rects();
                if dirty_rects.is_empty() {
                    desktop.render(screen);
                } else {
                    desktop.render_dirty(screen, &dirty_rects);
                }
            }
            cursor::show_at(screen, mx, my);
        }
        
        // Small delay to prevent CPU spinning
        for _ in 0..3000 {
            core::hint::spin_loop();
        }
    }
}
