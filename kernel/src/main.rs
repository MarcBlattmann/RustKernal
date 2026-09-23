#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

mod memory;
mod cpu;
mod drivers;
mod shell;
mod utils;
mod demo;

extern crate alloc;

use core::panic::PanicInfo;
use bootloader_api::config::Mapping;
use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use memory::init_heap;

/// The display driver maps video memory itself, which needs access to physical memory.
static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    cpu::init();
    drivers::init();
    init_heap();

    let physical_memory_offset = boot_info.physical_memory_offset.into_option().expect("physical memory is not mapped");
    unsafe { memory::paging::init(physical_memory_offset, &boot_info.memory_regions) };

    let display = drivers::display::init(boot_info.framebuffer.take());
    shell::run(display)
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("KERNEL PANIC: {}", info);
    loop {
        core::hint::spin_loop();
    }
}
