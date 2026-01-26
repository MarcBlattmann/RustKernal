//! Window Module - Clean window management
//!
//! Features:
//! - Window structure with title bar
//! - Dragging support with XOR outline
//! - Dirty rectangle tracking for efficient redraws
//! - PursuitScript support for button click handlers
//! - Native app hosting (Editor, Terminal, Explorer, Docs)

use alloc::string::String;
use crate::drivers::display::screen::Screen;
use super::theme::*;
use super::widgets::{Rect, draw_filled_rect, draw_filled_rect_clipped, draw_rect_border, draw_rect_border_clipped, draw_text, draw_text_clipped, draw_close_button, draw_xor_outline};
use super::app::{AppDef, Element};
use super::script::{ScriptEngine, ScriptAction};
use super::builtin_apps::{CodeEditor, FileExplorer, TerminalEmulator, DocViewer, SpecialKey, ExplorerAction};

/// Maximum number of windows
const MAX_WINDOWS: usize = 8;

/// Types of dirty regions for efficient partial redraws
#[derive(Clone, Copy)]
pub enum DirtyRegion {
    /// Full window needs redraw (moved, resized, opened, etc.)
    FullWindow(usize), // window_id
    /// Only content area needs redraw (scroll, command output, etc.)
    ContentOnly(usize), // window_id
    /// Only the typing/input area needs redraw (single char typed)
    TypingOnly(usize), // window_id
    /// Arbitrary rectangle from a window move (only redraw windows below source_window_id in z-order)
    RectFromWindow(Rect, usize), // rect, source_window_id
    /// Arbitrary rectangle - redraw all overlapping windows (e.g., area behind closed window)
    Rect(Rect),
}

/// Types of native apps
#[derive(Clone, Copy, PartialEq)]
pub enum NativeAppType {
    None,
    CodeEditor,
    FileExplorer,
    Terminal,
    DocViewer,
}

/// Native app state container
pub enum NativeApp {
    None,
    Editor(CodeEditor),
    Explorer(FileExplorer),
    Terminal(TerminalEmulator),
    Docs(DocViewer),
}

