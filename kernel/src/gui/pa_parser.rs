//! Pursuit App (.pa) File Parser
//!
//! A simple XML-like markup language for defining apps declaratively.
//!
//! ## Syntax Example:
//! ```xml
//! <app title="My App" width="300" height="200" x="100" y="100">
//!     <label x="20" y="40">Hello World!</label>
//!     <button x="20" y="80" width="100" height="30">Click Me</button>
//!     <textbox x="20" y="120" width="200" height="25"/>
//!     <panel x="20" y="160" width="200" height="100"/>
//! </app>
//! ```

use alloc::string::String;
use alloc::format;
use super::app::{AppDef, Element};

/// Parse a .pa file content into an AppDef
pub fn parse_pa(content: &str) -> Result<AppDef, ParseError> {
    let mut parser = PaParser::new(content);
    parser.parse()
}

/// Parse error types
#[derive(Debug)]
pub enum ParseError {
    UnexpectedEnd,
    ExpectedTag,
    ExpectedAttribute,
    InvalidNumber,
    MissingAttribute(&'static str),
    UnknownTag,
    MismatchedClosingTag,
    NotFound,
}

impl ParseError {
    /// Convert error to a human-readable message
    pub fn to_message(&self) -> String {
        match self {
            ParseError::UnexpectedEnd => String::from("Unexpected end of file"),
            ParseError::ExpectedTag => String::from("Expected a tag (e.g., <button>)"),
            ParseError::ExpectedAttribute => String::from("Expected attribute (name=\"value\")"),
            ParseError::InvalidNumber => String::from("Invalid number in attribute"),
            ParseError::MissingAttribute(attr) => format!("Missing required attribute: {}", attr),
            ParseError::UnknownTag => String::from("Unknown tag encountered"),
            ParseError::MismatchedClosingTag => String::from("Mismatched closing tag"),
            ParseError::NotFound => String::from("App not found"),
        }
    }
}

pub fn create_error_app(app_id: &str, error: &ParseError) -> AppDef {
    let error_msg = error.to_message();
    AppDef::new("Parse Error")
        .size(350, 150)
        .position(150, 150)
        .element(Element::VBox { 
            padding: 10, 
            gap: 5, 
            children: alloc::vec![
                Element::Label { 
                    text: format!("Failed to load: {}", app_id), 
                    x: 0, 
                    y: 0 
                },
                Element::Label { 
                    text: error_msg, 
                    x: 0, 
                    y: 0 
                },
                Element::Spacer,
                Element::HBox {
                    padding: 0,
                    gap: 5,
                    children: alloc::vec![
                        Element::Spacer,
                        Element::Button {
                            text: String::from("OK"),
                            x: 0,
                            y: 0,
                            width: 80,
                            height: 25,
                            on_click: Some(String::from("close()")),
                        },
                    ],
                },
            ],
        })
}

/// Simple PA format parser
struct PaParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> PaParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn parse(&mut self) -> Result<AppDef, ParseError> {
        self.skip_whitespace();
        
        // Expect <app ...>
        if !self.consume("<app") {
            return Err(ParseError::ExpectedTag);
        }
        
        // Parse app attributes
        let mut title = String::from("Untitled");
        let mut width: usize = 300;
        let mut height: usize = 200;
        let mut x: i32 = 100;
        let mut y: i32 = 100;
        
        // Parse attributes until >
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') {
                self.advance();
                break;
            }
            
            if self.peek() == Some('/') {
                // Self-closing tag
                self.advance();
                if self.peek() == Some('>') {
                    self.advance();
                    return Ok(AppDef::new(&title).size(width, height).position(x, y));
                }
            }
            
