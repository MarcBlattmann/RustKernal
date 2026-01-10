use spin::Mutex;

const KEYBOARD_DATA_PORT: u16 = 0x60;

static INPUT_BUFFER: Mutex<InputBuffer> = Mutex::new(InputBuffer::new());

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
    if let Some(character) = scancode_to_char(scancode) {
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

fn scancode_to_char(scancode: u8) -> Option<u8> {
    // Only handle key press (ignore key release - high bit set)
    if scancode & 0x80 != 0 {
        return None;
    }
    
    // Swiss keyboard layout (Set 1 scancodes)
    // Note: Swiss layout is similar to QWERTZ with special characters
    let character = match scancode {
        // Numbers row (without shift)
        0x02 => b'1', 0x03 => b'2', 0x04 => b'3', 0x05 => b'4', 0x06 => b'5',
        0x07 => b'6', 0x08 => b'7', 0x09 => b'8', 0x0A => b'9', 0x0B => b'0',
        
        // Top letter row (q w e r t z u i o p) - note Z and Y swapped
        0x10 => b'q', 0x11 => b'w', 0x12 => b'e', 0x13 => b'r', 0x14 => b't',
        0x15 => b'z', 0x16 => b'u', 0x17 => b'i', 0x18 => b'o', 0x19 => b'p',
        
        // Middle letter row (a s d f g h j k l)
        0x1E => b'a', 0x1F => b's', 0x20 => b'd', 0x21 => b'f', 0x22 => b'g',
        0x23 => b'h', 0x24 => b'j', 0x25 => b'k', 0x26 => b'l',
        
        // Bottom letter row (y x c v b n m) - Y instead of Z
        0x2C => b'y', 0x2D => b'x', 0x2E => b'c', 0x2F => b'v', 0x30 => b'b',
        0x31 => b'n', 0x32 => b'm',
        
        0x39 => b' ',  // Space
        0x1C => b'\n', // Enter
        0x0E => 0x08,  // Backspace (ASCII backspace)
        
        _ => return None,
    };
    
    Some(character)
}
