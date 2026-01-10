pub mod pic;
pub mod apic;
pub mod timer;
pub mod keyboard;
pub mod display;

pub fn init() {
    pic::init();
    timer::init();
}
