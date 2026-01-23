use spin::Mutex;

const KEYBOARD_DATA_PORT: u16 = 0x60;

static INPUT_BUFFER: Mutex<InputBuffer> = Mutex::new(InputBuffer::new());

/// Keyboard state for modifier keys
static KEYBOARD_STATE: Mutex<KeyboardState> = Mutex::new(KeyboardState::new());

struct KeyboardState {
    shift_pressed: bool,
    ctrl_pressed: bool,
    alt_pressed: bool,
    altgr_pressed: bool,
}

impl KeyboardState {
    const fn new() -> Self {
        Self {
            shift_pressed: false,
            ctrl_pressed: false,
            alt_pressed: false,
            altgr_pressed: false,
        }
    }
}

/// Special keys that aren't printable characters
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SpecialKey {
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Delete,
    Escape,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
}

static SPECIAL_KEY_BUFFER: Mutex<SpecialKeyBuffer> = Mutex::new(SpecialKeyBuffer::new());

struct SpecialKeyBuffer {
    buffer: [Option<SpecialKey>; 16],
    write_position: usize,
    read_position: usize,
}

impl SpecialKeyBuffer {
    const fn new() -> Self {
        Self {
            buffer: [None; 16],
            write_position: 0,
            read_position: 0,
        }
    }
    
    fn push(&mut self, key: SpecialKey) {
        let next_position = (self.write_position + 1) % self.buffer.len();
        if next_position != self.read_position {
            self.buffer[self.write_position] = Some(key);
            self.write_position = next_position;
        }
    }
    
    fn pop(&mut self) -> Option<SpecialKey> {
        if self.read_position == self.write_position {
            None
        } else {
            let key = self.buffer[self.read_position].take();
            self.read_position = (self.read_position + 1) % self.buffer.len();
            key
        }
    }
}

struct InputBuffer {
    buffer: [u8; 256],
    write_position: usize,
    read_position: usize,
}

impl InputBuffer {
    const fn new() -> Self {
        Self {
            buffer: [0; 256],
            write_position: 0,
            read_position: 0,
        }
    }
    
    fn push(&mut self, character: u8) {
        let next_position = (self.write_position + 1) % self.buffer.len();
        if next_position != self.read_position {
            self.buffer[self.write_position] = character;
            self.write_position = next_position;
        }
    }
    
    fn pop(&mut self) -> Option<u8> {
        if self.read_position == self.write_position {
            None
        } else {
            let character = self.buffer[self.read_position];
            self.read_position = (self.read_position + 1) % self.buffer.len();
            Some(character)
        }
    }
}

pub fn handle_scancode(scancode: u8) {
    let is_release = scancode & 0x80 != 0;
    let key_code = scancode & 0x7F;
    
    // Handle modifier keys
    {
        let mut state = KEYBOARD_STATE.lock();
        match key_code {
            0x2A | 0x36 => { // Left Shift, Right Shift
                state.shift_pressed = !is_release;
                return;
            }
            0x1D => { // Left Ctrl (also Right Ctrl in some cases)
                state.ctrl_pressed = !is_release;
                return;
            }
            0x38 => { // Left Alt (AltGr on some layouts is Right Alt = 0xE0 0x38)
                // For Swiss German, Right Alt is AltGr
                state.alt_pressed = !is_release;
                return;
            }
            _ => {}
        }
    }
    
    // Only process key presses, not releases
    if is_release {
        return;
    }
    
    // Check for special keys first
    if let Some(special) = scancode_to_special(scancode) {
        SPECIAL_KEY_BUFFER.lock().push(special);
        return;
    }
    
    // Get modifier state
    let (shift, ctrl, alt) = {
        let state = KEYBOARD_STATE.lock();
        (state.shift_pressed, state.ctrl_pressed, state.alt_pressed)
    };
    
    // Handle Ctrl+key combos separately
    if ctrl {
        // Convert scancode to letter for Ctrl combos
        let letter = match key_code {
            0x1F => Some('s'), // S
            0x1E => Some('a'), // A
            0x2E => Some('c'), // C
            0x2F => Some('v'), // V
            0x2D => Some('x'), // X
            0x2C => Some('z'), // Z (or Y on Swiss)
            _ => None,
        };
        if let Some(c) = letter {
            CTRL_COMBO_BUFFER.lock().push(c);
            return;
        }
    }
    
    if let Some(character) = scancode_to_char_swiss(scancode, shift, alt) {
        INPUT_BUFFER.lock().push(character);
    }
}

