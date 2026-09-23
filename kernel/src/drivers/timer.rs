use core::sync::atomic::{AtomicU64, Ordering};

const TIMER_DATA_PORT: u16 = 0x40;
const TIMER_COMMAND_PORT: u16 = 0x43;

const CHANNEL_0_RATE_GENERATOR_BINARY: u8 = 0x34;
const PIT_BASE_FREQUENCY_HZ: u64 = 1_193_182;
const TIMER_FREQUENCY_HZ: u64 = 1000;
const FREQUENCY_DIVIDER: u16 = (PIT_BASE_FREQUENCY_HZ / TIMER_FREQUENCY_HZ) as u16;

const TSC_CALIBRATION_TICKS: u64 = 50;
/// Wake up from `hlt` this long before a deadline and spin the rest, since ticks are ~1 ms apart.
const SPIN_WINDOW_NANOSECONDS: u64 = 1_500_000;

static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);
static TSC_AT_BOOT: AtomicU64 = AtomicU64::new(0);
static TSC_TICKS_PER_MILLISECOND: AtomicU64 = AtomicU64::new(0);

unsafe fn write_to_port(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value);
    }
}

pub fn init() {
    unsafe {
        write_to_port(TIMER_COMMAND_PORT, CHANNEL_0_RATE_GENERATOR_BINARY);

        let low_byte = (FREQUENCY_DIVIDER & 0xFF) as u8;
        let high_byte = ((FREQUENCY_DIVIDER >> 8) & 0xFF) as u8;

        write_to_port(TIMER_DATA_PORT, low_byte);
        write_to_port(TIMER_DATA_PORT, high_byte);
    }
}

/// Measures the CPU timestamp counter against the PIT so `uptime_nanoseconds` gets sub-microsecond
/// precision. Needs interrupts enabled; blocks for about 50 ms.
pub fn calibrate() {
    let start_tick = wait_for_next_tick();
    let start_tsc = read_tsc();
    while ticks() < start_tick + TSC_CALIBRATION_TICKS {
        x86_64::instructions::hlt();
    }
    let elapsed_tsc = read_tsc() - start_tsc;
    let elapsed_nanoseconds = ticks_to_nanoseconds(TSC_CALIBRATION_TICKS);
    let tsc_per_millisecond = (elapsed_tsc as u128 * 1_000_000 / elapsed_nanoseconds as u128) as u64;
    let tsc_since_boot = (ticks_to_nanoseconds(start_tick) as u128 * tsc_per_millisecond as u128 / 1_000_000) as u64;

    TSC_AT_BOOT.store(start_tsc.saturating_sub(tsc_since_boot), Ordering::SeqCst);
    TSC_TICKS_PER_MILLISECOND.store(tsc_per_millisecond, Ordering::SeqCst);
}

fn wait_for_next_tick() -> u64 {
    let current = ticks();
    while ticks() == current {
        x86_64::instructions::hlt();
    }
    ticks()
}

fn read_tsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

fn ticks_to_nanoseconds(ticks: u64) -> u64 {
    (ticks as u128 * FREQUENCY_DIVIDER as u128 * 1_000_000_000 / PIT_BASE_FREQUENCY_HZ as u128) as u64
}

pub fn tick() {
    TIMER_TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u64 {
    TIMER_TICKS.load(Ordering::SeqCst)
}

pub fn uptime_nanoseconds() -> u64 {
    let tsc_per_millisecond = TSC_TICKS_PER_MILLISECOND.load(Ordering::Relaxed);
    if tsc_per_millisecond == 0 {
        return ticks_to_nanoseconds(ticks());
    }
    let elapsed_tsc = read_tsc().saturating_sub(TSC_AT_BOOT.load(Ordering::Relaxed));
    (elapsed_tsc as u128 * 1_000_000 / tsc_per_millisecond as u128) as u64
}

/// Sleeps until `uptime_nanoseconds()` reaches `deadline`, halting the CPU for most of the wait.
pub fn sleep_until(deadline: u64) {
    while uptime_nanoseconds() + SPIN_WINDOW_NANOSECONDS < deadline {
        x86_64::instructions::hlt();
    }
    while uptime_nanoseconds() < deadline {
        core::hint::spin_loop();
    }
}