            // Parse attribute name
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                return Err(ParseError::ExpectedAttribute);
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "title" => title = attr_value,
                "width" => width = attr_value.parse().map_err(|_| ParseError::InvalidNumber)?,
                "height" => height = attr_value.parse().map_err(|_| ParseError::InvalidNumber)?,
                "x" => x = attr_value.parse().map_err(|_| ParseError::InvalidNumber)?,
                "y" => y = attr_value.parse().map_err(|_| ParseError::InvalidNumber)?,
                _ => {} // Ignore unknown attributes
            }
        }
        
        let mut app = AppDef::new(&title).size(width, height).position(x, y);
        
        // Parse child elements until </app>
        loop {
            self.skip_whitespace();
            
            if self.consume("</app>") {
                break;
            }
            
            if self.peek() != Some('<') {
                if self.pos >= self.input.len() {
                    return Err(ParseError::UnexpectedEnd);
                }
                self.advance();
                continue;
            }
            
            // Check for script tag specially
            if self.peek_str("<script") {
                self.consume("<script");
                self.skip_until('>');
                self.advance();
                let script_content = self.parse_text_until_str("</script>");
                self.consume("</script>");
                app.script = Some(script_content);
                continue;
            }
            
            // Parse child element
            if let Some(element) = self.parse_element()? {
                app = app.element(element);
            }
        }
        
        Ok(app)
    }
    
    fn parse_element(&mut self) -> Result<Option<Element>, ParseError> {
        if !self.consume("<") {
            return Ok(None);
        }
        
        let tag_name = self.parse_identifier()?;
        
        match tag_name.as_str() {
            "label" => self.parse_label(),
            "button" => self.parse_button(),
            "textbox" => self.parse_textbox(),
            "panel" => self.parse_panel(),
            "text" => self.parse_label(), // Alias for label
            "btn" => self.parse_button(), // Alias for button
            "input" => self.parse_textbox(), // Alias for textbox
            "vbox" => self.parse_vbox(),
            "hbox" => self.parse_hbox(),
            "spacer" => self.parse_spacer(),
            "script" => {
                // Script tag is handled separately in parse(), skip content here
                self.skip_until_str("</script>");
                self.consume("</script>");
                Ok(None)
            }
            _ => {
                // Skip unknown tags
                self.skip_until('>');
                self.advance();
                Ok(None)
            }
        }
    }
    
    fn parse_vbox(&mut self) -> Result<Option<Element>, ParseError> {
        let mut padding: usize = 0;
        let mut gap: usize = 5;
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') {
                self.advance();
                break;
            }
            
            if self.peek() == Some('/') {
                self.advance();
                if self.consume(">") {
                    return Ok(Some(Element::VBox { padding, gap, children: alloc::vec::Vec::new() }));
                }
            }
            
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                continue;
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "padding" => padding = attr_value.parse().unwrap_or(0),
                "gap" => gap = attr_value.parse().unwrap_or(5),
                _ => {}
            }
        }
        
        // Parse children until </vbox>
        let mut children = alloc::vec::Vec::new();
        loop {
            self.skip_whitespace();
            
            if self.consume("</vbox>") || self.consume("</VBox>") {
                break;
            }
            
            if self.peek() != Some('<') {
                if self.pos >= self.input.len() {
                    break;
                }
                self.advance();
                continue;
            }
            
            if let Some(element) = self.parse_element()? {
                children.push(element);
            }
        }
        
        Ok(Some(Element::VBox { padding, gap, children }))
    }
    
    fn parse_hbox(&mut self) -> Result<Option<Element>, ParseError> {
        let mut padding: usize = 0;
        let mut gap: usize = 5;
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') {
                self.advance();
                break;
            }
            
            if self.peek() == Some('/') {
                self.advance();
                if self.consume(">") {
                    return Ok(Some(Element::HBox { padding, gap, children: alloc::vec::Vec::new() }));
                }
            }
            
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                continue;
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "padding" => padding = attr_value.parse().unwrap_or(0),
                "gap" => gap = attr_value.parse().unwrap_or(5),
                _ => {}
            }
        }
        
        // Parse children until </hbox>
        let mut children = alloc::vec::Vec::new();
        loop {
            self.skip_whitespace();
            
            if self.consume("</hbox>") || self.consume("</HBox>") {
                break;
            }
            
            if self.peek() != Some('<') {
                if self.pos >= self.input.len() {
                    break;
                }
                self.advance();
                continue;
            }
            
            if let Some(element) = self.parse_element()? {
                children.push(element);
            }
        }
        
        Ok(Some(Element::HBox { padding, gap, children }))
    }
    
    fn parse_spacer(&mut self) -> Result<Option<Element>, ParseError> {
        // Skip any attributes and close tag
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') {
                self.advance();
                break;
            }
            
            if self.peek() == Some('/') {
                self.advance();
                if self.consume(">") {
                    return Ok(Some(Element::Spacer));
                }
            }
            
            self.advance();
        }
        
        // Check for </spacer> closing tag
        self.skip_whitespace();
        let _ = self.consume("</spacer>");
        
        Ok(Some(Element::Spacer))
    }
    
    fn parse_label(&mut self) -> Result<Option<Element>, ParseError> {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') {
                self.advance();
                break;
            }
            
            if self.peek() == Some('/') {
                self.advance();
                if self.consume(">") {
                    return Ok(Some(Element::Label { 
                        text: String::new(), 
                        x, 
                        y 
                    }));
                }
            }
            
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                continue;
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "x" => x = attr_value.parse().unwrap_or(0),
                "y" => y = attr_value.parse().unwrap_or(0),
                _ => {}
            }
        }
        
        // Parse text content
        let text = self.parse_text_until('<');
        
        // Skip closing tag
        self.skip_until('>');
        self.advance();
        
        Ok(Some(Element::Label { text, x, y }))
    }
    
    fn parse_button(&mut self) -> Result<Option<Element>, ParseError> {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut width: usize = 80;
        let mut height: usize = 30;
        let mut on_click: Option<String> = None;
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') {
                self.advance();
                break;
            }
            
            if self.peek() == Some('/') {
                self.advance();
                if self.consume(">") {
                    return Ok(Some(Element::Button { 
                        text: String::from("Button"), 
                        x, y, width, height, on_click
                    }));
                }
            }
            
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                continue;
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "x" => x = attr_value.parse().unwrap_or(0),
                "y" => y = attr_value.parse().unwrap_or(0),
                "width" => width = attr_value.parse().unwrap_or(80),
                "height" => height = attr_value.parse().unwrap_or(30),
                "on_click" | "onclick" => on_click = Some(attr_value),
                _ => {}
            }
        }
        
        // Parse text content (button label)
        let text = self.parse_text_until('<');
        let text = if text.is_empty() { String::from("Button") } else { text };
        
        // Skip closing tag
        self.skip_until('>');
        self.advance();
        
        Ok(Some(Element::Button { text, x, y, width, height, on_click }))
    }
    
    fn parse_textbox(&mut self) -> Result<Option<Element>, ParseError> {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut width: usize = 150;
        let mut height: usize = 25;
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') || self.peek() == Some('/') {
                if self.peek() == Some('/') {
                    self.advance();
                }
                if self.peek() == Some('>') {
                    self.advance();
                }
                break;
            }
            
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                continue;
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "x" => x = attr_value.parse().unwrap_or(0),
                "y" => y = attr_value.parse().unwrap_or(0),
                "width" => width = attr_value.parse().unwrap_or(150),
                "height" => height = attr_value.parse().unwrap_or(25),
                _ => {}
            }
        }
        
        // Skip to end of element if not self-closing
        if self.peek() == Some('<') {
            self.skip_until('>');
            self.advance();
        }
        
        Ok(Some(Element::TextBox { x, y, width, height }))
    }
    
    fn parse_panel(&mut self) -> Result<Option<Element>, ParseError> {
        let mut x: i32 = 0;
        let mut y: i32 = 0;
        let mut width: usize = 100;
        let mut height: usize = 100;
        
        // Parse attributes
        loop {
            self.skip_whitespace();
            
            if self.peek() == Some('>') || self.peek() == Some('/') {
                if self.peek() == Some('/') {
                    self.advance();
                }
                if self.peek() == Some('>') {
                    self.advance();
                }
                break;
            }
            
            let attr_name = self.parse_identifier()?;
            self.skip_whitespace();
            
            if !self.consume("=") {
                continue;
            }
            
            self.skip_whitespace();
            let attr_value = self.parse_quoted_string()?;
            
            match attr_name.as_str() {
                "x" => x = attr_value.parse().unwrap_or(0),
                "y" => y = attr_value.parse().unwrap_or(0),
                "width" => width = attr_value.parse().unwrap_or(100),
                "height" => height = attr_value.parse().unwrap_or(100),
                _ => {}
            }
        }
        
        // Skip to end of element if not self-closing
        if self.peek() == Some('<') {
            self.skip_until('>');
            self.advance();
        }
        
        Ok(Some(Element::Panel { x, y, width, height }))
    }
    
    // Helper methods
    
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    
    fn advance(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }
    
    fn skip_whitespace(&mut self) {
        loop {
            // Skip regular whitespace
            while let Some(c) = self.peek() {
                if c.is_whitespace() {
                    self.advance();
                } else {
                    break;
                }
            }
            
            // Skip XML comments <!-- ... -->
            if self.input[self.pos..].starts_with("<!--") {
                self.pos += 4; // Skip "<!--"
                while self.pos < self.input.len() {
                    if self.input[self.pos..].starts_with("-->") {
                        self.pos += 3; // Skip "-->"
                        break;
                    }
                    self.advance();
                }
            } else {
                break;
            }
        }
    }
    
    fn skip_until(&mut self, target: char) {
        while let Some(c) = self.peek() {
            if c == target {
                break;
            }
            self.advance();
        }
    }
    
    fn skip_until_str(&mut self, target: &str) {
        while self.pos < self.input.len() {
            if self.input[self.pos..].starts_with(target) {
                break;
            }
            self.advance();
        }
    }
    
    fn peek_str(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }
    
    fn parse_text_until_str(&mut self, stop: &str) -> String {
        let mut result = String::new();
        
        while self.pos < self.input.len() {
            if self.input[self.pos..].starts_with(stop) {
                break;
            }
            if let Some(c) = self.peek() {
                result.push(c);
                self.advance();
            } else {
                break;
            }
        }
        
        result.trim().into()
    }
    
    fn consume(&mut self, s: &str) -> bool {
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }
    
    fn parse_identifier(&mut self) -> Result<String, ParseError> {
        let mut result = String::new();
        
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                result.push(c);
                self.advance();
            } else {
                break;
            }
        }
        
        if result.is_empty() {
            Err(ParseError::ExpectedAttribute)
        } else {
            Ok(result)
        }
    }
    
    fn parse_quoted_string(&mut self) -> Result<String, ParseError> {
        let quote = self.peek().ok_or(ParseError::UnexpectedEnd)?;
        
        if quote != '"' && quote != '\'' {
            return Err(ParseError::ExpectedAttribute);
        }
        
        self.advance(); // Skip opening quote
        
        let mut result = String::new();
        
        while let Some(c) = self.peek() {
            if c == quote {
                self.advance(); // Skip closing quote
                return Ok(result);
            }
            result.push(c);
            self.advance();
        }
        
        Err(ParseError::UnexpectedEnd)
    }
    
    fn parse_text_until(&mut self, stop: char) -> String {
        let mut result = String::new();
        
        while let Some(c) = self.peek() {
            if c == stop {
                break;
            }
            result.push(c);
            self.advance();
        }
        
        // Trim whitespace
        result.trim().into()
    }
}

