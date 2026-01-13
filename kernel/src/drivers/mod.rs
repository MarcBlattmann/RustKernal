pub mod pic;
pub mod apic;
pub mod timer;
pub mod keyboard;
pub mod display;
pub mod filesystem;
pub mod ata;

pub fn init() {
    pic::init();
    timer::init();
    
    if ata::init().is_ok() {
        let _ = filesystem::init();
    }
}
