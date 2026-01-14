//! Window Module - Clean window management
//!
//! Features:
//! - Window structure with title bar
//! - Dragging support with XOR outline
//! - Dirty rectangle tracking for efficient redraws

use alloc::string::String;
use crate::drivers::display::screen::Screen;
use super::theme::*;
use super::widgets::{Rect, draw_filled_rect, draw_filled_rect_clipped, draw_rect_border, draw_rect_border_clipped, draw_text, draw_text_clipped, draw_close_button, draw_xor_outline};
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
    /// Original size when created (for scaling elements)
    pub original_width: usize,
    pub original_height: usize,
}

impl Window {
    pub fn new(id: usize, title: &str, x: i32, y: i32, width: usize, height: usize) -> Self {
        Self {
            id,
            title: String::from(title),
            bounds: Rect::new(x, y, width, height),
            visible: true,
            elements: alloc::vec::Vec::new(),
            original_width: width,
            original_height: height,
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
            original_width: app.width,
            original_height: app.height,
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
    
    /// Get resize handle bounds (bottom-right corner)
    pub fn resize_handle_bounds(&self) -> Rect {
        const HANDLE_SIZE: usize = 16;
        Rect::new(
            self.bounds.x + self.bounds.width as i32 - HANDLE_SIZE as i32,
            self.bounds.y + self.bounds.height as i32 - HANDLE_SIZE as i32,
            HANDLE_SIZE,
            HANDLE_SIZE,
        )
    }
    
    /// Minimum window size
    pub const MIN_WIDTH: usize = 100;
    pub const MIN_HEIGHT: usize = 80;
    
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
        
        // Calculate scale factors for responsive elements
        let original_content_w = self.original_width.saturating_sub(BORDER_WIDTH * 2);
        let original_content_h = self.original_height.saturating_sub(TITLEBAR_HEIGHT + BORDER_WIDTH);
        let content = self.content_bounds();
        let scale_x = if original_content_w > 0 { 
            content.width as f32 / original_content_w as f32 
        } else { 1.0 };
        let scale_y = if original_content_h > 0 { 
            content.height as f32 / original_content_h as f32 
        } else { 1.0 };
        
        // Clip region is the content area - elements outside will be hidden
        let clip = &content;
        
        // Render elements with scaling and clipping
        for elem in &self.elements {
            match elem {
                Element::Label { text, x, y } => {
                    let px = content.x as usize + ((*x as f32) * scale_x) as usize;
                    let py = content.y as usize + ((*y as f32) * scale_y) as usize;
                    draw_text_clipped(screen, px, py, text, COLOR_FOREGROUND, clip);
                }
                Element::Button { text, x, y, width, height } => {
                    let scaled_x = ((*x as f32) * scale_x) as i32;
                    let scaled_y = ((*y as f32) * scale_y) as i32;
                    let scaled_w = ((*width as f32) * scale_x) as usize;
                    let scaled_h = ((*height as f32) * scale_y) as usize;
                    let rect = Rect::new(
                        content.x + scaled_x,
                        content.y + scaled_y,
                        scaled_w.max(20),
                        scaled_h.max(16),
                    );
                    draw_filled_rect_clipped(screen, &rect, COLOR_BUTTON_BG, clip);
                    draw_rect_border_clipped(screen, &rect, COLOR_BUTTON_BORDER, 1, clip);
                    let tx = rect.x as usize + 4;
                    let ty = rect.y as usize + (rect.height.saturating_sub(8)) / 2;
                    draw_text_clipped(screen, tx, ty, text, COLOR_BUTTON_TEXT, clip);
                }
                Element::Panel { x, y, width, height } => {
                    let scaled_x = ((*x as f32) * scale_x) as i32;
                    let scaled_y = ((*y as f32) * scale_y) as i32;
                    let scaled_w = ((*width as f32) * scale_x) as usize;
                    let scaled_h = ((*height as f32) * scale_y) as usize;
                    let rect = Rect::new(content.x + scaled_x, content.y + scaled_y, scaled_w.max(1), scaled_h.max(1));
                    draw_filled_rect_clipped(screen, &rect, 0xFF222222, clip);
                    draw_rect_border_clipped(screen, &rect, COLOR_WINDOW_BORDER, 1, clip);
                }
                Element::TextBox { x, y, width, height } => {
                    let scaled_x = ((*x as f32) * scale_x) as i32;
                    let scaled_y = ((*y as f32) * scale_y) as i32;
                    let scaled_w = ((*width as f32) * scale_x) as usize;
                    let scaled_h = ((*height as f32) * scale_y) as usize;
                    let rect = Rect::new(content.x + scaled_x, content.y + scaled_y, scaled_w.max(10), scaled_h.max(10));
                    draw_filled_rect_clipped(screen, &rect, 0xFF1A1A1A, clip);
                    draw_rect_border_clipped(screen, &rect, COLOR_FOREGROUND, 1, clip);
                }
            }
        }
    }
}

/// Interaction mode
#[derive(Clone, Copy, PartialEq)]
enum InteractionMode {
    None,
    Dragging,
    Resizing,
}

/// Drag/Resize state
struct DragState {
    mode: InteractionMode,
    window_id: usize,
    offset_x: i32,
    offset_y: i32,
    /// Current outline position during drag/resize
    outline_rect: Option<Rect>,
    /// Original window bounds before operation
    original_bounds: Option<Rect>,
}

/// Window manager
pub struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    window_count: usize,
    /// Z-order: indices into windows array, front (top) is last
    z_order: [usize; MAX_WINDOWS],
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
            z_order: [0; MAX_WINDOWS],
            drag: DragState {
                mode: InteractionMode::None,
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
    
    /// Bring a window to the front (top of z-order)
    fn bring_to_front(&mut self, window_idx: usize) {
        // Find where this window is in z_order
        let mut pos = None;
        for i in 0..self.window_count {
            if self.z_order[i] == window_idx {
                pos = Some(i);
                break;
            }
        }
        
        if let Some(p) = pos {
            // Shift everything after it down, put this at end
            for i in p..(self.window_count - 1) {
                self.z_order[i] = self.z_order[i + 1];
            }
            self.z_order[self.window_count - 1] = window_idx;
        }
    }
    
    /// Check if mouse is over any window's resize handle
    pub fn is_over_resize_handle(&self, mx: i32, my: i32) -> bool {
        for i in (0..self.window_count).rev() {
            let idx = self.z_order[i];
            if let Some(window) = &self.windows[idx] {
                if window.visible && window.resize_handle_bounds().contains(mx, my) {
                    return true;
                }
            }
        }
        false
    }
    
    /// Add a window from AppDef
    pub fn add_app(&mut self, app: &AppDef) -> Option<usize> {
        if self.window_count >= MAX_WINDOWS {
            return None;
        }
        
        let id = self.window_count;
        self.windows[id] = Some(Window::from_app(id, app));
        self.z_order[self.window_count] = id; // Add to top of z-order
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
        self.z_order[self.window_count] = id; // Add to top of z-order
        self.window_count += 1;
        Some(id)
    }
    
    /// Handle mouse input with XOR outline dragging
    /// Returns: (needs_partial_redraw, drag_just_started)
    pub fn handle_input(&mut self, screen: &mut Screen, mx: i32, my: i32, mouse_down: bool) -> (bool, bool) {
        let mouse_pressed = mouse_down && !self.last_mouse_down;
        let mouse_released = !mouse_down && self.last_mouse_down;
        self.last_mouse_down = mouse_down;
        
        // Handle operation end (drag or resize)
        if mouse_released && self.drag.mode != InteractionMode::None {
            // Erase the XOR outline by drawing it again
            if let Some(outline) = self.drag.outline_rect.take() {
                draw_xor_outline(screen, &outline);
            }
            
            // Track old position for dirty rect
            if let Some(original) = self.drag.original_bounds.take() {
                self.dirty_rects.push(original);
            }
            
            if let Some(window) = &mut self.windows[self.drag.window_id] {
                if self.drag.mode == InteractionMode::Dragging {
                    // Move window to final position
                    let new_x = mx - self.drag.offset_x;
                    let new_y = my - self.drag.offset_y;
                    window.bounds.x = new_x;
                    window.bounds.y = new_y;
                } else if self.drag.mode == InteractionMode::Resizing {
                    // Resize window
                    let new_w = (mx - window.bounds.x).max(Window::MIN_WIDTH as i32) as usize;
                    let new_h = (my - window.bounds.y).max(Window::MIN_HEIGHT as i32) as usize;
                    window.bounds.width = new_w;
                    window.bounds.height = new_h;
                }
                // New bounds are dirty
                self.dirty_rects.push(window.bounds);
            }
            
            self.drag.mode = InteractionMode::None;
            return (true, false); // Need redraw of dirty areas
        }
        
        // Handle dragging - XOR erase old, XOR draw new
        if self.drag.mode == InteractionMode::Dragging && mouse_down {
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
        
        // Handle resizing - XOR erase old, XOR draw new
        if self.drag.mode == InteractionMode::Resizing && mouse_down {
            if let Some(window) = &self.windows[self.drag.window_id] {
                let new_w = (mx - window.bounds.x).max(Window::MIN_WIDTH as i32) as usize;
                let new_h = (my - window.bounds.y).max(Window::MIN_HEIGHT as i32) as usize;
                let new_rect = Rect::new(window.bounds.x, window.bounds.y, new_w, new_h);
                
                // Only update if size changed
                if self.drag.outline_rect != Some(new_rect) {
                    // Erase old outline
                    if let Some(old_rect) = self.drag.outline_rect {
                        draw_xor_outline(screen, &old_rect);
                    }
                    
                    // Draw new outline
                    draw_xor_outline(screen, &new_rect);
                    self.drag.outline_rect = Some(new_rect);
                }
            }
            return (false, false); // No redraw needed during resize
        }
        
        // Check for new interactions
        if mouse_pressed {
            // Check windows in reverse z-order (top to bottom)
            for zi in (0..self.window_count).rev() {
                let i = self.z_order[zi];
                
                // Get window data we need (to avoid borrowing issues)
                let (visible, close_bounds, resize_bounds, titlebar, bounds, offset_x, offset_y) = {
                    if let Some(window) = &self.windows[i] {
                        (
                            window.visible,
                            window.close_button_bounds(),
                            window.resize_handle_bounds(),
                            window.titlebar_bounds(),
                            window.bounds,
                            mx - window.bounds.x,
                            my - window.bounds.y,
                        )
                    } else {
                        continue;
                    }
                };
                
                if !visible {
                    continue;
                }
                
                // Check close button
                if close_bounds.contains(mx, my) {
                    self.dirty_rects.push(bounds);
                    if let Some(w) = &mut self.windows[i] {
                        w.visible = false;
                    }
                    return (true, false); // Need redraw
                }
                
                // Check resize handle
                if resize_bounds.contains(mx, my) {
                    self.bring_to_front(i);
                    self.drag.mode = InteractionMode::Resizing;
                    self.drag.window_id = i;
                    self.drag.offset_x = 0;
                    self.drag.offset_y = 0;
                    self.drag.outline_rect = Some(bounds);
                    self.drag.original_bounds = Some(bounds);
                    return (false, true); // operation_just_started = true
                }
                
                // Check titlebar for drag start
                if titlebar.contains(mx, my) {
                    self.bring_to_front(i);
                    self.drag.mode = InteractionMode::Dragging;
                    self.drag.window_id = i;
                    self.drag.offset_x = offset_x;
                    self.drag.offset_y = offset_y;
                    self.drag.outline_rect = Some(bounds);
                    self.drag.original_bounds = Some(bounds);
                    return (false, true); // operation_just_started = true
                }
                
                // Click inside window body - bring to front
                if bounds.contains(mx, my) {
                    self.bring_to_front(i);
                    // Mark all windows dirty for z-order redraw
                    for j in 0..self.window_count {
                        if let Some(w) = &self.windows[j] {
                            if w.visible {
                                self.dirty_rects.push(w.bounds);
                            }
                        }
                    }
                    return (true, false);
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
        // Render in z-order (back to front)
        for zi in 0..self.window_count {
            let i = self.z_order[zi];
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
        // Render in z-order (back to front)
        for zi in 0..self.window_count {
            let i = self.z_order[zi];
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
    
    /// Check if currently dragging or resizing
    pub fn is_dragging(&self) -> bool {
        self.drag.mode != InteractionMode::None
    }
}
