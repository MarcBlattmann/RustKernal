//! Desktop Module - Desktop environment
//!
//! Features:
//! - Background rendering
//! - Taskbar with Start Menu
//! - Window management integration
//! - Dirty rectangle tracking for efficient redraws

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::display::screen::Screen;
use crate::drivers::keyboard::SpecialKey;
use crate::drivers::filesystem::FILESYSTEM;
use super::theme::*;
use super::widgets::{Rect, draw_filled_rect, draw_rect_border, draw_text};
use super::window::{WindowManager, DirtyRegion};
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
    
    /// Handle keyboard input, route to active window
    pub fn handle_keyboard_input(&mut self, key: char, ctrl: bool) {
        self.window_manager.handle_keyboard_input(key, ctrl);
    }
    
    /// Handle special key input (arrows, function keys, etc.)
    pub fn handle_special_key_input(&mut self, key: SpecialKey) {
        self.window_manager.handle_special_key_input(key);
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
                
                // Check if it's a filesystem path (contains /) or a .pa file
                if app_id.ends_with(".pa") {
                    // Run .pa file from filesystem
                    self.run_pa_file(&app_id);
                } else {
                    // Launch built-in or embedded app
                    self.launch_app(&app_id);
                }
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
        use super::pa_parser::{load_app, create_error_app};
        use super::app::{create_code_editor_app, create_terminal_app, create_docs_app, create_explorer_app};
        
        // Skip separator items
        if app_id == "---" {
            return;
        }
        
        // Check for built-in native apps first
        let app = match app_id {
            "editor" => Ok(create_code_editor_app()),
            "terminal" => Ok(create_terminal_app()),
            "docs" => Ok(create_docs_app()),
            "explorer" => Ok(create_explorer_app()),
            "files" => Ok(super::app::create_file_manager_app()),
            _ => {
                // Try to load from auto-detected .pa files
                load_app(app_id)
            }
        };
        
        match app {
            Ok(app_def) => {
                // Mark the new window area as dirty
                let win_rect = Rect::new(app_def.x, app_def.y, app_def.width, app_def.height);
                self.dirty_rects.push(win_rect);
                
                // For native apps, add with special type
                let native_type = match app_id {
                    "editor" => Some(super::window::NativeAppType::CodeEditor),
                    "terminal" => Some(super::window::NativeAppType::Terminal),
                    "docs" => Some(super::window::NativeAppType::DocViewer),
                    "explorer" => Some(super::window::NativeAppType::FileExplorer),
                    _ => None,
                };
                
                if let Some(native_type) = native_type {
                    self.window_manager.add_native_app(&app_def, native_type);
                } else {
                    self.window_manager.add_app(&app_def);
                }
            }
            Err(error) => {
                // Show error dialog
                let error_app = create_error_app(app_id, &error);
                let win_rect = Rect::new(error_app.x, error_app.y, error_app.width, error_app.height);
                self.dirty_rects.push(win_rect);
                self.window_manager.add_app(&error_app);
            }
        }
    }
    
    /// Open a file in the code editor
    pub fn open_file_in_editor(&mut self, filepath: &str) {
        use super::app::create_code_editor_app;
        
        // Read file content
        let content = {
            let fs = FILESYSTEM.lock();
            if let Some(data) = fs.read_file(filepath) {
                String::from_utf8_lossy(&data).into_owned()
            } else {
                String::new()
            }
        };
        
        // Create editor app
        let app_def = create_code_editor_app();
        
        // Mark the window area as dirty
        let win_rect = Rect::new(app_def.x, app_def.y, app_def.width, app_def.height);
        self.dirty_rects.push(win_rect);
        
        // Add as native app and get window ID
        if let Some(window_id) = self.window_manager.add_native_app(&app_def, super::window::NativeAppType::CodeEditor) {
            // Open the file in the editor
            self.window_manager.open_file_in_editor(window_id, filepath, &content);
        }
    }
    
    /// Run a .pa file from the filesystem (called from start menu or Ctrl+R in editor)
    pub fn run_pa_file(&mut self, filepath: &str) {
        use super::pa_parser::{parse_pa, create_error_app, ParseError};
        
        // Read file content from filesystem
        let content = {
            let fs = FILESYSTEM.lock();
            if let Some(data) = fs.read_file(filepath) {
                String::from_utf8_lossy(&data).into_owned()
            } else {
                // File not found
                let error_app = create_error_app(filepath, &ParseError::NotFound);
                let win_rect = Rect::new(error_app.x, error_app.y, error_app.width, error_app.height);
                self.dirty_rects.push(win_rect);
                self.window_manager.add_app(&error_app);
                return;
            }
        };
        
        // Parse the .pa content
        match parse_pa(&content) {
            Ok(app_def) => {
                let win_rect = Rect::new(app_def.x, app_def.y, app_def.width, app_def.height);
                self.dirty_rects.push(win_rect);
                self.window_manager.add_app(&app_def);
            }
            Err(error) => {
                let error_app = create_error_app(filepath, &error);
                let win_rect = Rect::new(error_app.x, error_app.y, error_app.width, error_app.height);
                self.dirty_rects.push(win_rect);
                self.window_manager.add_app(&error_app);
            }
        }
    }
    
    /// Process any pending actions from window manager (e.g., opening files)
    pub fn process_pending_actions(&mut self) {
        use super::script::ScriptAction;
        
        if let Some((_window_id, action)) = self.window_manager.take_pending_action() {
            match action {
                ScriptAction::Open(filepath) => {
                    // Check if this looks like a file path (contains / or .)
                    if filepath.contains('/') || filepath.contains('.') {
                        // It's a file - always open in editor (including .pa files for editing)
                        self.open_file_in_editor(&filepath);
                    } else {
                        // It's an app ID, launch it
                        self.launch_app(&filepath);
                    }
                }
                ScriptAction::RunApp(filepath) => {
                    // Run a .pa app file (from Ctrl+R in code editor)
                    self.run_pa_file(&filepath);
                }
                ScriptAction::RefreshStartMenu => {
                    // Apps folder was modified - refresh the start menu
                    self.start_menu.refresh();
                }
                ScriptAction::Close => {
                    // Window was closed - already handled
                }
                ScriptAction::Minimize | ScriptAction::None => {
                    // No action needed
                }
            }
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
    
    /// Check if there are label-only updates (no full redraw needed)
    pub fn has_label_updates(&self) -> bool {
        self.window_manager.has_label_updates()
    }
    
    /// Render only dynamic label updates (flicker-free)
    pub fn render_label_updates(&mut self, screen: &mut Screen) {
        self.window_manager.render_label_updates(screen);
    }
    
    /// Take dirty regions from both desktop and window manager
    pub fn take_dirty_regions(&mut self) -> Vec<DirtyRegion> {
        let mut regions = Vec::new();
        // Convert desktop's plain Rects to DirtyRegion::Rect
        for rect in self.dirty_rects.drain(..) {
            regions.push(DirtyRegion::Rect(rect));
        }
        // Add window manager's dirty regions directly
        regions.extend(self.window_manager.take_dirty_regions());
        regions
    }
    
    /// Render only dirty regions (smart partial redraw)
    pub fn render_dirty_regions(&mut self, screen: &mut Screen, dirty: &[DirtyRegion]) {
        // Collect all Rect regions that need background clearing
        let mut rects_to_clear = Vec::new();
        for region in dirty {
            match region {
                DirtyRegion::Rect(rect) => rects_to_clear.push(*rect),
                DirtyRegion::RectFromWindow(rect, _) => rects_to_clear.push(*rect),
                _ => {}
            }
        }
        
        // Fill areas with background first (only for Rect dirty regions)
        for rect in &rects_to_clear {
            self.render_background_rect(screen, rect);
        }
        
        // Redraw taskbar if any Rect dirty region intersects it
        let taskbar_y = (self.height - TASKBAR_HEIGHT) as i32;
        let taskbar_rect = Rect::new(0, taskbar_y, self.width, TASKBAR_HEIGHT);
        for rect in &rects_to_clear {
            if rect.intersects(&taskbar_rect) {
                self.render_taskbar(screen);
                break;
            }
        }
        
        // Render windows using the smart dirty region system
        self.window_manager.render_dirty_regions(screen, dirty);
        
        // Render start menu on top
        self.start_menu.render(screen);
    }
    
    /// Render desktop (full redraw)
    pub fn render(&mut self, screen: &mut Screen) {
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
