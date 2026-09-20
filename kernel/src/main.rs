#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

mod memory;
mod cpu;
mod drivers;
mod shell;
mod utils;

extern crate alloc;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use memory::init_heap;
use drivers::display::init_screen;
use alloc::string::String;
use shell::Console;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    cpu::init();
    drivers::init();
    init_heap();

    let mut screen = init_screen(boot_info);

    screen.clear_screen(0xFF000000);
    let mut console = Console::new(screen);

    console.print("Welcome to the rust kernel\n");
    console.print("> ");

    let mut command_contents = String::new();

    loop {
        if let Some(character) = drivers::keyboard::try_read_char() {
            if character == '\n' {
                if command_contents == "clear" {
                    console.clear();
                }
                console.print("\n> ");
                command_contents.clear();
            } else if character == ' ' {
                console.print(" ");
            } else if character == '\x08' {
                console.backspace();
            } else {
                let string = &alloc::format!("{}", character);
                command_contents.push(character);
                console.print(string);
            }
        }
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}