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
use shell::{Console, handle_command};
use alloc::string::String;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    cpu::init();
    drivers::init();
    init_heap();

    let mut screen = init_screen(boot_info);
    screen.clear_screen(0xFF000000);
    
    let mut console = Console::new(screen);
    console.print("Welcome to the rust kernel\n");
    console.print("Type 'help' for commands\n");
    console.print("> ");

    let mut input = String::new();

    loop {
        if let Some(c) = drivers::keyboard::try_read_char() {
            match c {
                '\n' => {
                    console.print("\n");
                    handle_command(&input, &mut console);
                    input.clear();
                    console.print("> ");
                }
                '\u{0008}' => {
                    if !input.is_empty() {
                        input.pop();
                        console.backspace();
                    }
                }
                _ => {
                    input.push(c);
                    console.print_char(c);
                }
            }
        }
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop { core::hint::spin_loop(); }
}
