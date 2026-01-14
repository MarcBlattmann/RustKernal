pub mod pic;
pub mod apic;
pub mod timer;
pub mod keyboard;
pub mod mouse;
pub mod display;
pub mod filesystem;
pub mod ata;
pub mod drives;

pub fn init() {
    pic::init();
    timer::init();
    
    if ata::init().is_ok() {
        let _ = drives::init();  // Initialize multi-drive support
    }
}

