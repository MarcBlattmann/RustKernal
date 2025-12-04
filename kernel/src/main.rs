#![no_std]
#![no_main]

mod heap;
mod display_driver;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use heap::init_heap;
use display_driver::Display;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init_heap();
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let display = Display  {
            width: info.width,
            height: info.height,
            framebuffer: framebuffer.buffer_mut(),
            color_format: info.pixel_format,
        };
    }

    loop {
        core::hint::spin_loop();
    }
}

// Panic handler
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}