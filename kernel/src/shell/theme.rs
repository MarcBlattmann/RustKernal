//! Terminal color palette (0xAARRGGBB).

pub const BACKGROUND: u32 = 0xFF0F1117;
pub const FOREGROUND: u32 = 0xFFD4D8E0;
pub const MUTED: u32 = 0xFF6C7386;
pub const ACCENT: u32 = 0xFF6CB6FF;
pub const GREEN: u32 = 0xFF8BD17C;
pub const YELLOW: u32 = 0xFFE5C07B;
pub const RED: u32 = 0xFFF07178;
pub const MAGENTA: u32 = 0xFFC792EA;
pub const CYAN: u32 = 0xFF5FD7D7;
pub const CURSOR: u32 = 0xFFD4D8E0;

/// Colors selectable with the `color` command.
pub const NAMED_COLORS: [(&str, u32); 8] = [
    ("white", FOREGROUND),
    ("gray", MUTED),
    ("red", RED),
    ("green", GREEN),
    ("yellow", YELLOW),
    ("blue", ACCENT),
    ("magenta", MAGENTA),
    ("cyan", CYAN),
];
