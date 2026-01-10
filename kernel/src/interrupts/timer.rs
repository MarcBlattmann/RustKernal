const TIMER_DATA_PORT: u16 = 0x40;
const TIMER_COMMAND_PORT: u16 = 0x43;

const CHANNEL_0_RATE_GENERATOR_BINARY: u8 = 0x34;
const MAX_FREQUENCY_DIVIDER: u16 = 65535;

unsafe fn write_to_port(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value);
    }
}

pub fn init() {
    unsafe {
        write_to_port(TIMER_COMMAND_PORT, CHANNEL_0_RATE_GENERATOR_BINARY);
        
        let frequency_divider = MAX_FREQUENCY_DIVIDER;
        let low_byte = (frequency_divider & 0xFF) as u8;
        let high_byte = ((frequency_divider >> 8) & 0xFF) as u8;
        
        write_to_port(TIMER_DATA_PORT, low_byte);
        write_to_port(TIMER_DATA_PORT, high_byte);
    }
}
