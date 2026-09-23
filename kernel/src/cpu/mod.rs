pub mod gdt;
pub mod idt;
pub mod info;

pub fn init() {
    gdt::init();
    idt::init();
}
