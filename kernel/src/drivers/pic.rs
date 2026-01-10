const MASTER_PIC_COMMAND_PORT: u16 = 0x20;
const MASTER_PIC_DATA_PORT: u16 = 0x21;
const SLAVE_PIC_COMMAND_PORT: u16 = 0xA0;
const SLAVE_PIC_DATA_PORT: u16 = 0xA1;

const INIT_COMMAND: u8 = 0x11;
const MODE_8086: u8 = 0x01;
const END_OF_INTERRUPT: u8 = 0x20;

const MASTER_PIC_VECTOR_OFFSET: u8 = 32;
const SLAVE_PIC_VECTOR_OFFSET: u8 = 40;

const SLAVE_ON_IRQ2_FOR_MASTER: u8 = 4;
const SLAVE_CASCADE_IDENTITY: u8 = 2;

const MASK_TIMER_AND_KEYBOARD_ENABLED: u8 = 0xFC;
const MASK_ALL: u8 = 0xFF;

unsafe fn wait_for_io_operation() {
    unsafe {
        core::arch::asm!("in al, 0x80", lateout("al") _);
    }
}

unsafe fn write_to_port(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value);
    }
}

pub fn init() {
    super::apic::disable();
    
    unsafe {
        write_to_port(MASTER_PIC_COMMAND_PORT, INIT_COMMAND);
        write_to_port(SLAVE_PIC_COMMAND_PORT, INIT_COMMAND);
        wait_for_io_operation();
        
        write_to_port(MASTER_PIC_DATA_PORT, MASTER_PIC_VECTOR_OFFSET);
        write_to_port(SLAVE_PIC_DATA_PORT, SLAVE_PIC_VECTOR_OFFSET);
        wait_for_io_operation();
        
        write_to_port(MASTER_PIC_DATA_PORT, SLAVE_ON_IRQ2_FOR_MASTER);
        write_to_port(SLAVE_PIC_DATA_PORT, SLAVE_CASCADE_IDENTITY);
        wait_for_io_operation();
        
        write_to_port(MASTER_PIC_DATA_PORT, MODE_8086);
        write_to_port(SLAVE_PIC_DATA_PORT, MODE_8086);
        wait_for_io_operation();
        
        write_to_port(MASTER_PIC_DATA_PORT, MASK_TIMER_AND_KEYBOARD_ENABLED);
        write_to_port(SLAVE_PIC_DATA_PORT, MASK_ALL);
        wait_for_io_operation();
    }
    
    x86_64::instructions::interrupts::enable();
}

pub fn send_eoi(irq_number: u8) {
    unsafe {
        if irq_number >= 8 {
            write_to_port(SLAVE_PIC_COMMAND_PORT, END_OF_INTERRUPT);
        }
        write_to_port(MASTER_PIC_COMMAND_PORT, END_OF_INTERRUPT);
    }
}
