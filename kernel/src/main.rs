#![no_std]
#![no_main]

mod heap;
mod display_driver;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use heap::init_heap;
use display_driver::display::init_screen;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init_heap();
    let mut screen = init_screen(boot_info);

    screen.clear_screen("#59c5ffff");
    screen.write_pixel(10, 10, "#ff5634");

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