pub fn read_scancode() -> u8 {
    unsafe {
        let scancode: u8;
        core::arch::asm!("in al, dx", in("dx") KEYBOARD_DATA_PORT, out("al") scancode);
        scancode
    }
}

pub fn try_read_char() -> Option<char> {
    INPUT_BUFFER.lock().pop().map(|b| b as char)
}

/// Check if Ctrl key is currently pressed
pub fn is_ctrl_pressed() -> bool {
    KEYBOARD_STATE.lock().ctrl_pressed
}

/// Buffer for Ctrl+key combos
static CTRL_COMBO_BUFFER: Mutex<CtrlComboBuffer> = Mutex::new(CtrlComboBuffer::new());

struct CtrlComboBuffer {
    buffer: [Option<char>; 16],
    write_position: usize,
    read_position: usize,
}

impl CtrlComboBuffer {
    const fn new() -> Self {
        Self {
            buffer: [None; 16],
            write_position: 0,
            read_position: 0,
        }
    }
    
    fn push(&mut self, key: char) {
        let next_position = (self.write_position + 1) % self.buffer.len();
        if next_position != self.read_position {
            self.buffer[self.write_position] = Some(key);
            self.write_position = next_position;
        }
    }
    
    fn pop(&mut self) -> Option<char> {
        if self.read_position == self.write_position {
            None
        } else {
            let key = self.buffer[self.read_position].take();
            self.read_position = (self.read_position + 1) % self.buffer.len();
            key
        }
    }
}

/// Try to read a Ctrl+key combo (e.g., Ctrl+S)
pub fn try_read_ctrl_combo() -> Option<char> {
    CTRL_COMBO_BUFFER.lock().pop()
}

/// Try to read a special key from the buffer
pub fn try_read_special_key() -> Option<SpecialKey> {
    SPECIAL_KEY_BUFFER.lock().pop()
}

/// Convert scancode to special key
fn scancode_to_special(scancode: u8) -> Option<SpecialKey> {
    // Only handle key press (ignore key release - high bit set)
    if scancode & 0x80 != 0 {
        return None;
    }
    
    match scancode {
        // Arrow keys (these are extended scancodes, but QEMU sends simple ones)
        0x48 => Some(SpecialKey::Up),
        0x50 => Some(SpecialKey::Down),
        0x4B => Some(SpecialKey::Left),
        0x4D => Some(SpecialKey::Right),
        
        // Navigation keys
        0x49 => Some(SpecialKey::PageUp),
        0x51 => Some(SpecialKey::PageDown),
        0x47 => Some(SpecialKey::Home),
        0x4F => Some(SpecialKey::End),
        0x53 => Some(SpecialKey::Delete),
        
        // Function keys
        0x3B => Some(SpecialKey::F1),
        0x3C => Some(SpecialKey::F2),
        0x3D => Some(SpecialKey::F3),
        0x3E => Some(SpecialKey::F4),
        0x3F => Some(SpecialKey::F5),
        0x40 => Some(SpecialKey::F6),
        0x41 => Some(SpecialKey::F7),
        0x42 => Some(SpecialKey::F8),
        0x43 => Some(SpecialKey::F9),
        0x44 => Some(SpecialKey::F10),
        0x57 => Some(SpecialKey::F11),
        0x58 => Some(SpecialKey::F12),
        
        // Escape
        0x01 => Some(SpecialKey::Escape),
        
        _ => None,
    }
}

