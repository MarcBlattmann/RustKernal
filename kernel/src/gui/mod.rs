//! GUI Module - Clean, modular graphical user interface
//!
//! Architecture:
//! - `cursor`: Hardware-style cursor with save/restore
//! - `window`: Window management and rendering
//! - `desktop`: Desktop environment and event handling
//! - `widgets`: Reusable UI components
//! - `app`: Declarative app builder system (HTML-like)
//! - `script`: PursuitScript interpreter for app logic
//! - `builtin_apps`: Native apps (Editor, Explorer, Terminal, Docs)

pub mod cursor;
pub mod window;
pub mod desktop;
pub mod widgets;
pub mod theme;
pub mod app;
pub mod pa_parser;
pub mod layout;
pub mod start_menu;
pub mod script;
pub mod builtin_apps;

use crate::drivers::display::screen::Screen;
use crate::drivers::{mouse, keyboard};

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
        
        // Poll keyboard input - route to active window (process ONE char per frame)
        // The window manager will add dirty rects automatically when handling keyboard input
        let ctrl = keyboard::is_ctrl_pressed();
        
        if let Some(c) = keyboard::try_read_char() {
            let char_code = c as u32;
            if char_code >= 32 && char_code < 127 {
                // Printable character - pass it through
                desktop.handle_keyboard_input(c, ctrl);
            } else if c == '\n' || c == '\r' || c == '\x08' {
                // Control character - pass it through
                desktop.handle_keyboard_input(c, ctrl);
            }
        } else if ctrl {
            // No char in buffer but Ctrl is pressed - check for Ctrl+letter combos
            // by polling the raw scancode
            if let Some(key) = keyboard::try_read_ctrl_combo() {
                desktop.handle_keyboard_input(key, true);
            }
        }
        
        // Poll special keys (arrows, function keys, etc.)
        if let Some(special) = keyboard::try_read_special_key() {
            desktop.handle_special_key_input(special);
        }
        
        // Get mouse state
        let (mx, my) = mouse::get_position();
        let (left_pressed, right_pressed, _) = mouse::get_buttons();
        
        // Check dragging state BEFORE handling input
        let was_dragging = desktop.is_dragging();
        
        // Update cursor type based on what we're hovering over
        let new_cursor_type = if desktop.is_over_resize_handle(mx, my) && !desktop.is_dragging() {
            cursor::CursorType::ResizeNWSE
        } else {
            cursor::CursorType::Arrow
        };
        
        let cursor_changed = new_cursor_type != cursor::get_type();
        if cursor_changed {
            cursor::set_type(new_cursor_type);
        }
        
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
                let dirty_regions = desktop.take_dirty_regions();
                if dirty_regions.is_empty() {
                    // Fallback to full redraw if no dirty regions tracked
                    desktop.render(screen);
                } else {
                    // Smart redraw: ContentOnly for keyboard, FullWindow for resize/move
                    desktop.render_dirty_regions(screen, &dirty_regions);
                }
            }
            cursor::draw_at(screen, mx, my);
        } else if desktop.is_dragging() {
            // Still dragging - don't draw cursor
        } else {
            // Normal operation - not dragging
            
            // Process any pending actions (e.g., opening files from file explorer)
            desktop.process_pending_actions();
            
            // Always check for dirty regions first (keyboard, mouse, etc may have added them)
            let dirty_regions = desktop.take_dirty_regions();
            
            if !dirty_regions.is_empty() {
                // Smart redraw using DirtyRegion types:
                // - ContentOnly: Just re-render content area (efficient for typing)
                // - FullWindow: Re-render entire window (for resize/move)
                // - Rect: Arbitrary area (for closed windows, etc.)
                desktop.render_dirty_regions(screen, &dirty_regions);
            } else if needs_redraw || cursor_changed {
                // No dirty regions but something changed - full redraw
                desktop.render(screen);
            }
            cursor::show_at(screen, mx, my);
        }
        
        // Small delay to prevent CPU spinning (reduced for better responsiveness)
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }
}
