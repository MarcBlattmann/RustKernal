//! Desktop Module - Desktop environment
//!
//! Features:
//! - Background rendering
//! - Taskbar
//! - Window management integration
//! - Dirty rectangle tracking for efficient redraws

use crate::drivers::display::screen::Screen;
use super::theme::*;
use super::widgets::{Rect, draw_filled_rect, draw_rect_border, draw_text};
use super::window::WindowManager;

/// Desktop environment
pub struct Desktop {
    width: usize,
    height: usize,
    window_manager: WindowManager,
}

impl Desktop {
    pub fn new(width: usize, height: usize) -> Self {
        let mut desktop = Self {
            width,
            height,
            window_manager: WindowManager::new(),
        };
        
        // Create demo windows using the new app builder system
        use super::app::{create_welcome_app, create_about_app};
        desktop.window_manager.add_app(&create_welcome_app());
        desktop.window_manager.add_app(&create_about_app());
        
        desktop
    }
    
    /// Handle input, returns (needs_partial_redraw, drag_just_started)
    pub fn handle_input(&mut self, screen: &mut Screen, mx: i32, my: i32, mouse_down: bool) -> (bool, bool) {
        self.window_manager.handle_input(screen, mx, my, mouse_down)
    }
    
    /// Draw initial drag outline (call after hiding cursor)
    pub fn draw_initial_outline(&mut self, screen: &mut Screen) {
        self.window_manager.draw_initial_outline(screen);
    }
    
    /// Check if currently dragging a window
    pub fn is_dragging(&self) -> bool {
        self.window_manager.is_dragging()
    }
    
    /// Take dirty rects from window manager
    pub fn take_dirty_rects(&mut self) -> alloc::vec::Vec<Rect> {
        self.window_manager.take_dirty_rects()
    }
    
    /// Render only dirty regions (partial redraw)
    pub fn render_dirty(&self, screen: &mut Screen, dirty: &[Rect]) {
        // For each dirty rect, redraw just that area
        for rect in dirty {
            // Fill dirty area with background first
            self.render_background_rect(screen, rect);
        }
        
        // Redraw taskbar if any dirty rect intersects it
        let taskbar_y = (self.height - TASKBAR_HEIGHT) as i32;
        let taskbar_rect = Rect::new(0, taskbar_y, self.width, TASKBAR_HEIGHT);
        for rect in dirty {
            if rect.intersects(&taskbar_rect) {
                self.render_taskbar(screen);
                break;
            }
        }
        
        // Render windows that intersect dirty areas
        self.window_manager.render_dirty(screen, dirty);
    }
    
    /// Render desktop (full redraw)
    pub fn render(&self, screen: &mut Screen) {
        // Clear background
        self.render_background(screen);
        
        // Render taskbar
        self.render_taskbar(screen);
        
        // Render windows
        self.window_manager.render(screen);
    }
    
    /// Render background
    fn render_background(&self, screen: &mut Screen) {
        let rect = Rect::new(0, 0, self.width, self.height);
        draw_filled_rect(screen, &rect, COLOR_BACKGROUND);
    }
    
    /// Render background for just a specific rect
    fn render_background_rect(&self, screen: &mut Screen, rect: &Rect) {
        draw_filled_rect(screen, rect, COLOR_BACKGROUND);
    }
    
    /// Render taskbar
    fn render_taskbar(&self, screen: &mut Screen) {
        let taskbar_y = self.height - TASKBAR_HEIGHT;
        
        // Taskbar background
        let taskbar_rect = Rect::new(0, taskbar_y as i32, self.width, TASKBAR_HEIGHT);
        draw_filled_rect(screen, &taskbar_rect, COLOR_TASKBAR_BG);
        
        // Taskbar top border
        for x in 0..self.width {
            screen.write_pixel(x, taskbar_y, COLOR_FOREGROUND);
        }
        
        // Start button area
        let start_rect = Rect::new(4, taskbar_y as i32 + 4, 60, TASKBAR_HEIGHT - 8);
        draw_rect_border(screen, &start_rect, COLOR_FOREGROUND, 1);
        draw_text(screen, 12, taskbar_y + 12, "Start", COLOR_FOREGROUND);
        
        // System name on right
        let text = "Pursuit OS";
        let text_x = self.width - text.len() * 8 - 12;
        draw_text(screen, text_x, taskbar_y + 12, text, COLOR_FOREGROUND);
    }
}
