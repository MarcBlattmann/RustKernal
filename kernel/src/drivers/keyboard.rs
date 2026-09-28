use spin::Mutex;

const KEYBOARD_DATA_PORT: u16 = 0x60;

const EXTENDED_PREFIX: u8 = 0xE0;
const RELEASE_BIT: u8 = 0x80;

const LEFT_SHIFT: u8 = 0x2A;
const RIGHT_SHIFT: u8 = 0x36;
const CONTROL: u8 = 0x1D;
const ALT: u8 = 0x38;
const CAPS_LOCK: u8 = 0x3A;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    Tab,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Delete,
    PageUp,
    PageDown,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
}

static KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard::new());

struct Keyboard {
    events: [Option<KeyEvent>; 64],
    write_position: usize,
    read_position: usize,
    extended: bool,
    left_shift: bool,
    right_shift: bool,
    ctrl: bool,
    alt_gr: bool,
    caps_lock: bool,
}

impl Keyboard {
    const fn new() -> Self {
        Self {
            events: [None; 64],
            write_position: 0,
            read_position: 0,
            extended: false,
            left_shift: false,
            right_shift: false,
            ctrl: false,
            alt_gr: false,
            caps_lock: false,
        }
    }

    fn push(&mut self, key: Key) {
        let next_position = (self.write_position + 1) % self.events.len();
        if next_position != self.read_position {
            let shift = self.left_shift || self.right_shift;
            // Windows hosts and some keyboards send AltGr as Ctrl+AltGr. A character typed with
            // AltGr (@ # | ~ [ ] { } \ on the Swiss layout) is text, not a Ctrl shortcut.
            let ctrl = self.ctrl && !(self.alt_gr && matches!(key, Key::Char(_)));
            self.events[self.write_position] = Some(KeyEvent { key, ctrl, shift });
            self.write_position = next_position;
        }
    }

    fn pop(&mut self) -> Option<KeyEvent> {
        if self.read_position == self.write_position {
            return None;
        }
        let event = self.events[self.read_position].take();
        self.read_position = (self.read_position + 1) % self.events.len();
        event
    }

    fn handle_scancode(&mut self, scancode: u8) {
        if scancode == EXTENDED_PREFIX {
            self.extended = true;
            return;
        }
        let extended = core::mem::replace(&mut self.extended, false);
        let released = scancode & RELEASE_BIT != 0;
        let code = scancode & !RELEASE_BIT;

        match (extended, code) {
            (false, LEFT_SHIFT) => self.left_shift = !released,
            (false, RIGHT_SHIFT) => self.right_shift = !released,
            (_, CONTROL) => self.ctrl = !released,
            (true, ALT) => self.alt_gr = !released,
            // Keyboards send fake shift codes around some extended keys; ignore them.
            (true, LEFT_SHIFT) | (true, RIGHT_SHIFT) => {}
            _ if released => {}
            (false, CAPS_LOCK) => self.caps_lock = !self.caps_lock,
            (true, code) => {
                if let Some(key) = extended_key(code) {
                    self.push(key);
                }
            }
            (false, code) => {
                if let Some(key) = self.basic_key(code) {
                    self.push(key);
                }
            }
        }
    }

    fn basic_key(&self, code: u8) -> Option<Key> {
        let key = match code {
            0x01 => Key::Escape,
            0x0E => Key::Backspace,
            0x0F => Key::Tab,
            0x1C => Key::Enter,
            _ => {
                let (normal, shifted, alt_gr) = swiss_layout(code)?;
                let shift = self.left_shift || self.right_shift;
                let character = if self.alt_gr {
                    alt_gr?
                } else if normal.is_alphabetic() && self.caps_lock != shift {
                    normal.to_uppercase().next().unwrap_or(normal)
                } else if shift {
                    shifted
                } else {
                    normal
                };
                Key::Char(character)
            }
        };
        Some(key)
    }
}

fn extended_key(code: u8) -> Option<Key> {
    Some(match code {
        0x1C => Key::Enter,
        0x48 => Key::Up,
        0x50 => Key::Down,
        0x4B => Key::Left,
        0x4D => Key::Right,
        0x47 => Key::Home,
        0x4F => Key::End,
        0x53 => Key::Delete,
        0x49 => Key::PageUp,
        0x51 => Key::PageDown,
        _ => return None,
    })
}

/// Swiss German layout (scancode set 1): (normal, with shift, with AltGr).
fn swiss_layout(code: u8) -> Option<(char, char, Option<char>)> {
    let keys = match code {
        0x29 => ('§', '°', None),
        0x02 => ('1', '+', None),
        0x03 => ('2', '"', Some('@')),
        0x04 => ('3', '*', Some('#')),
        0x05 => ('4', 'ç', None),
        0x06 => ('5', '%', None),
        0x07 => ('6', '&', None),
        0x08 => ('7', '/', Some('|')),
        0x09 => ('8', '(', None),
        0x0A => ('9', ')', None),
        0x0B => ('0', '=', None),
        0x0C => ('\'', '?', None),
        0x0D => ('^', '`', Some('~')),

        0x10 => ('q', 'Q', None), 0x11 => ('w', 'W', None), 0x12 => ('e', 'E', None),
        0x13 => ('r', 'R', None), 0x14 => ('t', 'T', None), 0x15 => ('z', 'Z', None),
        0x16 => ('u', 'U', None), 0x17 => ('i', 'I', None), 0x18 => ('o', 'O', None),
        0x19 => ('p', 'P', None),
        0x1A => ('ü', 'è', Some('[')),
        0x1B => ('¨', '!', Some(']')),

        0x1E => ('a', 'A', None), 0x1F => ('s', 'S', None), 0x20 => ('d', 'D', None),
        0x21 => ('f', 'F', None), 0x22 => ('g', 'G', None), 0x23 => ('h', 'H', None),
        0x24 => ('j', 'J', None), 0x25 => ('k', 'K', None), 0x26 => ('l', 'L', None),
        0x27 => ('ö', 'é', None),
        0x28 => ('ä', 'à', Some('{')),
        0x2B => ('$', '£', Some('}')),

        0x56 => ('<', '>', Some('\\')),
        0x2C => ('y', 'Y', None), 0x2D => ('x', 'X', None), 0x2E => ('c', 'C', None),
        0x2F => ('v', 'V', None), 0x30 => ('b', 'B', None), 0x31 => ('n', 'N', None),
        0x32 => ('m', 'M', None),
        0x33 => (',', ';', None),
        0x34 => ('.', ':', None),
        0x35 => ('-', '_', None),

        0x39 => (' ', ' ', None),
        _ => return None,
    };
    Some(keys)
}

/// Called from the keyboard interrupt handler.
pub fn handle_scancode(scancode: u8) {
    KEYBOARD.lock().handle_scancode(scancode);
}

pub fn read_scancode() -> u8 {
    unsafe {
        let scancode: u8;
        core::arch::asm!("in al, dx", in("dx") KEYBOARD_DATA_PORT, out("al") scancode);
        scancode
    }
}

/// Returns the next key press, if any. Interrupts are held off while the buffer is locked so the
/// keyboard interrupt can never wait on a lock held by the code it interrupted.
pub fn read_key() -> Option<KeyEvent> {
    x86_64::instructions::interrupts::without_interrupts(|| KEYBOARD.lock().pop())
}
