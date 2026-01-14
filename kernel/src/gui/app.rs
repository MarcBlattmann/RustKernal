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
    Label { text: String, x: i32, y: i32 },
    Button { text: String, x: i32, y: i32, width: usize, height: usize },
    TextBox { x: i32, y: i32, width: usize, height: usize },
    Panel { x: i32, y: i32, width: usize, height: usize, color: u32 },
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
        });
        self
    }
    
    /// Add a panel/box
    pub fn panel(mut self, x: i32, y: i32, width: usize, height: usize, color: u32) -> Self {
        self.elements.push(Element::Panel { x, y, width, height, color });
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
        .size(350, 250)
        .position(200, 150)
        .panel(10, 30, 330, 200, 0xFF111111)
        .label("Documents/", 20, 50)
        .label("Downloads/", 20, 70)
        .label("readme.txt", 20, 90)
}
