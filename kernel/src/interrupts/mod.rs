pub mod gdt;
pub mod idt;
pub mod pic;
pub mod timer;
mod apic;

pub fn init() {
    gdt::init();
    idt::init();
    pic::init();
    timer::init();
}