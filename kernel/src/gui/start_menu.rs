//! Start Menu - App launcher popup

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::display::screen::Screen;
use crate::drivers::filesystem::FILESYSTEM;
use super::widgets::{Rect, draw_filled_rect, draw_rect_border, draw_text};
use super::theme::*;

/// A menu item entry
#[derive(Clone)]
pub struct MenuItem {
    pub name: String,
    pub app_id: String,  // Identifier to load the app (or filepath for filesystem apps)
}

impl MenuItem {
    pub fn new(name: &str, app_id: &str) -> Self {
        Self {
            name: String::from(name),
            app_id: String::from(app_id),
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
        
        // Add built-in native apps first
        items.push(MenuItem::new("Code Editor", "editor"));
        items.push(MenuItem::new("File Explorer", "explorer"));
        items.push(MenuItem::new("Terminal", "terminal"));
        items.push(MenuItem::new("Documentation", "docs"));
        items.push(MenuItem::new("---", "---")); // Separator
        
        // Add .pa apps embedded at compile time
        for app_id in get_app_ids() {
            // Create a nice display name from the app_id
            let name = Self::format_app_name(app_id);
            items.push(MenuItem::new(&name, app_id));
        }
        
        // Add .pa apps from filesystem "apps" folder
        {
            let fs = FILESYSTEM.lock();
            let entries = fs.list_directory("apps");
            for (name, is_dir) in entries {
                if !is_dir && name.ends_with(".pa") {
                    // Extract the app name without .pa extension
                    let app_name = name.trim_end_matches(".pa");
                    let display_name = Self::format_app_name(app_name);
                    // Store the full path so it can be opened from filesystem
                    let filepath = alloc::format!("apps/{}", name);
                    items.push(MenuItem::new(&display_name, &filepath));
                }
            }
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
    
    /// Refresh the menu items (call when apps folder changes)
    pub fn refresh(&mut self) {
        use super::pa_parser::get_app_ids;
        
        self.items.clear();
        
        // Add built-in native apps first
        self.items.push(MenuItem::new("Code Editor", "editor"));
        self.items.push(MenuItem::new("File Explorer", "explorer"));
        self.items.push(MenuItem::new("Terminal", "terminal"));
        self.items.push(MenuItem::new("Documentation", "docs"));
        self.items.push(MenuItem::new("---", "---")); // Separator
        
        // Add .pa apps embedded at compile time
        for app_id in get_app_ids() {
            let name = Self::format_app_name(app_id);
            self.items.push(MenuItem::new(&name, app_id));
        }
        
        // Add .pa apps from filesystem "apps" folder
        {
            let fs = FILESYSTEM.lock();
            let entries = fs.list_directory("apps");
            for (name, is_dir) in entries {
                if !is_dir && name.ends_with(".pa") {
                    let app_name = name.trim_end_matches(".pa");
                    let display_name = Self::format_app_name(app_name);
                    let filepath = alloc::format!("apps/{}", name);
                    self.items.push(MenuItem::new(&display_name, &filepath));
                }
            }
        }
        
        self.height = self.items.len() * Self::ITEM_HEIGHT + Self::PADDING * 2;
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
    pub fn handle_input(&mut self, mx: i32, my: i32, mouse_pressed: bool) -> (bool, Option<String>) {
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
                    let app_id = self.items[index].app_id.clone();
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
