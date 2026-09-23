pub mod pic;
pub mod apic;
pub mod timer;
pub mod keyboard;
pub mod display;
pub mod pci;
pub mod serial;
pub mod rtc;
pub mod power;

pub fn init() {
    serial::init();
    pic::init();
    timer::init();
    timer::calibrate();
}
