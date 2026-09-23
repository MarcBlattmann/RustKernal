//! CMOS real-time clock. QEMU and most PCs keep it in UTC.

use x86_64::instructions::port::Port;

const CMOS_ADDRESS_PORT: u16 = 0x70;
const CMOS_DATA_PORT: u16 = 0x71;

const REGISTER_SECONDS: u8 = 0x00;
const REGISTER_MINUTES: u8 = 0x02;
const REGISTER_HOURS: u8 = 0x04;
const REGISTER_DAY: u8 = 0x07;
const REGISTER_MONTH: u8 = 0x08;
const REGISTER_YEAR: u8 = 0x09;
const REGISTER_STATUS_A: u8 = 0x0A;
const REGISTER_STATUS_B: u8 = 0x0B;

const UPDATE_IN_PROGRESS: u8 = 0x80;
const STATUS_B_24_HOUR: u8 = 0x02;
const STATUS_B_BINARY: u8 = 0x04;
const HOUR_PM_BIT: u8 = 0x80;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

fn read_register(register: u8) -> u8 {
    unsafe {
        Port::<u8>::new(CMOS_ADDRESS_PORT).write(register);
        Port::<u8>::new(CMOS_DATA_PORT).read()
    }
}

fn read_raw() -> DateTime {
    while read_register(REGISTER_STATUS_A) & UPDATE_IN_PROGRESS != 0 {
        core::hint::spin_loop();
    }
    DateTime {
        year: read_register(REGISTER_YEAR) as u16,
        month: read_register(REGISTER_MONTH),
        day: read_register(REGISTER_DAY),
        hour: read_register(REGISTER_HOURS),
        minute: read_register(REGISTER_MINUTES),
        second: read_register(REGISTER_SECONDS),
    }
}

fn from_bcd(value: u8) -> u8 {
    (value & 0x0F) + (value >> 4) * 10
}

pub fn now() -> DateTime {
    // Read until two reads agree so we never see a half-updated clock.
    let mut time = read_raw();
    loop {
        let again = read_raw();
        if again == time {
            break;
        }
        time = again;
    }

    let status = read_register(REGISTER_STATUS_B);
    let is_pm = time.hour & HOUR_PM_BIT != 0;
    time.hour &= !HOUR_PM_BIT;

    if status & STATUS_B_BINARY == 0 {
        time.second = from_bcd(time.second);
        time.minute = from_bcd(time.minute);
        time.hour = from_bcd(time.hour);
        time.day = from_bcd(time.day);
        time.month = from_bcd(time.month);
        time.year = from_bcd(time.year as u8) as u16;
    }
    if status & STATUS_B_24_HOUR == 0 && is_pm {
        time.hour = (time.hour + 12) % 24;
    }
    time.year += 2000;
    time
}
