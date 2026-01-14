//! Start Menu - App launcher popup

use alloc::string::String;
use alloc::vec::Vec;
use alloc::vec;
use crate::drivers::display::screen::Screen;
use super::widgets::{Rect, draw_filled_rect, draw_rect_border, draw_text};
use super::theme::*;

/// A menu item entry
#[derive(Clone)]
pub struct MenuItem {
    pub name: String,
    pub app_id: &'static str,  // Identifier to load the app
}

impl MenuItem {
    pub fn new(name: &str, app_id: &'static str) -> Self {
        Self {
            name: String::from(name),
            app_id,
        }
    }
}

/// The start menu popup
pub struct StartMenu {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub items: Vec<MenuItem>,
    pub hover_index: Option<usize>,
}

impl StartMenu {
    pub const ITEM_HEIGHT: usize = 28;
    pub const PADDING: usize = 4;
    
    pub fn new() -> Self {
        // Auto-detect apps from the apps folder
        use super::pa_parser::get_app_ids;
        
        let mut items = Vec::new();
        for app_id in get_app_ids() {
            // Create a nice display name from the app_id
            let name = Self::format_app_name(app_id);
            items.push(MenuItem::new(&name, app_id));
        }
        
        let height = items.len() * Self::ITEM_HEIGHT + Self::PADDING * 2;
        
        Self {
            visible: false,
            x: 4,
            y: 0, // Will be set based on taskbar position
            width: 180,
            height,
            items,
            hover_index: None,
        }
    }
    
    /// Format app ID into a nice display name
    /// e.g., "settings_flex" -> "Settings Flex", "welcome" -> "Welcome"
    fn format_app_name(app_id: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = true;
        
        for c in app_id.chars() {
            if c == '_' || c == '-' {
                result.push(' ');
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
        result
    }
    
    /// Position the menu above the taskbar
    pub fn position_above_taskbar(&mut self, screen_height: usize) {
        self.y = (screen_height - TASKBAR_HEIGHT - self.height) as i32;
    }
    
    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible {
            self.hover_index = None;
        }
    }
    
    /// Show menu
    pub fn show(&mut self) {
        self.visible = true;
    }
    
    /// Hide menu
    pub fn hide(&mut self) {
        self.visible = false;
        self.hover_index = None;
    }
    
    /// Get bounds
    pub fn bounds(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
    
    /// Check if point is in start button area
    pub fn is_in_start_button(mx: i32, my: i32, screen_height: usize) -> bool {
        let taskbar_y = screen_height - TASKBAR_HEIGHT;
        mx >= 4 && mx < 68 && my >= taskbar_y as i32 + 4 && my < (taskbar_y + TASKBAR_HEIGHT - 4) as i32
    }
    
    /// Handle mouse input
    /// Returns: (needs_redraw, clicked_app_id)
    pub fn handle_input(&mut self, mx: i32, my: i32, mouse_pressed: bool) -> (bool, Option<&'static str>) {
        if !self.visible {
            return (false, None);
        }
        
        let bounds = self.bounds();
        
        // Check if mouse is in menu
        if bounds.contains(mx, my) {
            // Calculate which item is hovered
            let relative_y = (my - self.y - Self::PADDING as i32) as usize;
            let index = relative_y / Self::ITEM_HEIGHT;
            
            let old_hover = self.hover_index;
            
            if index < self.items.len() {
                self.hover_index = Some(index);
                
                if mouse_pressed {
                    let app_id = self.items[index].app_id;
                    self.hide();
                    return (true, Some(app_id));
                }
            } else {
                self.hover_index = None;
            }
            
            return (old_hover != self.hover_index, None);
        } else {
            // Click outside closes menu
            if mouse_pressed {
                self.hide();
                return (true, None);
            }
        }
        
        (false, None)
    }
    
    /// Render the menu
    pub fn render(&self, screen: &mut Screen) {
        if !self.visible {
            return;
        }
        
        let bounds = self.bounds();
        
        // Menu background
        draw_filled_rect(screen, &bounds, COLOR_WINDOW_BG);
        draw_rect_border(screen, &bounds, COLOR_WINDOW_BORDER, 1);
        
        // Draw items
        for (i, item) in self.items.iter().enumerate() {
            let item_y = self.y + Self::PADDING as i32 + (i * Self::ITEM_HEIGHT) as i32;
            let item_rect = Rect::new(
                self.x + Self::PADDING as i32,
                item_y,
                self.width - Self::PADDING * 2,
                Self::ITEM_HEIGHT - 2,
            );
            
            // Highlight hovered item
            if self.hover_index == Some(i) {
                draw_filled_rect(screen, &item_rect, COLOR_TITLEBAR);
            }
            
            // Draw item text
            let text_y = item_y as usize + (Self::ITEM_HEIGHT - 16) / 2;
            draw_text(screen, (self.x + 12) as usize, text_y, &item.name, COLOR_FOREGROUND);
        }
    }
}
