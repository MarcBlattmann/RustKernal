//! Desktop Module - Desktop environment
//!
//! Features:
//! - Background rendering
//! - Taskbar with Start Menu
//! - Window management integration
//! - Dirty rectangle tracking for efficient redraws

use alloc::vec::Vec;
use crate::drivers::display::screen::Screen;
use super::theme::*;
use super::widgets::{Rect, draw_filled_rect, draw_rect_border, draw_text};
use super::window::WindowManager;
use super::start_menu::StartMenu;

/// Desktop environment
pub struct Desktop {
    width: usize,
    height: usize,
    window_manager: WindowManager,
    start_menu: StartMenu,
    last_mouse_down: bool,
    dirty_rects: Vec<Rect>,
}

impl Desktop {
    pub fn new(width: usize, height: usize) -> Self {
        let mut start_menu = StartMenu::new();
        start_menu.position_above_taskbar(height);
        
        let desktop = Self {
            width,
            height,
            window_manager: WindowManager::new(),
            start_menu,
            last_mouse_down: false,
            dirty_rects: Vec::new(),
        };
        
        // Desktop starts empty - launch apps from the Start Menu
        
        desktop
    }
    
    /// Handle input, returns (needs_redraw, operation_just_started)
    pub fn handle_input(&mut self, screen: &mut Screen, mx: i32, my: i32, mouse_down: bool) -> (bool, bool) {
        let mouse_pressed = mouse_down && !self.last_mouse_down;
        self.last_mouse_down = mouse_down;
        
        // Handle start menu first if visible
        if self.start_menu.visible {
            // Save bounds before handle_input might hide the menu
            let menu_rect = self.start_menu.bounds();
            let was_visible = true;
            
            let (menu_redraw, app_id) = self.start_menu.handle_input(mx, my, mouse_pressed);
            
            if let Some(app_id) = app_id {
                // Menu was closed and app selected - mark menu area as dirty
                self.dirty_rects.push(menu_rect);
                
                // Launch the selected app - mark new window area as dirty
                self.launch_app(app_id);
                return (true, false);
            }
            
            // Check if menu was just closed by clicking outside
            if was_visible && !self.start_menu.visible {
                // Menu was closed - need to redraw to clear it
                self.dirty_rects.push(menu_rect);
                let taskbar_y = (self.height - TASKBAR_HEIGHT) as i32;
                self.dirty_rects.push(Rect::new(0, taskbar_y, 80, TASKBAR_HEIGHT));
                return (true, false);
            }
            
            // Only redraw menu if hover state changed - do it directly here
            if menu_redraw {
                // Just redraw the menu, not the whole screen
                self.start_menu.render(screen);
                return (false, false); // Return false since we already handled the redraw
            }
        }
        
        // Check start button click
        if mouse_pressed && StartMenu::is_in_start_button(mx, my, self.height) {
            // Mark menu area and start button as dirty
            self.dirty_rects.push(self.start_menu.bounds());
            let taskbar_y = (self.height - TASKBAR_HEIGHT) as i32;
            self.dirty_rects.push(Rect::new(0, taskbar_y, 80, TASKBAR_HEIGHT));
            self.start_menu.toggle();
            return (true, false);
        }
        
        // Handle window manager input
        self.window_manager.handle_input(screen, mx, my, mouse_down)
    }
    
    /// Check if mouse is over a resize handle (for cursor change)
    pub fn is_over_resize_handle(&self, mx: i32, my: i32) -> bool {
        self.window_manager.is_over_resize_handle(mx, my)
    }
    
    /// Launch an app by its ID
    fn launch_app(&mut self, app_id: &str) {
        use super::pa_parser::*;
        
        let app = match app_id {
            "welcome" => load_welcome_app().ok(),
            "about" => load_about_app().ok(),
            "calculator" => load_calculator_app().ok(),
            "notepad" => load_notepad_app().ok(),
            "settings" => load_settings_app().ok(),
            "files" => {
                // Use the programmatic version
                Some(super::app::create_file_manager_app())
            }
            _ => None,
        };
        
        if let Some(app) = app {
            // Mark the new window area as dirty
            let win_rect = Rect::new(app.x, app.y, app.width, app.height);
            self.dirty_rects.push(win_rect);
            self.window_manager.add_app(&app);
        }
    }
    
    /// Draw initial drag outline (call after hiding cursor)
    pub fn draw_initial_outline(&mut self, screen: &mut Screen) {
        self.window_manager.draw_initial_outline(screen);
    }
    
    /// Check if currently dragging a window
    pub fn is_dragging(&self) -> bool {
        self.window_manager.is_dragging()
    }
    
    /// Take dirty rects from both desktop and window manager
    pub fn take_dirty_rects(&mut self) -> Vec<Rect> {
        let mut rects = core::mem::take(&mut self.dirty_rects);
        rects.extend(self.window_manager.take_dirty_rects());
        rects
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
        
        // Render start menu on top
        self.start_menu.render(screen);
    }
    
    /// Render desktop (full redraw)
    pub fn render(&self, screen: &mut Screen) {
        // Clear background
        self.render_background(screen);
        
        // Render taskbar
        self.render_taskbar(screen);
        
        // Render windows
        self.window_manager.render(screen);
        
        // Render start menu on top
        self.start_menu.render(screen);
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
        
        // Start button area (highlighted if menu is open)
        let start_rect = Rect::new(4, taskbar_y as i32 + 4, 60, TASKBAR_HEIGHT - 8);
        if self.start_menu.visible {
            draw_filled_rect(screen, &start_rect, COLOR_TITLEBAR);
        }
        draw_rect_border(screen, &start_rect, COLOR_FOREGROUND, 1);
        draw_text(screen, 12, taskbar_y + 12, "Start", COLOR_FOREGROUND);
        
        // System name on right
        let text = "Pursuit OS";
        let text_x = self.width - text.len() * 8 - 12;
        draw_text(screen, text_x, taskbar_y + 12, text, COLOR_FOREGROUND);
    }
}
