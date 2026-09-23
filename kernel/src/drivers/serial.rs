use core::fmt::{self, Write};
use spin::Mutex;
use x86_64::instructions::port::Port;

const COM1_PORT: u16 = 0x3F8;
const LINE_STATUS_TRANSMIT_EMPTY: u8 = 0x20;

static SERIAL: Mutex<SerialPort> = Mutex::new(SerialPort { base: COM1_PORT });

struct SerialPort {
    base: u16,
}

impl SerialPort {
    fn init(&mut self) {
        unsafe {
            Port::<u8>::new(self.base + 1).write(0x00); // disable serial interrupts
            Port::<u8>::new(self.base + 3).write(0x80); // enable baud rate divisor access
            Port::<u8>::new(self.base).write(0x01);     // divisor low byte: 115200 baud
            Port::<u8>::new(self.base + 1).write(0x00); // divisor high byte
            Port::<u8>::new(self.base + 3).write(0x03); // 8 bits, no parity, one stop bit
            Port::<u8>::new(self.base + 2).write(0xC7); // enable and clear FIFOs
            Port::<u8>::new(self.base + 4).write(0x0B); // data terminal ready, request to send
        }
    }

    fn write_byte(&mut self, byte: u8) {
        unsafe {
            let mut line_status = Port::<u8>::new(self.base + 5);
            while line_status.read() & LINE_STATUS_TRANSMIT_EMPTY == 0 {
                core::hint::spin_loop();
            }
            Port::<u8>::new(self.base).write(byte);
        }
    }
}

impl Write for SerialPort {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

pub fn init() {
    SERIAL.lock().init();
}

/// Writes formatted text to COM1. With QEMU's `-serial stdio` it shows up in the terminal.
pub fn log(arguments: fmt::Arguments) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let _ = SERIAL.lock().write_fmt(arguments);
    });
}

#[macro_export]
macro_rules! serial_println {
    ($($argument:tt)*) => {
        $crate::drivers::serial::log(format_args!("{}\n", format_args!($($argument)*)))
    };
}
