#![no_std]
#![no_main]

mod heap;
mod display_driver;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use heap::init_heap;
use display_driver::Screen;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init_heap();
    let framebuffer = boot_info.framebuffer.as_mut().expect("No framebuffer found");
    let info = framebuffer.info();
    let display = Screen::new(
        info.width,
        info.height,
        framebuffer.buffer_mut(),
        info.pixel_format,
    );

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