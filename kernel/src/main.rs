#![no_std]
#![no_main]

mod heap;
mod display_driver;
mod utils;

extern crate alloc;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use heap::init_heap;
use display_driver::display::init_screen;
use display_driver::bitmap::Bitmap;
use utils::icons::house::get_house_icon;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    init_heap();
    let mut screen = init_screen(boot_info);

    screen.clear_screen(0x454545);
    screen.draw_bitmap(30, 30, &get_house_icon());

    screen.draw_bitmap(60, 30, &get_house_icon());

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