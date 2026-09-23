use x86_64::instructions::port::Port;

const KEYBOARD_CONTROLLER_STATUS_PORT: u16 = 0x64;
const KEYBOARD_CONTROLLER_INPUT_FULL: u8 = 0x02;
const KEYBOARD_CONTROLLER_RESET_CPU: u8 = 0xFE;

/// ACPI power-off shortcuts of common virtual machines: (port, value).
const VIRTUAL_MACHINE_POWER_OFF: [(u16, u16); 3] = [
    (0x604, 0x2000),  // QEMU
    (0xB004, 0x2000), // Bochs and older QEMU
    (0x4004, 0x3400), // VirtualBox
];

pub fn reboot() -> ! {
    x86_64::instructions::interrupts::disable();
    unsafe {
        // Ask the keyboard controller to pulse the CPU reset line.
        let mut status = Port::<u8>::new(KEYBOARD_CONTROLLER_STATUS_PORT);
        while status.read() & KEYBOARD_CONTROLLER_INPUT_FULL != 0 {
            core::hint::spin_loop();
        }
        status.write(KEYBOARD_CONTROLLER_RESET_CPU);

        // Fallback: an interrupt with an empty interrupt table triple faults, which resets.
        let empty = x86_64::structures::DescriptorTablePointer { limit: 0, base: x86_64::VirtAddr::zero() };
        x86_64::instructions::tables::lidt(&empty);
        x86_64::instructions::interrupts::int3();
    }
    loop {
        x86_64::instructions::hlt();
    }
}

/// Powers the machine off. Only works on virtual machines until ACPI support exists; returns if
/// nothing responded.
pub fn shutdown() {
    for (port, value) in VIRTUAL_MACHINE_POWER_OFF {
        unsafe { Port::<u16>::new(port).write(value) };
    }
}