/// Cached position of a rendered label that uses variable interpolation
#[derive(Clone)]
struct DynamicLabel {
    bounds: Rect,
    text: String, // Original text with {var} placeholders
}

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
    /// Script engine for this window
    pub script: ScriptEngine,
    /// Cached positions of labels with {variables} for partial redraws
    dynamic_labels: alloc::vec::Vec<DynamicLabel>,
    /// Native app type
    pub native_type: NativeAppType,
    /// Native app state
    pub native_app: NativeApp,
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
            script: ScriptEngine::new(),
            dynamic_labels: alloc::vec::Vec::new(),
            native_type: NativeAppType::None,
            native_app: NativeApp::None,
        }
    }
    
    /// Create window from AppDef
    pub fn from_app(id: usize, app: &AppDef) -> Self {
        let mut script = ScriptEngine::new();
        
        // Initialize script engine with app's script block
        if let Some(ref script_source) = app.script {
            script.execute_script(script_source);
        }
        
        Self {
            id,
            title: app.title.clone(),
            bounds: Rect::new(app.x, app.y, app.width, app.height),
            visible: app.visible,
            elements: app.elements.clone(),
            original_width: app.width,
            original_height: app.height,
            script,
            dynamic_labels: alloc::vec::Vec::new(),
            native_type: NativeAppType::None,
            native_app: NativeApp::None,
        }
    }
    
    /// Create a native app window
    pub fn from_native(id: usize, app: &AppDef, native_type: NativeAppType) -> Self {
        let native_app = match native_type {
            NativeAppType::CodeEditor => NativeApp::Editor(CodeEditor::new()),
            NativeAppType::FileExplorer => NativeApp::Explorer(FileExplorer::new()),
            NativeAppType::Terminal => NativeApp::Terminal(TerminalEmulator::new()),
            NativeAppType::DocViewer => NativeApp::Docs(DocViewer::new()),
            NativeAppType::None => NativeApp::None,
        };
        
        Self {
            id,
            title: app.title.clone(),
            bounds: Rect::new(app.x, app.y, app.width, app.height),
            visible: app.visible,
            elements: alloc::vec::Vec::new(),
            original_width: app.width,
            original_height: app.height,
            script: ScriptEngine::new(),
            dynamic_labels: alloc::vec::Vec::new(),
            native_type,
            native_app,
        }
    }
    
    /// Check if this is a native app window
    pub fn is_native(&self) -> bool {
        self.native_type != NativeAppType::None
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
    pub fn render(&mut self, screen: &mut Screen) {
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
        
        // Content area for clipping - elements outside will be hidden
        let content = self.content_bounds();
        let clip = content;
        
        // Check if this is a native app window
        if self.is_native() {
            // Render native app content
            match &self.native_app {
                NativeApp::Editor(editor) => editor.render(screen, &content),
                NativeApp::Explorer(explorer) => explorer.render(screen, &content),
                NativeApp::Terminal(terminal) => terminal.render(screen, &content),
                NativeApp::Docs(docs) => docs.render(screen, &content),
                NativeApp::None => {}
            }
            return;
        }
        
        // Clear cached dynamic labels - they'll be rebuilt during render
        self.dynamic_labels.clear();
        
        // Clone elements to avoid borrow checker issues with &mut self
        let elements = self.elements.clone();
        
        // Render elements - layout containers fill the content area
        for elem in &elements {
            self.render_element(screen, elem, content.x, content.y, content.width, content.height, &clip);
        }
    }
    
    /// Render ONLY the content area (not titlebar or borders)
    /// Used for efficient partial updates when only content changed (e.g., keyboard input)
    pub fn render_content_only(&mut self, screen: &mut Screen) {
        if !self.visible {
            return;
        }
        
        let content = self.content_bounds();
        
        // Check if this is a native app window
        if self.is_native() {
            // Render native app content only
            match &self.native_app {
                NativeApp::Editor(editor) => editor.render(screen, &content),
                NativeApp::Explorer(explorer) => explorer.render(screen, &content),
                NativeApp::Terminal(terminal) => terminal.render(screen, &content),
                NativeApp::Docs(docs) => docs.render(screen, &content),
                NativeApp::None => {}
            }
            return;
        }
        
        // For non-native apps, fill content background and re-render elements
        draw_filled_rect(screen, &content, COLOR_WINDOW_BG);
        
        // Clear cached dynamic labels - they'll be rebuilt during render
        self.dynamic_labels.clear();
        
        // Clone elements to avoid borrow checker issues with &mut self
        let elements = self.elements.clone();
        let clip = content;
        
        // Render elements
        for elem in &elements {
            self.render_element(screen, elem, content.x, content.y, content.width, content.height, &clip);
        }
    }
    
    /// Render ONLY the typing area - minimal update for keyboard input
    /// This is the most efficient update for single character typing
    pub fn render_typing_area_only(&mut self, screen: &mut Screen) {
        if !self.visible {
            return;
        }
        
        let content = self.content_bounds();
        
        // Only native apps have optimized single-line rendering
        match &self.native_app {
            NativeApp::Editor(editor) => editor.render_cursor_line(screen, &content),
            NativeApp::Terminal(terminal) => terminal.render_input_line(screen, &content),
            NativeApp::Docs(_) | NativeApp::Explorer(_) | NativeApp::None => {
                // These don't have typing areas - fall back to full content render
                self.render_content_only(screen);
            }
        }
    }
    
    /// Handle keyboard input for native apps
    /// Returns ExplorerAction for file explorer (to open files) or code editor (to run apps)
    pub fn handle_key(&mut self, key: char, ctrl: bool) -> ExplorerAction {
        match &mut self.native_app {
            NativeApp::Editor(editor) => editor.handle_key(key, ctrl),
            NativeApp::Terminal(terminal) => { terminal.handle_key(key); ExplorerAction::None },
            NativeApp::Docs(docs) => { docs.handle_key(key); ExplorerAction::None },
            NativeApp::Explorer(explorer) => explorer.handle_key(key),
            _ => ExplorerAction::None
        }
    }
    
    /// Handle special key input for native apps
    pub fn handle_special_key(&mut self, key: SpecialKey) {
        match &mut self.native_app {
            NativeApp::Editor(editor) => editor.handle_special_key(key),
            NativeApp::Terminal(terminal) => terminal.handle_special_key(key),
            NativeApp::Explorer(explorer) => explorer.handle_special_key(key),
            NativeApp::Docs(docs) => {
                match key {
                    SpecialKey::Up => docs.scroll(-1),
                    SpecialKey::Down => docs.scroll(1),
                    SpecialKey::PageUp => docs.scroll(-10),
                    SpecialKey::PageDown => docs.scroll(10),
                    SpecialKey::Home => docs.scroll_to_top(),
                    SpecialKey::End => docs.scroll_to_bottom(),
                    _ => {}
                }
            }
            NativeApp::None => {}
        }
    }
    
    /// Handle mouse click for native apps (file explorer, etc.)
    /// Returns (needs_redraw, explorer_action)
    pub fn handle_native_click(&mut self, mx: i32, my: i32, right_button: bool) -> (bool, ExplorerAction) {
        let content = self.content_bounds();
        
        match &mut self.native_app {
            NativeApp::Explorer(explorer) => explorer.handle_click(mx, my, &content, right_button),
            _ => (false, ExplorerAction::None)
        }
    }
    
    /// Render only the dynamic labels (labels with {variables})
    /// Used for efficient partial updates when only variable values change
    pub fn render_dynamic_labels(&self, screen: &mut Screen) {
        let clip = self.content_bounds();
        
        for label in &self.dynamic_labels {
            // Redraw background first to clear old text
            draw_filled_rect_clipped(screen, &label.bounds, COLOR_WINDOW_BG, &clip);
            
            // Draw updated text
            let display_text = self.script.interpolate(&label.text);
            draw_text_clipped(screen, label.bounds.x as usize, label.bounds.y as usize, &display_text, COLOR_FOREGROUND, &clip);
        }
    }
    
    /// Get the dirty rects for dynamic labels only
    pub fn get_dynamic_label_rects(&self) -> alloc::vec::Vec<Rect> {
        self.dynamic_labels.iter().map(|l| l.bounds).collect()
    }
    
    /// Render a single element at the given position
    fn render_element(&mut self, screen: &mut Screen, elem: &Element, x: i32, y: i32, available_w: usize, available_h: usize, clip: &Rect) {
        match elem {
            Element::Label { text, x: ox, y: oy } => {
                let px = x as usize + *ox as usize;
                let py = y as usize + *oy as usize;
                // Interpolate {variables} in text
                let display_text = self.script.interpolate(text);
                
                // Cache dynamic labels (those with {variables}) for partial updates
                if text.contains('{') {
                    // Use a generous width to account for changing values
                    let text_width = (display_text.len() + 10) * 8; // Extra buffer for growing numbers
                    let text_height = 10; // Slightly taller for safety
                    self.dynamic_labels.push(DynamicLabel {
                        bounds: Rect::new(px as i32, py as i32, text_width, text_height),
                        text: text.clone(),
                    });
                }
                
                draw_text_clipped(screen, px, py, &display_text, COLOR_FOREGROUND, clip);
            }
            Element::Button { text, x: ox, y: oy, width, height, .. } => {
                let rect = Rect::new(x + *ox, y + *oy, *width, *height);
                draw_filled_rect_clipped(screen, &rect, COLOR_BUTTON_BG, clip);
                draw_rect_border_clipped(screen, &rect, COLOR_BUTTON_BORDER, 1, clip);
                let tx = rect.x as usize + 4;
                let ty = rect.y as usize + (rect.height.saturating_sub(8)) / 2;
                draw_text_clipped(screen, tx, ty, text, COLOR_BUTTON_TEXT, clip);
            }
            Element::Panel { x: ox, y: oy, width, height } => {
                let rect = Rect::new(x + *ox, y + *oy, *width, *height);
                draw_filled_rect_clipped(screen, &rect, 0xFF222222, clip);
                draw_rect_border_clipped(screen, &rect, COLOR_WINDOW_BORDER, 1, clip);
            }
            Element::TextBox { x: ox, y: oy, width, height } => {
                let rect = Rect::new(x + *ox, y + *oy, *width, *height);
                draw_filled_rect_clipped(screen, &rect, 0xFF1A1A1A, clip);
                draw_rect_border_clipped(screen, &rect, COLOR_FOREGROUND, 1, clip);
            }
            Element::VBox { padding, gap, children } => {
                self.render_vbox(screen, children, x, y, available_w, available_h, *padding, *gap, clip);
            }
            Element::HBox { padding, gap, children } => {
                self.render_hbox(screen, children, x, y, available_w, available_h, *padding, *gap, clip);
            }
            Element::Spacer => {
                // Spacer doesn't render anything visually
            }
        }
    }
    
    /// Get the minimum size an element needs
    fn element_min_size(elem: &Element) -> (usize, usize) {
        match elem {
            Element::Label { text, .. } => (text.len() * 8, 16),
            Element::Button { text, width, height, .. } => {
                if *width > 0 && *height > 0 {
                    (*width, *height)
                } else {
                    (text.len() * 8 + 16, 24)
                }
            }
            Element::Panel { width, height, .. } => (*width, *height),
            Element::TextBox { width, height, .. } => (*width, *height),
            Element::VBox { padding, gap, children } => {
                let mut w: usize = 0;
                let mut h: usize = padding * 2;
                for (i, child) in children.iter().enumerate() {
                    if matches!(child, Element::Spacer) { continue; }
                    let (cw, ch) = Self::element_min_size(child);
                    w = w.max(cw);
                    h += ch;
                    if i > 0 { h += gap; }
                }
                (w + padding * 2, h)
            }
            Element::HBox { padding, gap, children } => {
                let mut w: usize = padding * 2;
                let mut h: usize = 0;
                for (i, child) in children.iter().enumerate() {
                    if matches!(child, Element::Spacer) { continue; }
                    let (cw, ch) = Self::element_min_size(child);
                    w += cw;
                    h = h.max(ch);
                    if i > 0 { w += gap; }
                }
                (w, h + padding * 2)
            }
            Element::Spacer => (0, 0),
        }
    }
    
    /// Render a VBox layout
    fn render_vbox(&mut self, screen: &mut Screen, children: &[Element], x: i32, y: i32, w: usize, h: usize, padding: usize, gap: usize, clip: &Rect) {
        let inner_x = x + padding as i32;
        let inner_y = y + padding as i32;
        let inner_w = w.saturating_sub(padding * 2);
        let inner_h = h.saturating_sub(padding * 2);
        
        // Count spacers and calculate fixed content height
        let mut spacer_count = 0;
        let mut fixed_height: usize = 0;
        for (i, child) in children.iter().enumerate() {
            if matches!(child, Element::Spacer) {
                spacer_count += 1;
            } else {
                let (_, ch) = Self::element_min_size(child);
                fixed_height += ch;
            }
            if i > 0 { fixed_height += gap; }
        }
        
        // Calculate spacer size
        let remaining = inner_h.saturating_sub(fixed_height);
        let spacer_size = if spacer_count > 0 { remaining / spacer_count } else { 0 };
        
        // Render children
        let mut cur_y = inner_y;
        for child in children.iter() {
            if matches!(child, Element::Spacer) {
                cur_y += spacer_size as i32;
            } else {
                let (_, ch) = Self::element_min_size(child);
                self.render_element(screen, child, inner_x, cur_y, inner_w, ch, clip);
                cur_y += ch as i32 + gap as i32;
            }
        }
    }
    
    /// Render an HBox layout
    fn render_hbox(&mut self, screen: &mut Screen, children: &[Element], x: i32, y: i32, w: usize, h: usize, padding: usize, gap: usize, clip: &Rect) {
        let inner_x = x + padding as i32;
        let inner_y = y + padding as i32;
        let inner_w = w.saturating_sub(padding * 2);
        let inner_h = h.saturating_sub(padding * 2);
        
        // Count spacers and calculate fixed content width
        let mut spacer_count = 0;
        let mut fixed_width: usize = 0;
        for (i, child) in children.iter().enumerate() {
            if matches!(child, Element::Spacer) {
                spacer_count += 1;
            } else {
                let (cw, _) = Self::element_min_size(child);
                fixed_width += cw;
            }
            if i > 0 { fixed_width += gap; }
        }
        
        // Calculate spacer size
        let remaining = inner_w.saturating_sub(fixed_width);
        let spacer_size = if spacer_count > 0 { remaining / spacer_count } else { 0 };
        
        // Render children
        let mut cur_x = inner_x;
        for child in children.iter() {
            if matches!(child, Element::Spacer) {
                cur_x += spacer_size as i32;
            } else {
                let (cw, _) = Self::element_min_size(child);
                self.render_element(screen, child, cur_x, inner_y, cw, inner_h, clip);
                cur_x += cw as i32 + gap as i32;
            }
        }
    }
    
    /// Handle a click at the given position (relative to window)
    /// Returns: None if no button was clicked, Some(action) if a button was clicked
    pub fn handle_click(&mut self, mx: i32, my: i32) -> Option<ScriptAction> {
        let content = self.content_bounds();
        
        // Clone elements to avoid borrow checker issues
        let elements = self.elements.clone();
        
        // Check each button element
        for elem in &elements {
            if let Some(action) = self.check_element_click(elem, mx, my, content.x, content.y, content.width, content.height) {
                return Some(action);
            }
        }
        
        None // No button was clicked
    }
    
    /// Recursively check if a click hits any element
    fn check_element_click(&mut self, elem: &Element, mx: i32, my: i32, x: i32, y: i32, w: usize, h: usize) -> Option<ScriptAction> {
        match elem {
            Element::Button { x: ox, y: oy, width, height, on_click, .. } => {
                let rect = Rect::new(x + *ox, y + *oy, *width, *height);
                if rect.contains(mx, my) {
                    if let Some(handler) = on_click {
                        self.script.execute_inline(handler);
                        return Some(self.script.take_action());
                    }
                }
                None
            }
            Element::VBox { padding, gap, children } => {
                let inner_x = x + *padding as i32;
                let inner_y = y + *padding as i32;
                let inner_w = w.saturating_sub(*padding * 2);
                let inner_h = h.saturating_sub(*padding * 2);
                
                // Walk through children
                let mut cur_y = inner_y;
                let spacer_count = children.iter().filter(|c| matches!(c, Element::Spacer)).count();
                let mut fixed_height: usize = 0;
                for (i, child) in children.iter().enumerate() {
                    if !matches!(child, Element::Spacer) {
                        let (_, ch) = Self::element_min_size(child);
                        fixed_height += ch;
                    }
                    if i > 0 { fixed_height += *gap; }
                }
                let remaining = inner_h.saturating_sub(fixed_height);
                let spacer_size = if spacer_count > 0 { remaining / spacer_count } else { 0 };
                
                for child in children.iter() {
                    if matches!(child, Element::Spacer) {
                        cur_y += spacer_size as i32;
                    } else {
                        let (_, ch) = Self::element_min_size(child);
                        if let Some(action) = self.check_element_click(child, mx, my, inner_x, cur_y, inner_w, ch) {
                            return Some(action);
                        }
                        cur_y += ch as i32 + *gap as i32;
                    }
                }
                None
            }
            Element::HBox { padding, gap, children } => {
                let inner_x = x + *padding as i32;
                let inner_y = y + *padding as i32;
                let inner_w = w.saturating_sub(*padding * 2);
                let inner_h = h.saturating_sub(*padding * 2);
                
                // Walk through children
                let mut cur_x = inner_x;
                let spacer_count = children.iter().filter(|c| matches!(c, Element::Spacer)).count();
                let mut fixed_width: usize = 0;
                for (i, child) in children.iter().enumerate() {
                    if !matches!(child, Element::Spacer) {
                        let (cw, _) = Self::element_min_size(child);
                        fixed_width += cw;
                    }
                    if i > 0 { fixed_width += *gap; }
                }
                let remaining = inner_w.saturating_sub(fixed_width);
                let spacer_size = if spacer_count > 0 { remaining / spacer_count } else { 0 };
                
                for child in children.iter() {
                    if matches!(child, Element::Spacer) {
                        cur_x += spacer_size as i32;
                    } else {
                        let (cw, _) = Self::element_min_size(child);
                        if let Some(action) = self.check_element_click(child, mx, my, cur_x, inner_y, cw, inner_h) {
                            return Some(action);
                        }
                        cur_x += cw as i32 + *gap as i32;
                    }
                }
                None
            }
            _ => None,
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
    /// Dirty regions to redraw (using DirtyRegion for smarter partial updates)
    dirty_regions: alloc::vec::Vec<DirtyRegion>,
    /// Pending action from script (window_id, action)
    pending_action: Option<(usize, ScriptAction)>,
    /// Windows that need only dynamic label updates (no full redraw)
    label_update_windows: alloc::vec::Vec<usize>,
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
            dirty_regions: alloc::vec::Vec::new(),
            pending_action: None,
            label_update_windows: alloc::vec::Vec::new(),
        }
    }
    
    /// Take any pending script action
    pub fn take_pending_action(&mut self) -> Option<(usize, ScriptAction)> {
        self.pending_action.take()
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
    
    /// Add a native app window
    pub fn add_native_app(&mut self, app: &AppDef, native_type: NativeAppType) -> Option<usize> {
        if self.window_count >= MAX_WINDOWS {
            return None;
        }
        
        let id = self.window_count;
        self.windows[id] = Some(Window::from_native(id, app, native_type));
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
    
    /// Open a file in an existing code editor window
    pub fn open_file_in_editor(&mut self, window_id: usize, filepath: &str, content: &str) {
        if let Some(window) = &mut self.windows[window_id] {
            if let NativeApp::Editor(editor) = &mut window.native_app {
                editor.open_file(filepath, content);
                // Update window title to show filename
                let filename = filepath.rsplit('/').next().unwrap_or(filepath);
                window.title.clear();
                window.title.push_str("Code Editor - ");
                window.title.push_str(filename);
            }
        }
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
            
            // Track old position for dirty rect - with source window for smart z-order redraw
            if let Some(original) = self.drag.original_bounds.take() {
                self.dirty_regions.push(DirtyRegion::RectFromWindow(original, self.drag.window_id));
            }
            
            if let Some(window) = &mut self.windows[self.drag.window_id] {
                let is_on_top = self.z_order[self.window_count - 1] == self.drag.window_id;
                
                if self.drag.mode == InteractionMode::Dragging {
                    // Move window to final position
                    let new_x = mx - self.drag.offset_x;
                    let new_y = my - self.drag.offset_y;
                    window.bounds.x = new_x;
                    window.bounds.y = new_y;
                    
                    // Only redraw the moved window if it's on top
                    // (windows below are handled by RectFromWindow)
                    if is_on_top {
                        self.dirty_regions.push(DirtyRegion::FullWindow(self.drag.window_id));
                    }
                } else if self.drag.mode == InteractionMode::Resizing {
                    // Resize window
                    let new_w = (mx - window.bounds.x).max(Window::MIN_WIDTH as i32) as usize;
                    let new_h = (my - window.bounds.y).max(Window::MIN_HEIGHT as i32) as usize;
                    window.bounds.width = new_w;
                    window.bounds.height = new_h;
                    // Always redraw on resize
                    self.dirty_regions.push(DirtyRegion::FullWindow(self.drag.window_id));
                }
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
                    self.dirty_regions.push(DirtyRegion::Rect(bounds));
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
                
                // Click inside window body - check buttons first, then native apps, then bring to front
                if bounds.contains(mx, my) {
                    // Check if this window is already on top
                    let already_on_top = self.z_order[self.window_count - 1] == i;
                    
                    // Check for button clicks first (for .pa apps)
                    let mut needs_redraw = false;
                    if let Some(window) = &mut self.windows[i] {
                        if let Some(action) = window.handle_click(mx, my) {
                            // A button was actually clicked
                            needs_redraw = true;
                            match action {
                                ScriptAction::Close => {
                                    self.dirty_regions.push(DirtyRegion::Rect(bounds));
                                    window.visible = false;
                                    return (true, false);
                                }
                                ScriptAction::Open(app_id) => {
                                    self.pending_action = Some((i, ScriptAction::Open(app_id)));
                                    self.dirty_regions.push(DirtyRegion::FullWindow(i));
                                }
                                ScriptAction::RunApp(filepath) => {
                                    self.pending_action = Some((i, ScriptAction::RunApp(filepath)));
                                    self.dirty_regions.push(DirtyRegion::FullWindow(i));
                                }
                                ScriptAction::RefreshStartMenu => {
                                    self.pending_action = Some((i, ScriptAction::RefreshStartMenu));
                                }
                                ScriptAction::Minimize => {
                                    // Could implement minimize later
                                }
                                ScriptAction::None => {
                                    // Button clicked, script ran - queue label-only update
                                    // This avoids flickering by not clearing the background
                                    self.label_update_windows.push(i);
                                }
                            }
                        } else {
                            // No button was clicked - check native app click handlers
                            let (native_needs_redraw, explorer_action) = window.handle_native_click(mx, my, false);
                            if native_needs_redraw {
                                needs_redraw = true;
                                self.dirty_regions.push(DirtyRegion::ContentOnly(i));
                            }
                            
                            // Handle explorer actions
                            match explorer_action {
                                ExplorerAction::OpenFile(filename) => {
                                    // Store the action to open a file
                                    self.pending_action = Some((i, ScriptAction::Open(filename)));
                                }
                                ExplorerAction::RunApp(filepath) => {
                                    // Run a .pa app file - uses same Open action
                                    self.pending_action = Some((i, ScriptAction::Open(filepath)));
                                }
                                ExplorerAction::NavigateToDir(_) => {
                                    // Navigation already handled inside explorer
                                    self.dirty_regions.push(DirtyRegion::ContentOnly(i));
                                }
                                ExplorerAction::RefreshStartMenu => {
                                    // Apps folder was modified
                                    self.pending_action = Some((i, ScriptAction::RefreshStartMenu));
                                }
                                ExplorerAction::None => {}
                            }
                        }
                    }
                    
                    // Only change z-order if not already on top
                    if !already_on_top {
                        self.bring_to_front(i);
                        // Only redraw the window that was brought to front
                        // It will be drawn on top of everything
                        self.dirty_regions.push(DirtyRegion::FullWindow(i));
                        return (true, false);
                    }
                    
                    // Return whether we need to redraw
                    return (needs_redraw, false);
                }
            }
        }
        
        (false, false)
    }
    
    /// Route keyboard input to the focused (topmost) window
    pub fn handle_keyboard_input(&mut self, key: char, ctrl: bool) {
        // Send to the topmost visible window
        if self.window_count > 0 {
            let topmost_idx = self.z_order[self.window_count - 1];
            if let Some(window) = &mut self.windows[topmost_idx] {
                if window.visible {
                    // Call the handler with ctrl state
                    let action = window.handle_key(key, ctrl);
                    
                    // Handle explorer actions (open file) or editor actions (run app)
                    match action {
                        ExplorerAction::OpenFile(filepath) => {
                            self.pending_action = Some((topmost_idx, ScriptAction::Open(filepath)));
                        }
                        ExplorerAction::RunApp(filepath) => {
                            // Run app - use dedicated RunApp action
                            self.pending_action = Some((topmost_idx, ScriptAction::RunApp(filepath)));
                        }
                        ExplorerAction::NavigateToDir(_) => {
                            // Navigation handled inside explorer
                        }
                        ExplorerAction::RefreshStartMenu => {
                            // Apps folder was modified
                            self.pending_action = Some((topmost_idx, ScriptAction::RefreshStartMenu));
                        }
                        ExplorerAction::None => {}
                    }
                    
                    // Choose the most efficient redraw region based on the key
                    if key == '\n' || key == '\r' {
                        // Enter key - command executed, need full content redraw
                        self.dirty_regions.push(DirtyRegion::ContentOnly(topmost_idx));
                    } else {
                        // Regular typing - only update the input/cursor line
                        self.dirty_regions.push(DirtyRegion::TypingOnly(topmost_idx));
                    }
                }
            }
        } else {
            // No windows open - silently ignore
        }
    }
    
    /// Route special key input to the focused (topmost) window
    pub fn handle_special_key_input(&mut self, key: SpecialKey) {
        // Send to the topmost visible window
        if self.window_count > 0 {
            let topmost_idx = self.z_order[self.window_count - 1];
            if let Some(window) = &mut self.windows[topmost_idx] {
                if window.visible {
                    window.handle_special_key(key);
                    // Special keys usually need content redraw (scrolling, selection, etc.)
                    self.dirty_regions.push(DirtyRegion::ContentOnly(topmost_idx));
                }
            }
        }
    }
    
    /// Draw the initial XOR outline (called after cursor is hidden)
    pub fn draw_initial_outline(&mut self, screen: &mut Screen) {
        if let Some(rect) = self.drag.outline_rect {
            draw_xor_outline(screen, &rect);
        }
    }
    
    /// Render all windows
    pub fn render(&mut self, screen: &mut Screen) {
        // Render in z-order (back to front)
        for zi in 0..self.window_count {
            let i = self.z_order[zi];
            if let Some(window) = &mut self.windows[i] {
                window.render(screen);
            }
        }
    }
    
    /// Take accumulated dirty regions and clear them
    pub fn take_dirty_regions(&mut self) -> alloc::vec::Vec<DirtyRegion> {
        core::mem::take(&mut self.dirty_regions)
    }
    
    /// Check if there are pending dirty regions or label updates
    pub fn has_dirty_regions(&self) -> bool {
        !self.dirty_regions.is_empty() || !self.label_update_windows.is_empty()
    }
    
    /// Check if there are pending label-only updates
    pub fn has_label_updates(&self) -> bool {
        !self.label_update_windows.is_empty()
    }
    
    /// Render only the dynamic labels for windows that need updates
    /// This is flicker-free because it doesn't clear the background
    pub fn render_label_updates(&mut self, screen: &mut Screen) {
        let windows_to_update = core::mem::take(&mut self.label_update_windows);
        for window_idx in windows_to_update {
            if let Some(window) = &self.windows[window_idx] {
                if window.visible {
                    window.render_dynamic_labels(screen);
                }
            }
        }
    }
    
    /// Render only windows affected by dirty regions
    /// Uses DirtyRegion enum for smarter partial updates:
    /// - TypingOnly: Only re-render the input/cursor line (most efficient)
    /// - ContentOnly: Only re-render the content area (efficient for keyboard input)
    /// - FullWindow: Re-render the entire window (for resize, move, etc.)
    /// - RectFromWindow: Old position of moved window - only redraw windows BELOW source in z-order
    /// - Rect: Arbitrary rectangle area - redraw all overlapping windows
    pub fn render_dirty_regions(&mut self, screen: &mut Screen, dirty: &[DirtyRegion]) {
        // First, handle any label-only updates (flicker-free)
        self.render_label_updates(screen);
        
        // Track which windows have been rendered to avoid duplicate renders
        let mut rendered_full: [bool; MAX_WINDOWS] = [false; MAX_WINDOWS];
        let mut rendered_content: [bool; MAX_WINDOWS] = [false; MAX_WINDOWS];
        let mut rendered_typing: [bool; MAX_WINDOWS] = [false; MAX_WINDOWS];
        
        // Process dirty regions
        for region in dirty {
            match region {
                DirtyRegion::TypingOnly(window_idx) => {
                    // Most efficient - only render the typing/input line
                    if !rendered_full[*window_idx] && !rendered_content[*window_idx] && !rendered_typing[*window_idx] {
                        if let Some(window) = &mut self.windows[*window_idx] {
                            if window.visible {
                                window.render_typing_area_only(screen);
                                rendered_typing[*window_idx] = true;
                            }
                        }
                    }
                }
                DirtyRegion::ContentOnly(window_idx) => {
                    // Only render the content area of this window
                    if !rendered_full[*window_idx] && !rendered_content[*window_idx] {
                        if let Some(window) = &mut self.windows[*window_idx] {
                            if window.visible {
                                window.render_content_only(screen);
                                rendered_content[*window_idx] = true;
                            }
                        }
                    }
                }
                DirtyRegion::FullWindow(window_idx) => {
                    // Render the entire window
                    if !rendered_full[*window_idx] {
                        if let Some(window) = &mut self.windows[*window_idx] {
                            if window.visible {
                                window.render(screen);
                                rendered_full[*window_idx] = true;
                            }
                        }
                    }
                }
                DirtyRegion::RectFromWindow(rect, source_window_id) => {
                    // Only redraw windows that are BELOW the source window in z-order
                    // Find the z-index of the source window
                    let mut source_z_idx = 0;
                    for zi in 0..self.window_count {
                        if self.z_order[zi] == *source_window_id {
                            source_z_idx = zi;
                            break;
                        }
                    }
                    
                    // Render windows below source in z-order that overlap the rect
                    for zi in 0..source_z_idx {
                        let i = self.z_order[zi];
                        if !rendered_full[i] {
                            if let Some(window) = &mut self.windows[i] {
                                if window.visible && window.bounds.intersects(rect) {
                                    window.render(screen);
                                    rendered_full[i] = true;
                                }
                            }
                        }
                    }
                }
                DirtyRegion::Rect(rect) => {
                    // Re-render any visible window that intersects this rect
                    // Render in z-order (back to front)
                    for zi in 0..self.window_count {
                        let i = self.z_order[zi];
                        if !rendered_full[i] {
                            if let Some(window) = &mut self.windows[i] {
                                if window.visible && window.bounds.intersects(rect) {
                                    window.render(screen);
                                    rendered_full[i] = true;
                                }
                            }
                        }
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
