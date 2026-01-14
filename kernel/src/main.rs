#![feature(abi_x86_interrupt)]
#![no_std]
#![no_main]

mod memory;
mod cpu;
mod drivers;
mod shell;
mod gui;
mod utils;

extern crate alloc;

use core::panic::PanicInfo;
use bootloader_api::{entry_point, BootInfo};
use memory::init_heap;
use drivers::display::init_screen;
use shell::{Console, Shell, handle_command};
use alloc::string::String;

static mut SCREEN_PTR: Option<*mut drivers::display::Screen> = None;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    cpu::init();
    init_heap();
    drivers::init();

    let mut screen = init_screen(boot_info);
    screen.clear_screen(0xFF000000);
    
    unsafe {
        SCREEN_PTR = Some(&mut screen as *mut _);
    }
    
    let mut console = Console::new(screen);
    let mut shell = Shell::new();
    
    console.print("Welcome to the Pursuit OS\n");
    console.print("Type 'help' for commands\n");
    console.print(&alloc::format!("{}> ", shell.get_prompt()));

    let mut input = String::new();

    loop {
        if let Some(c) = drivers::keyboard::try_read_char() {
            match c {
                '\n' => {
                    console.print("\n");
                    
                    // Check for GUI command
                    if input.trim() == "ui" {
                        console.print("Starting Pursuit Desktop...\n");
                        // Get screen back from console and run GUI
                        let screen = console.take_screen();
                        gui::run_gui(screen);
                        // Return to console mode
                        console.print("Returned to shell.\n");
                    } else {
                        handle_command(&input, &mut console, &mut shell);
                    }
                    
                    input.clear();
                    console.print(&alloc::format!("{}> ", shell.get_prompt()));
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
