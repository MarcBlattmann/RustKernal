//! Window Module - Clean window management
//!
//! Features:
//! - Window structure with title bar
//! - Dragging support with XOR outline
//! - Dirty rectangle tracking for efficient redraws

use alloc::string::String;
use crate::drivers::display::screen::Screen;
use super::theme::*;
use super::widgets::{Rect, draw_filled_rect, draw_rect_border, draw_text, draw_close_button, draw_xor_outline};
use super::app::{AppDef, Element};

/// Maximum number of windows
const MAX_WINDOWS: usize = 8;

/// Window structure
pub struct Window {
    pub id: usize,
    pub title: String,
    pub bounds: Rect,
    pub visible: bool,
    pub elements: alloc::vec::Vec<Element>,
}

impl Window {
    pub fn new(id: usize, title: &str, x: i32, y: i32, width: usize, height: usize) -> Self {
        Self {
            id,
            title: String::from(title),
            bounds: Rect::new(x, y, width, height),
            visible: true,
            elements: alloc::vec::Vec::new(),
        }
    }
    
    /// Create window from AppDef
    pub fn from_app(id: usize, app: &AppDef) -> Self {
        Self {
            id,
            title: app.title.clone(),
            bounds: Rect::new(app.x, app.y, app.width, app.height),
            visible: app.visible,
            elements: app.elements.clone(),
        }
    }
    
    /// Get title bar bounds
    pub fn titlebar_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            TITLEBAR_HEIGHT,
        )
    }
    
    /// Get close button bounds
    pub fn close_button_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x + self.bounds.width as i32 - BUTTON_SIZE as i32 - 4,
            self.bounds.y + 4,
            BUTTON_SIZE,
            BUTTON_SIZE,
        )
    }
    
    /// Get content area bounds
    pub fn content_bounds(&self) -> Rect {
        Rect::new(
            self.bounds.x + BORDER_WIDTH as i32,
            self.bounds.y + TITLEBAR_HEIGHT as i32,
            self.bounds.width - BORDER_WIDTH * 2,
            self.bounds.height - TITLEBAR_HEIGHT - BORDER_WIDTH,
        )
    }
    
    /// Render the window
    pub fn render(&self, screen: &mut Screen) {
        if !self.visible {
            return;
        }
        
        // Window background
        draw_filled_rect(screen, &self.bounds, COLOR_WINDOW_BG);
        
        // Window border
        draw_rect_border(screen, &self.bounds, COLOR_WINDOW_BORDER, BORDER_WIDTH);
        
        // Title bar separator
        let sep_y = self.bounds.y as usize + TITLEBAR_HEIGHT;
        for x in 0..self.bounds.width {
            let px = self.bounds.x as usize + x;
            if px < screen.width() && sep_y < screen.height() {
                screen.write_pixel(px, sep_y, COLOR_WINDOW_BORDER);
            }
        }
        
        // Title text
        let title_x = self.bounds.x as usize + 8;
        let title_y = self.bounds.y as usize + (TITLEBAR_HEIGHT - 8) / 2;
        draw_text(screen, title_x, title_y, &self.title, COLOR_TITLEBAR_TEXT);
        
        // Close button
        let close_bounds = self.close_button_bounds();
        draw_close_button(screen, close_bounds.x as usize, close_bounds.y as usize, BUTTON_SIZE);
        
        // Render elements
        let content = self.content_bounds();
        for elem in &self.elements {
            match elem {
                Element::Label { text, x, y } => {
                    let px = content.x as usize + *x as usize;
                    let py = content.y as usize + *y as usize;
                    draw_text(screen, px, py, text, COLOR_FOREGROUND);
                }
                Element::Button { text, x, y, width, height } => {
                    let rect = Rect::new(
                        content.x + *x,
                        content.y + *y,
                        *width,
                        *height,
                    );
                    draw_filled_rect(screen, &rect, COLOR_BUTTON_BG);
                    draw_rect_border(screen, &rect, COLOR_BUTTON_BORDER, 1);
                    let tx = rect.x as usize + 4;
                    let ty = rect.y as usize + (*height - 8) / 2;
                    draw_text(screen, tx, ty, text, COLOR_BUTTON_TEXT);
                }
                Element::Panel { x, y, width, height, color } => {
                    let rect = Rect::new(content.x + *x, content.y + *y, *width, *height);
                    draw_filled_rect(screen, &rect, *color);
                }
                Element::TextBox { x, y, width, height } => {
                    let rect = Rect::new(content.x + *x, content.y + *y, *width, *height);
                    draw_filled_rect(screen, &rect, 0xFF222222);
                    draw_rect_border(screen, &rect, COLOR_FOREGROUND, 1);
                }
            }
        }
    }
}

/// Drag state
struct DragState {
    active: bool,
    window_id: usize,
    offset_x: i32,
    offset_y: i32,
    /// Current outline position during drag
    outline_rect: Option<Rect>,
    /// Original window position before drag
    original_bounds: Option<Rect>,
}

/// Window manager
pub struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    window_count: usize,
    drag: DragState,
    last_mouse_down: bool,
    /// Dirty regions to redraw
    dirty_rects: alloc::vec::Vec<Rect>,
}

impl WindowManager {
    pub fn new() -> Self {
        const NONE: Option<Window> = None;
        Self {
            windows: [NONE; MAX_WINDOWS],
            window_count: 0,
            drag: DragState {
                active: false,
                window_id: 0,
                offset_x: 0,
                offset_y: 0,
                outline_rect: None,
                original_bounds: None,
            },
            last_mouse_down: false,
            dirty_rects: alloc::vec::Vec::new(),
        }
    }
    
