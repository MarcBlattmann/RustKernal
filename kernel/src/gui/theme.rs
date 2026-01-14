//! Theme Module - Centralized colors and styling
//!
//! Change these values to restyle the entire GUI
//! Note: Colors must have alpha = 0xFF or they will be transparent

/// Background color (black)
pub const COLOR_BACKGROUND: u32 = 0xFF000000;

/// Primary foreground color (white)
pub const COLOR_FOREGROUND: u32 = 0xFFFFFFFF;

/// Window background
pub const COLOR_WINDOW_BG: u32 = 0xFF000000;

/// Window border
pub const COLOR_WINDOW_BORDER: u32 = 0xFFFFFFFF;

/// Title bar background
pub const COLOR_TITLEBAR_BG: u32 = 0xFF000000;

/// Title bar text
pub const COLOR_TITLEBAR_TEXT: u32 = 0xFFFFFFFF;

/// Button background
pub const COLOR_BUTTON_BG: u32 = 0xFF000000;

/// Button border
pub const COLOR_BUTTON_BORDER: u32 = 0xFFFFFFFF;

/// Button text
pub const COLOR_BUTTON_TEXT: u32 = 0xFFFFFFFF;

/// Taskbar background
pub const COLOR_TASKBAR_BG: u32 = 0xFF111111;

/// Title bar highlight / selection color
pub const COLOR_TITLEBAR: u32 = 0xFF333366;

/// Menu highlight
pub const COLOR_HIGHLIGHT: u32 = 0xFF444488;

/// Cursor color
pub const COLOR_CURSOR: u32 = 0xFFFFFFFF;

/// Dimensions
pub const TITLEBAR_HEIGHT: usize = 24;
pub const BORDER_WIDTH: usize = 1;
pub const TASKBAR_HEIGHT: usize = 32;
pub const BUTTON_SIZE: usize = 16;
