//! PS/2 Mouse Driver

use x86_64::instructions::port::Port;
use core::sync::atomic::{AtomicI32, AtomicU8, AtomicBool, Ordering};

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const CMD_PORT: u16 = 0x64;

// Atomic mouse state - no locks needed
static MOUSE_X: AtomicI32 = AtomicI32::new(400);
static MOUSE_Y: AtomicI32 = AtomicI32::new(300);
static MOUSE_LEFT: AtomicBool = AtomicBool::new(false);
static MOUSE_RIGHT: AtomicBool = AtomicBool::new(false);
static MAX_X: AtomicI32 = AtomicI32::new(800);
static MAX_Y: AtomicI32 = AtomicI32::new(600);
static PACKET_IDX: AtomicU8 = AtomicU8::new(0);
static PACKET_BUF: [AtomicU8; 3] = [AtomicU8::new(0), AtomicU8::new(0), AtomicU8::new(0)];

fn wait_write() {
    for _ in 0..5000 {
        unsafe {
            if Port::<u8>::new(STATUS_PORT).read() & 0x02 == 0 { return; }
        }
    }
}

fn wait_read() {
    for _ in 0..5000 {
        unsafe {
            if Port::<u8>::new(STATUS_PORT).read() & 0x01 != 0 { return; }
        }
    }
}

fn cmd(c: u8) {
    wait_write();
    unsafe { Port::<u8>::new(CMD_PORT).write(c); }
}

fn mouse_cmd(c: u8) {
    cmd(0xD4);
    wait_write();
    unsafe { Port::<u8>::new(DATA_PORT).write(c); }
    wait_read();
    unsafe { Port::<u8>::new(DATA_PORT).read(); }
}

pub fn init(w: u32, h: u32) {
    MAX_X.store(w as i32, Ordering::Relaxed);
    MAX_Y.store(h as i32, Ordering::Relaxed);
    MOUSE_X.store(w as i32 / 2, Ordering::Relaxed);
    MOUSE_Y.store(h as i32 / 2, Ordering::Relaxed);
    PACKET_IDX.store(0, Ordering::Relaxed);

    // Flush
    for _ in 0..20 {
        unsafe {
            if Port::<u8>::new(STATUS_PORT).read() & 0x01 != 0 {
                Port::<u8>::new(DATA_PORT).read();
            }
        }
    }

    cmd(0xA8); // Enable aux
    cmd(0x20); // Get status
    wait_read();
    let s = unsafe { Port::<u8>::new(DATA_PORT).read() };
    cmd(0x60);
    wait_write();
    unsafe { Port::<u8>::new(DATA_PORT).write(s | 0x02); }
    
    mouse_cmd(0xF6); // Defaults
    mouse_cmd(0xF4); // Enable

    // Flush again
    for _ in 0..20 {
        unsafe {
            if Port::<u8>::new(STATUS_PORT).read() & 0x01 != 0 {
                Port::<u8>::new(DATA_PORT).read();
            }
        }
    }
}

fn process_byte(byte: u8) {
    let idx = PACKET_IDX.load(Ordering::Relaxed);
    
    // First byte must have bit 3 set
    if idx == 0 && (byte & 0x08) == 0 {
        return;
    }
    
    PACKET_BUF[idx as usize].store(byte, Ordering::Relaxed);
    
    if idx == 2 {
        PACKET_IDX.store(0, Ordering::Relaxed);
        
        let b0 = PACKET_BUF[0].load(Ordering::Relaxed);
        let b1 = PACKET_BUF[1].load(Ordering::Relaxed);
        let b2 = PACKET_BUF[2].load(Ordering::Relaxed);
        
        // Skip overflow
        if b0 & 0xC0 != 0 { return; }

        MOUSE_LEFT.store(b0 & 0x01 != 0, Ordering::Relaxed);
        MOUSE_RIGHT.store(b0 & 0x02 != 0, Ordering::Relaxed);

        let mut dx = b1 as i32;
        let mut dy = b2 as i32;
        if b0 & 0x10 != 0 { dx -= 256; }
        if b0 & 0x20 != 0 { dy -= 256; }

        let max_x = MAX_X.load(Ordering::Relaxed);
        let max_y = MAX_Y.load(Ordering::Relaxed);
        
        let new_x = (MOUSE_X.load(Ordering::Relaxed) + dx).clamp(0, max_x - 1);
        let new_y = (MOUSE_Y.load(Ordering::Relaxed) - dy).clamp(0, max_y - 1);
        
        MOUSE_X.store(new_x, Ordering::Relaxed);
        MOUSE_Y.store(new_y, Ordering::Relaxed);
    } else {
        PACKET_IDX.store(idx + 1, Ordering::Relaxed);
    }
}

pub fn poll() {
    unsafe {
        let s = Port::<u8>::new(STATUS_PORT).read();
        if s & 0x01 != 0 {
            let d = Port::<u8>::new(DATA_PORT).read();
            if s & 0x20 != 0 {
                process_byte(d);
            }
        }
    }
}

pub fn reset_packets() {
    PACKET_IDX.store(0, Ordering::Relaxed);
}

pub fn get_position() -> (i32, i32) {
    (MOUSE_X.load(Ordering::Relaxed), MOUSE_Y.load(Ordering::Relaxed))
}

pub fn get_buttons() -> (bool, bool, bool) {
    (MOUSE_LEFT.load(Ordering::Relaxed), MOUSE_RIGHT.load(Ordering::Relaxed), false)
}

pub fn handle_interrupt() {
    unsafe {
        let d = Port::<u8>::new(DATA_PORT).read();
        process_byte(d);
    }
}