fn scancode_to_char_swiss(scancode: u8, shift: bool, altgr: bool) -> Option<u8> {
    // Swiss German QWERTZ keyboard layout (PS/2 Scan Set 1)
    // AltGr combinations for special characters like @, [, ], {, }, \, etc.
    
    let character = match scancode {
        // Numbers row: 1 2 3 4 5 6 7 8 9 0
        // Swiss: + " * ç % & / ( ) = with shift: 1 2 3 4 5 6 7 8 9 0
        // AltGr: | @ # ¬ ¦ ¢ ¦ ¦ ¦ ¦
        0x02 => if shift { b'!' } else if altgr { b'|' } else { b'1' },
        0x03 => if shift { b'"' } else if altgr { b'@' } else { b'2' },
        0x04 => if shift { b'#' } else { b'3' },
        0x05 => if shift { b'$' } else { b'4' },
        0x06 => if shift { b'%' } else { b'5' },
        0x07 => if shift { b'&' } else { b'6' },
        0x08 => if shift { b'/' } else if altgr { b'|' } else { b'7' },
        0x09 => if shift { b'(' } else { b'8' },
        0x0A => if shift { b')' } else { b'9' },
        0x0B => if shift { b'=' } else { b'0' },
        
        // Top letter row: Q W E R T Z U I O P
        0x10 => if shift { b'Q' } else { b'q' },
        0x11 => if shift { b'W' } else { b'w' },
        0x12 => if shift { b'E' } else if altgr { 0x80 } else { b'e' }, // AltGr+E = € (use placeholder)
        0x13 => if shift { b'R' } else { b'r' },
        0x14 => if shift { b'T' } else { b't' },
        0x15 => if shift { b'Z' } else { b'z' }, // Swiss has Z here
        0x16 => if shift { b'U' } else { b'u' },
        0x17 => if shift { b'I' } else { b'i' },
        0x18 => if shift { b'O' } else { b'o' },
        0x19 => if shift { b'P' } else { b'p' },
        
        // Middle letter row: A S D F G H J K L
        0x1E => if shift { b'A' } else { b'a' },
        0x1F => if shift { b'S' } else { b's' },
        0x20 => if shift { b'D' } else { b'd' },
        0x21 => if shift { b'F' } else { b'f' },
        0x22 => if shift { b'G' } else { b'g' },
        0x23 => if shift { b'H' } else { b'h' },
        0x24 => if shift { b'J' } else { b'j' },
        0x25 => if shift { b'K' } else { b'k' },
        0x26 => if shift { b'L' } else { b'l' },
        
        // Bottom letter row: Y X C V B N M (Swiss has Y and Z swapped)
        0x2C => if shift { b'Y' } else { b'y' }, // Swiss has Y here
        0x2D => if shift { b'X' } else { b'x' },
        0x2E => if shift { b'C' } else { b'c' },
        0x2F => if shift { b'V' } else { b'v' },
        0x30 => if shift { b'B' } else { b'b' },
        0x31 => if shift { b'N' } else { b'n' },
        0x32 => if shift { b'M' } else { b'm' },
        
        // Punctuation - Swiss German layout
        // Key after L: ö Ö é (scancode 0x27)
        0x27 => if shift { b':' } else { b';' },
        // Key after ö: ä Ä à (scancode 0x28) 
        0x28 => if shift { b'"' } else { b'\'' },
        // Key before 1: § ° (scancode 0x29)
        0x29 => if shift { b'~' } else { b'`' },
        
        // Key after P: ü è [ (scancode 0x1A)
        0x1A => if altgr { b'[' } else if shift { b'{' } else { b'[' },
        // Key after ü: ¨ ! ] (scancode 0x1B)  
        0x1B => if altgr { b']' } else if shift { b'}' } else { b']' },
        // Key after ä: $ £ } (scancode 0x2B)
        0x2B => if altgr { b'}' } else if shift { b'*' } else { b'\\' },
        
        // Comma, Period, Minus
        // Shift+comma = <, Shift+period = > (US-style, works better in QEMU)
        0x33 => if shift { b'<' } else if altgr { b'<' } else { b',' },    // , < <
        0x34 => if shift { b'>' } else if altgr { b'>' } else { b'.' },    // . > >
        0x35 => if shift { b'_' } else { b'-' },    // - _
        
        // Key next to right shift: < > | (scancode 0x56 - the 102nd key)
        0x56 => if altgr { b'\\' } else if shift { b'>' } else { b'<' },
        
        // Key left of backspace: ' ? ´
        0x0C => if shift { b'?' } else { b'\'' },
        // Key right of 0: ^ ` ~ (dead key)
        0x0D => if shift { b'`' } else { b'^' },
        
        0x39 => b' ',  // Space
        0x1C => b'\n', // Enter/Return
        0x0E => 0x08,  // Backspace (ASCII backspace character)
        0x0F => b'\t', // Tab
        
        _ => return None,
    };
    
    Some(character)
}