    /// Add a window from AppDef
    pub fn add_app(&mut self, app: &AppDef) -> Option<usize> {
        if self.window_count >= MAX_WINDOWS {
            return None;
        }
        
        let id = self.window_count;
        self.windows[id] = Some(Window::from_app(id, app));
        self.window_count += 1;
        Some(id)
    }
    
    /// Add a simple window
    pub fn add_window(&mut self, title: &str, x: i32, y: i32, width: usize, height: usize) -> Option<usize> {
        if self.window_count >= MAX_WINDOWS {
            return None;
        }
        
        let id = self.window_count;
        self.windows[id] = Some(Window::new(id, title, x, y, width, height));
        self.window_count += 1;
        Some(id)
    }
    
    /// Handle mouse input with XOR outline dragging
    /// Returns: (needs_partial_redraw, drag_just_started)
    pub fn handle_input(&mut self, screen: &mut Screen, mx: i32, my: i32, mouse_down: bool) -> (bool, bool) {
        let mouse_pressed = mouse_down && !self.last_mouse_down;
        let mouse_released = !mouse_down && self.last_mouse_down;
        self.last_mouse_down = mouse_down;
        
        // Handle drag end - erase outline, move window, mark dirty areas
        if mouse_released && self.drag.active {
            // Erase the XOR outline by drawing it again
            if let Some(outline) = self.drag.outline_rect.take() {
                draw_xor_outline(screen, &outline);
            }
            
            // Track old position for dirty rect
            if let Some(original) = self.drag.original_bounds.take() {
                self.dirty_rects.push(original);
            }
            
            // Move window to final position
            if let Some(window) = &mut self.windows[self.drag.window_id] {
                let new_x = mx - self.drag.offset_x;
                let new_y = my - self.drag.offset_y;
                window.bounds.x = new_x;
                window.bounds.y = new_y;
                // New position is also dirty
                self.dirty_rects.push(window.bounds);
            }
            
            self.drag.active = false;
            return (true, false); // Need redraw of dirty areas
        }
        
        // Handle dragging - XOR erase old, XOR draw new (no redraw needed!)
        if self.drag.active && mouse_down {
            if let Some(window) = &self.windows[self.drag.window_id] {
                let new_x = mx - self.drag.offset_x;
                let new_y = my - self.drag.offset_y;
                let new_rect = Rect::new(new_x, new_y, window.bounds.width, window.bounds.height);
                
                // Only update if position changed
                if self.drag.outline_rect != Some(new_rect) {
                    // Erase old outline (XOR again = restore original)
                    if let Some(old_rect) = self.drag.outline_rect {
                        draw_xor_outline(screen, &old_rect);
                    }
                    
                    // Draw new outline
                    draw_xor_outline(screen, &new_rect);
                    self.drag.outline_rect = Some(new_rect);
                }
            }
            return (false, false); // No redraw needed during drag
        }
        
        // Check for new interactions
        if mouse_pressed {
            // Check windows in reverse order (top to bottom)
            for i in (0..self.window_count).rev() {
                if let Some(window) = &self.windows[i] {
                    if !window.visible {
                        continue;
                    }
                    
                    // Check close button
                    let close_bounds = window.close_button_bounds();
                    if close_bounds.contains(mx, my) {
                        // Mark window area dirty
                        let bounds = window.bounds;
                        self.dirty_rects.push(bounds);
                        if let Some(w) = &mut self.windows[i] {
                            w.visible = false;
                        }
                        return (true, false); // Need redraw
                    }
                    
                    // Check titlebar for drag start
                    let titlebar = window.titlebar_bounds();
                    if titlebar.contains(mx, my) {
                        self.drag.active = true;
                        self.drag.window_id = i;
                        self.drag.offset_x = mx - window.bounds.x;
                        self.drag.offset_y = my - window.bounds.y;
                        self.drag.outline_rect = Some(window.bounds);
                        self.drag.original_bounds = Some(window.bounds); // Save original
                        
                        // DON'T draw outline here - return flag so caller can hide cursor first
                        return (false, true); // drag_just_started = true
                    }
                    
                    // Click inside window
                    if window.bounds.contains(mx, my) {
                        return (false, false);
                    }
                }
            }
        }
        
        (false, false)
    }
    
    /// Draw the initial XOR outline (called after cursor is hidden)
    pub fn draw_initial_outline(&mut self, screen: &mut Screen) {
        if let Some(rect) = self.drag.outline_rect {
            draw_xor_outline(screen, &rect);
        }
    }
    
    /// Render all windows
    pub fn render(&self, screen: &mut Screen) {
        for i in 0..self.window_count {
            if let Some(window) = &self.windows[i] {
                window.render(screen);
            }
        }
    }
    
    /// Take accumulated dirty rects and clear them
    pub fn take_dirty_rects(&mut self) -> alloc::vec::Vec<Rect> {
        core::mem::take(&mut self.dirty_rects)
    }
    
    /// Check if there are pending dirty rects
    pub fn has_dirty_rects(&self) -> bool {
        !self.dirty_rects.is_empty()
    }
    
    /// Render only windows that intersect with dirty regions
    /// This is more efficient than full render for partial updates
    pub fn render_dirty(&self, screen: &mut Screen, dirty: &[Rect]) {
        for i in 0..self.window_count {
            if let Some(window) = &self.windows[i] {
                if !window.visible {
                    continue;
                }
                // Check if window intersects any dirty rect
                for d in dirty {
                    if window.bounds.intersects(d) {
                        window.render(screen);
                        break; // Only render once
                    }
                }
            }
        }
    }
    
    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool {
        self.drag.active
    }
}
