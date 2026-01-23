//! App Builder - Declarative app/window creation system
//!
//! Example usage:
//! ```
//! let app = App::new("My App")
//!     .size(300, 200)
//!     .position(100, 100)
//!     .add(Label::new("Hello World").at(10, 10))
//!     .add(Button::new("Click Me").at(10, 50).on_click(|| { ... }))
//!     .build();
//! ```

use alloc::string::String;
use alloc::vec::Vec;
use super::widgets::Rect;

/// Element types that can be added to an app
#[derive(Clone)]
pub enum Element {
    /// Text label - displays static or dynamic text
    Label { text: String, x: i32, y: i32 },
    /// Clickable button with optional on_click handler
    Button { 
        text: String, 
        x: i32, 
        y: i32, 
        width: usize, 
        height: usize,
        /// Script code to execute on click
        on_click: Option<String>,
    },
    /// Text input box
    TextBox { x: i32, y: i32, width: usize, height: usize },
    /// Container panel
    Panel { x: i32, y: i32, width: usize, height: usize },
    /// Vertical box - stacks children vertically
    VBox { padding: usize, gap: usize, children: Vec<Element> },
    /// Horizontal box - stacks children horizontally  
    HBox { padding: usize, gap: usize, children: Vec<Element> },
    /// Spacer - takes remaining space in layout
    Spacer,
}

/// App definition - declarative window builder
pub struct AppDef {
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    pub elements: Vec<Element>,
    pub visible: bool,
    /// Script code to initialize variables and define functions
    pub script: Option<String>,
}

impl AppDef {
    /// Create a new app with title
    pub fn new(title: &str) -> Self {
        Self {
            title: String::from(title),
            x: 100,
            y: 100,
            width: 300,
            height: 200,
            elements: Vec::new(),
            visible: true,
            script: None,
        }
    }
    
    /// Set window size
    pub fn size(mut self, width: usize, height: usize) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    
    /// Set window position
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.x = x;
        self.y = y;
        self
    }
    
    /// Add a label
    pub fn label(mut self, text: &str, x: i32, y: i32) -> Self {
        self.elements.push(Element::Label {
            text: String::from(text),
            x,
            y,
        });
        self
    }
    
    /// Add a button
    pub fn button(mut self, text: &str, x: i32, y: i32, width: usize, height: usize) -> Self {
        self.elements.push(Element::Button {
            text: String::from(text),
            x,
            y,
            width,
            height,
            on_click: None,
        });
        self
    }
    
    /// Add a panel/box
    pub fn panel(mut self, x: i32, y: i32, width: usize, height: usize) -> Self {
        self.elements.push(Element::Panel { x, y, width, height });
        self
    }
    
    /// Add a textbox
    pub fn textbox(mut self, x: i32, y: i32, width: usize, height: usize) -> Self {
        self.elements.push(Element::TextBox { x, y, width, height });
        self
    }
    
    /// Add any element directly (used by parser)
    pub fn element(mut self, elem: Element) -> Self {
        self.elements.push(elem);
        self
    }
    
    /// Get bounds
    pub fn bounds(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

/// Create built-in apps
pub fn create_welcome_app() -> AppDef {
    AppDef::new("Welcome")
        .size(320, 200)
        .position(80, 80)
        .label("Welcome to Pursuit OS!", 20, 40)
        .label("A Rust-based operating system", 20, 60)
        .label("Drag windows by title bar", 20, 100)
        .label("Click X to close", 20, 120)
}

pub fn create_about_app() -> AppDef {
    AppDef::new("About")
        .size(280, 160)
        .position(420, 120)
        .label("Pursuit OS v0.1", 20, 40)
        .label("Written in Rust", 20, 60)
        .label("No unsafe dependencies", 20, 80)
}

pub fn create_file_manager_app() -> AppDef {
    AppDef::new("Files")
        .size(400, 300)
        .position(150, 100)
        .panel(10, 30, 380, 250)
        .label("File Explorer", 20, 40)
        .label("------------------------------", 20, 55)
        .label("[D] Documents/", 20, 75)
        .label("[D] Downloads/", 20, 95)
        .label("[D] Apps/", 20, 115)
        .label("[F] readme.txt", 20, 135)
        .label("[F] notes.txt", 20, 155)
        .label("------------------------------", 20, 175)
        .label("Press Enter to open", 20, 195)
        .label("Press N for new file", 20, 215)
}

/// Create the Code Editor app
pub fn create_code_editor_app() -> AppDef {
    AppDef::new("Code Editor")
        .size(600, 450)
        .position(100, 50)
}

/// Create the Terminal app
pub fn create_terminal_app() -> AppDef {
    AppDef::new("Terminal")
        .size(550, 380)
        .position(120, 80)
}

/// Create the Documentation viewer app
pub fn create_docs_app() -> AppDef {
    AppDef::new("Documentation")
        .size(500, 400)
        .position(180, 60)
}

/// Create the enhanced File Explorer app
pub fn create_explorer_app() -> AppDef {
    AppDef::new("File Explorer")
        .size(450, 350)
        .position(140, 90)
}