/// Helper macro to embed .pa files at compile time
#[macro_export]
macro_rules! include_pa {
    ($path:expr) => {
        $crate::gui::pa_parser::parse_pa(include_str!($path))
    };
}

// ============================================================
// Auto-generated app definitions from kernel/apps/*.pa
// Just add .pa files to the apps folder - they're detected automatically!
// ============================================================

// Include the auto-generated app definitions from build.rs
include!(concat!(env!("OUT_DIR"), "/apps_generated.rs"));

// ============================================================
// Helper functions to load apps from .pa format
// ============================================================

/// Load an app by its ID (filename without extension)
pub fn load_app(id: &str) -> Result<AppDef, ParseError> {
    if let Some(content) = load_app_by_id(id) {
        parse_pa(content)
    } else {
        Err(ParseError::NotFound)
    }
}

/// Load welcome app from .pa format
pub fn load_welcome_app() -> Result<AppDef, ParseError> {
    load_app("welcome")
}

/// Load about app from .pa format
pub fn load_about_app() -> Result<AppDef, ParseError> {
    load_app("about")
}

/// Load settings app from .pa format
pub fn load_settings_app() -> Result<AppDef, ParseError> {
    load_app("settings_flex")
}

/// Load notepad app from .pa format
pub fn load_notepad_app() -> Result<AppDef, ParseError> {
    load_app("notepad")
}

/// Load calculator app from .pa format
pub fn load_calculator_app() -> Result<AppDef, ParseError> {
    load_app("calculator")
}
