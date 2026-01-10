#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

mod heap;
mod display_driver;
mod utils;
mod console;
mod gdt;
mod idt;

extern crate alloc;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use heap::init_heap;
use display_driver::display::init_screen;
use console::Console;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    gdt::init();
    idt::init();
    init_heap();

    let mut screen = init_screen(boot_info);

    screen.clear_screen(0xFF000000);
    let mut console = Console::new(screen);

    console.print("Hello world from the kernel");

    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}