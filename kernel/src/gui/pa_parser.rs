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
            _ => {
                // Skip unknown tags
                self.skip_until('>');
                self.advance();
                Ok(None)
            }
        }
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
                        x, y, width, height 
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
                _ => {}
            }
        }
        
        // Parse text content (button label)
        let text = self.parse_text_until('<');
        let text = if text.is_empty() { String::from("Button") } else { text };
        
        // Skip closing tag
        self.skip_until('>');
        self.advance();
        
        Ok(Some(Element::Button { text, x, y, width, height }))
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
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
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
// App definitions loaded from kernel/apps/ folder at compile time
// Edit the .pa files directly - they are the source of truth!
// ============================================================

/// Welcome app - loaded from kernel/apps/welcome.pa
pub const WELCOME_PA: &str = include_str!("../../apps/welcome.pa");

/// About app - loaded from kernel/apps/about.pa
pub const ABOUT_PA: &str = include_str!("../../apps/about.pa");

/// Settings app - loaded from kernel/apps/settings_flex.pa
pub const SETTINGS_PA: &str = include_str!("../../apps/settings_flex.pa");

/// Notepad app - loaded from kernel/apps/notepad.pa
pub const NOTEPAD_PA: &str = include_str!("../../apps/notepad.pa");

/// Calculator app - loaded from kernel/apps/calculator.pa
pub const CALCULATOR_PA: &str = include_str!("../../apps/calculator.pa");

// ============================================================
// Helper functions to load apps from .pa format
// ============================================================

/// Load welcome app from .pa format
pub fn load_welcome_app() -> Result<AppDef, ParseError> {
    parse_pa(WELCOME_PA)
}

/// Load about app from .pa format
pub fn load_about_app() -> Result<AppDef, ParseError> {
    parse_pa(ABOUT_PA)
}

/// Load settings app from .pa format
pub fn load_settings_app() -> Result<AppDef, ParseError> {
    parse_pa(SETTINGS_PA)
}

/// Load notepad app from .pa format
pub fn load_notepad_app() -> Result<AppDef, ParseError> {
    parse_pa(NOTEPAD_PA)
}

/// Load calculator app from .pa format
pub fn load_calculator_app() -> Result<AppDef, ParseError> {
    parse_pa(CALCULATOR_PA)
}
