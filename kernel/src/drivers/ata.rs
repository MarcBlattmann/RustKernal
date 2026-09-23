//! ATA PIO driver for the legacy IDE channels, the disk interface QEMU (`-drive ...,if=ide`, the
//! default on the `pc` machine), Bochs and VirtualBox emulate and that most chipsets still offer in
//! "compatibility mode".
//!
//! Each channel has a master and a slave device, which gives four possible drives:
//!
//! | name | channel   | I/O ports     | device |
//! |------|-----------|---------------|--------|
//! | hd0  | primary   | 0x1F0 / 0x3F6 | master |
//! | hd1  | primary   | 0x1F0 / 0x3F6 | slave  |
//! | hd2  | secondary | 0x170 / 0x376 | master |
//! | hd3  | secondary | 0x170 / 0x376 | slave  |
//!
//! The driver polls the status register instead of using interrupts (nIEN is set in the device
//! control register, and the PIC keeps IRQ 14/15 masked anyway). Every wait has a deadline, so a
//! dead or missing drive produces [`DeviceError::Timeout`] instead of hanging the kernel. Transfers
//! use 28-bit LBA commands where possible and the 48-bit "EXT" commands for sectors beyond 2^28.
//!
//! Nothing here writes to a disk unless [`BlockDevice::write_sectors`] is called, so detecting the
//! drives is safe even though hd0 holds the boot image.

use alloc::string::String;
use alloc::vec::Vec;
use fat32::{BlockDevice, DeviceError, SECTOR_SIZE};
use spin::Mutex;
use x86_64::instructions::port::Port;

use crate::drivers::timer;

// Command block registers, relative to the channel's I/O base.
const REGISTER_DATA: u16 = 0;
const REGISTER_ERROR: u16 = 1;
const REGISTER_FEATURES: u16 = 1;
const REGISTER_SECTOR_COUNT: u16 = 2;
const REGISTER_LBA_LOW: u16 = 3;
const REGISTER_LBA_MID: u16 = 4;
const REGISTER_LBA_HIGH: u16 = 5;
const REGISTER_DRIVE_SELECT: u16 = 6;
const REGISTER_STATUS: u16 = 7;
const REGISTER_COMMAND: u16 = 7;

// Control block register (alternate status when read, device control when written).
const REGISTER_ALTERNATE_STATUS: u16 = 0;
const REGISTER_DEVICE_CONTROL: u16 = 0;

const STATUS_ERROR: u8 = 0x01;
const STATUS_DATA_REQUEST: u8 = 0x08;
const STATUS_DEVICE_FAULT: u8 = 0x20;
const STATUS_BUSY: u8 = 0x80;
/// What a channel without any attached device reads back (pulled-up data lines).
const STATUS_FLOATING_BUS: u8 = 0xFF;

const CONTROL_DISABLE_INTERRUPTS: u8 = 0x02;
const CONTROL_SOFTWARE_RESET: u8 = 0x04;

/// Drive select: bits 7 and 5 are obsolete but set by convention, bit 6 selects LBA addressing,
/// bit 4 selects the slave, and bits 0-3 hold LBA bits 24-27 for 28-bit commands.
const DRIVE_SELECT_BASE: u8 = 0xA0;
const DRIVE_SELECT_LBA: u8 = 0x40;
const DRIVE_SELECT_SLAVE: u8 = 0x10;

const COMMAND_READ_SECTORS: u8 = 0x20;
const COMMAND_READ_SECTORS_EXT: u8 = 0x24;
const COMMAND_WRITE_SECTORS: u8 = 0x30;
const COMMAND_WRITE_SECTORS_EXT: u8 = 0x34;
const COMMAND_FLUSH_CACHE: u8 = 0xE7;
const COMMAND_FLUSH_CACHE_EXT: u8 = 0xEA;
const COMMAND_IDENTIFY: u8 = 0xEC;

/// Values of the LBA mid/high registers after reset or an aborted IDENTIFY, which tell the device
/// type apart without sending it any command it might not understand.
const SIGNATURE_ATA: (u8, u8) = (0x00, 0x00);
const SIGNATURE_ATAPI: (u8, u8) = (0x14, 0xEB);
const SIGNATURE_SATA: (u8, u8) = (0x3C, 0xC3);
const SIGNATURE_SATAPI: (u8, u8) = (0x69, 0x96);

// IDENTIFY DEVICE words.
const IDENTIFY_GENERAL_CONFIGURATION: usize = 0;
const IDENTIFY_SERIAL: core::ops::Range<usize> = 10..20;
const IDENTIFY_FIRMWARE: core::ops::Range<usize> = 23..27;
const IDENTIFY_MODEL: core::ops::Range<usize> = 27..47;
const IDENTIFY_CAPABILITIES: usize = 49;
const IDENTIFY_LBA28_SECTORS: usize = 60;
const IDENTIFY_COMMAND_SETS_SUPPORTED: usize = 83;
const IDENTIFY_LBA48_SECTORS: usize = 100;
const IDENTIFY_SECTOR_SIZE: usize = 106;
const IDENTIFY_LOGICAL_SECTOR_WORDS: usize = 117;

/// Word 0 bit 15 is clear for ATA devices.
const GENERAL_CONFIGURATION_NOT_ATA: u16 = 1 << 15;
const CAPABILITY_LBA: u16 = 1 << 9;
const COMMAND_SET_LBA48: u16 = 1 << 10;
const COMMAND_SET_FLUSH_CACHE: u16 = 1 << 12;
const COMMAND_SET_FLUSH_CACHE_EXT: u16 = 1 << 13;
/// Word 106: bits 15:14 = 01 mark the word as valid; bit 12 means logical sectors are larger
/// than 256 words (their size is then in words 117-118).
const SECTOR_SIZE_WORD_VALID_MASK: u16 = 0xC000;
const SECTOR_SIZE_WORD_VALID: u16 = 0x4000;
const SECTOR_SIZE_LARGE_LOGICAL: u16 = 1 << 12;

const WORDS_PER_SECTOR: usize = SECTOR_SIZE / 2;
/// Highest sector a 28-bit command can address is `LBA28_LIMIT - 1`.
const LBA28_LIMIT: u64 = 1 << 28;
/// Sectors per command. The protocol allows 256 (LBA28) or 65536 (LBA48); smaller chunks keep
/// the time a request monopolises the channel short and bound the damage of a failed command.
const MAX_SECTORS_PER_COMMAND: usize = 128;

const MILLISECOND: u64 = 1_000_000;
/// How long a drive may stay busy for a data transfer before the request fails.
const COMMAND_TIMEOUT: u64 = 5_000 * MILLISECOND;
/// Flushing a write cache can take a while on real hardware.
const FLUSH_TIMEOUT: u64 = 30_000 * MILLISECOND;
/// Detection budget per step; an absent device answers immediately, a present one within this.
const DETECT_TIMEOUT: u64 = 2_000 * MILLISECOND;
/// Spinning drives may take several seconds to come back from a software reset.
const RESET_TIMEOUT: u64 = 5_000 * MILLISECOND;

const DRIVE_NAMES: [&str; 4] = ["hd0", "hd1", "hd2", "hd3"];

#[derive(Clone, Copy)]
struct Channel {
    /// 0 = primary, 1 = secondary.
    index: usize,
    io_base: u16,
    control_base: u16,
}

const CHANNELS: [Channel; 2] = [
    Channel { index: 0, io_base: 0x1F0, control_base: 0x3F6 },
    Channel { index: 1, io_base: 0x170, control_base: 0x376 },
];

/// Both devices of a channel share its registers, so every command holds the channel's lock from
/// drive select to the end of the transfer.
static CHANNEL_LOCKS: [Mutex<()>; 2] = [Mutex::new(()), Mutex::new(())];

/// Why a channel operation failed, kept internal so detection can tell "no device" from errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    Timeout,
    /// ERR or DF was set; carries the status and error registers.
    DeviceError { status: u8, error: u8 },
    /// The drive stopped asking for data in the middle of a transfer.
    NoDataRequest { status: u8 },
}

impl From<Failure> for DeviceError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Timeout => DeviceError::Timeout,
            Failure::DeviceError { .. } | Failure::NoDataRequest { .. } => DeviceError::Io,
        }
    }
}

impl Channel {
    fn read_register(&self, register: u16) -> u8 {
        unsafe { Port::<u8>::new(self.io_base + register).read() }
    }

    fn write_register(&self, register: u16, value: u8) {
        unsafe { Port::<u8>::new(self.io_base + register).write(value) }
    }

    fn status(&self) -> u8 {
        self.read_register(REGISTER_STATUS)
    }

    fn alternate_status(&self) -> u8 {
        unsafe { Port::<u8>::new(self.control_base + REGISTER_ALTERNATE_STATUS).read() }
    }

    fn write_device_control(&self, value: u8) {
        unsafe { Port::<u8>::new(self.control_base + REGISTER_DEVICE_CONTROL).write(value) }
    }

    fn signature(&self) -> (u8, u8) {
        (self.read_register(REGISTER_LBA_MID), self.read_register(REGISTER_LBA_HIGH))
    }

    /// Waits about 400 ns, the time a device needs to put a valid status on the bus after a drive
    /// select or a command. Each alternate status read takes at least 100 ns on the ISA timings.
    fn delay_400ns(&self) {
        for _ in 0..4 {
            self.alternate_status();
        }
    }

    /// Selects `slave` (and, for 28-bit commands, LBA bits 24-27).
    fn select(&self, slave: bool, lba_high_nibble: u8) {
        let mut value = DRIVE_SELECT_BASE | DRIVE_SELECT_LBA | (lba_high_nibble & 0x0F);
        if slave {
            value |= DRIVE_SELECT_SLAVE;
        }
        self.write_register(REGISTER_DRIVE_SELECT, value);
        self.delay_400ns();
    }

    /// Polls until BSY clears and returns the final status.
    fn wait_not_busy(&self, timeout: u64) -> Result<u8, Failure> {
        let deadline = timer::uptime_nanoseconds() + timeout;
        loop {
            let status = self.status();
            if status & STATUS_BUSY == 0 {
                return Ok(status);
            }
            if timer::uptime_nanoseconds() > deadline {
                return Err(Failure::Timeout);
            }
            core::hint::spin_loop();
        }
    }

    /// Waits for the drive to finish a command phase and fails on ERR/DF.
    fn wait_ready(&self, timeout: u64) -> Result<u8, Failure> {
        let status = self.wait_not_busy(timeout)?;
        if status & (STATUS_ERROR | STATUS_DEVICE_FAULT) != 0 {
            return Err(Failure::DeviceError { status, error: self.read_register(REGISTER_ERROR) });
        }
        Ok(status)
    }

    /// Waits until the drive is ready to transfer one sector of data.
    fn wait_data_request(&self, timeout: u64) -> Result<(), Failure> {
        let status = self.wait_ready(timeout)?;
        if status & STATUS_DATA_REQUEST == 0 {
            return Err(Failure::NoDataRequest { status });
        }
        Ok(())
    }

    /// Resets both devices on the channel and leaves interrupts disabled. Afterwards the master is
    /// selected and both devices show their type signature.
    fn software_reset(&self) -> Result<(), Failure> {
        self.write_device_control(CONTROL_SOFTWARE_RESET | CONTROL_DISABLE_INTERRUPTS);
        // SRST must stay asserted for at least 5 us.
        spin_for(5_000);
        self.write_device_control(CONTROL_DISABLE_INTERRUPTS);
        // Devices may take up to 2 ms before they set BSY.
        spin_for(2 * MILLISECOND);
        self.wait_not_busy(RESET_TIMEOUT).map(|_| ())
    }

    /// Checks whether anything drives the bus: with no device attached the status reads 0xFF.
    fn is_floating(&self) -> bool {
        [false, true].iter().all(|&slave| {
            self.select(slave, 0);
            self.status() == STATUS_FLOATING_BUS
        })
    }

    /// Sends the task file for a transfer of `count` sectors at `lba` and issues `command`.
    /// `count` is 1..=256 for 28-bit and 1..=65536 for 48-bit commands.
    fn issue(&self, slave: bool, lba: u64, count: usize, lba48: bool, command: u8) -> Result<(), Failure> {
        // The registers may only be written while the currently selected device is idle, and the
        // newly selected one must be idle before it accepts a command.
        self.wait_not_busy(COMMAND_TIMEOUT)?;
        self.select(slave, if lba48 { 0 } else { (lba >> 24) as u8 });
        self.wait_not_busy(COMMAND_TIMEOUT)?;
        if lba48 {
            // High-order bytes first; each register is a two-deep FIFO.
            self.write_register(REGISTER_FEATURES, 0);
            self.write_register(REGISTER_SECTOR_COUNT, (count >> 8) as u8);
            self.write_register(REGISTER_LBA_LOW, (lba >> 24) as u8);
            self.write_register(REGISTER_LBA_MID, (lba >> 32) as u8);
            self.write_register(REGISTER_LBA_HIGH, (lba >> 40) as u8);
        }
        self.write_register(REGISTER_FEATURES, 0);
        // A count of 256 (LBA28) or 65536 (LBA48) is written as 0.
        self.write_register(REGISTER_SECTOR_COUNT, count as u8);
        self.write_register(REGISTER_LBA_LOW, lba as u8);
        self.write_register(REGISTER_LBA_MID, (lba >> 8) as u8);
        self.write_register(REGISTER_LBA_HIGH, (lba >> 16) as u8);
        self.write_register(REGISTER_COMMAND, command);
        self.delay_400ns();
        Ok(())
    }

    fn read_sector_data(&self, sector: &mut [u8]) {
        debug_assert_eq!(sector.len(), SECTOR_SIZE);
        // `rep insw` moves the whole sector in one instruction, which also keeps the number of
        // traps down under emulation.
        unsafe {
            core::arch::asm!(
                "rep insw",
                inout("rdi") sector.as_mut_ptr() => _,
                inout("rcx") WORDS_PER_SECTOR => _,
                in("dx") self.io_base + REGISTER_DATA,
                options(nostack, preserves_flags),
            );
        }
    }

    fn write_sector_data(&self, sector: &[u8]) {
        debug_assert_eq!(sector.len(), SECTOR_SIZE);
        // Word by word rather than `rep outsw`: some real drives cannot accept back-to-back words.
        let mut data = Port::<u16>::new(self.io_base + REGISTER_DATA);
        for word in sector.chunks_exact(2) {
            unsafe { data.write(u16::from_le_bytes([word[0], word[1]])) };
        }
    }

    fn identify(&self, slave: bool) -> Probe {
        self.select(slave, 0);
        self.write_register(REGISTER_SECTOR_COUNT, 0);
        self.write_register(REGISTER_LBA_LOW, 0);
        self.write_register(REGISTER_LBA_MID, 0);
        self.write_register(REGISTER_LBA_HIGH, 0);
        self.write_register(REGISTER_COMMAND, COMMAND_IDENTIFY);
        self.delay_400ns();

        // No device answers with status 0 (or 0xFF on a floating bus).
        let status = self.status();
        if status == 0 || status == STATUS_FLOATING_BUS {
            return Probe::Absent;
        }
        if self.wait_not_busy(DETECT_TIMEOUT).is_err() {
            return Probe::Unresponsive;
        }

        // Packet devices abort IDENTIFY DEVICE and leave their signature behind.
        let signature = self.signature();
        if signature != SIGNATURE_ATA {
            return Probe::Other(signature_name(signature));
        }

        let deadline = timer::uptime_nanoseconds() + DETECT_TIMEOUT;
        loop {
            let status = self.status();
            if status & STATUS_ERROR != 0 {
                let signature = self.signature();
                return Probe::Other(signature_name(signature));
            }
            if status & STATUS_DATA_REQUEST != 0 {
                break;
            }
            if timer::uptime_nanoseconds() > deadline {
                return Probe::Unresponsive;
            }
            core::hint::spin_loop();
        }

        let mut bytes = [0u8; SECTOR_SIZE];
        self.read_sector_data(&mut bytes);
        let mut words = [0u16; WORDS_PER_SECTOR];
        for (word, pair) in words.iter_mut().zip(bytes.chunks_exact(2)) {
            *word = u16::from_le_bytes([pair[0], pair[1]]);
        }
        Probe::Ata(words)
    }
}

enum Probe {
    Absent,
    Unresponsive,
    /// A device that is not an ATA disk (CD/DVD drive, ...).
    Other(&'static str),
    Ata([u16; WORDS_PER_SECTOR]),
}

fn signature_name(signature: (u8, u8)) -> &'static str {
    match signature {
        SIGNATURE_ATAPI => "ATAPI device (CD/DVD)",
        SIGNATURE_SATAPI => "SATAPI device",
        SIGNATURE_SATA => "SATA device in native mode",
        SIGNATURE_ATA => "ATA device that rejected IDENTIFY",
        _ => "unknown device",
    }
}

fn spin_for(nanoseconds: u64) {
    let deadline = timer::uptime_nanoseconds() + nanoseconds;
    while timer::uptime_nanoseconds() < deadline {
        core::hint::spin_loop();
    }
}

/// Decodes an IDENTIFY string: two ASCII characters per word, high byte first, space padded.
fn identify_string(words: &[u16]) -> String {
    let mut text = String::with_capacity(words.len() * 2);
    for word in words {
        for byte in word.to_be_bytes() {
            text.push(if byte.is_ascii_graphic() || byte == b' ' { byte as char } else { ' ' });
        }
    }
    String::from(text.trim())
}

/// What `detect` learned about a drive.
#[derive(Clone, Debug)]
pub struct DriveInfo {
    /// `hd0`..`hd3`.
    pub name: &'static str,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    /// Addressable 512-byte sectors.
    pub sector_count: u64,
    /// Supports 48-bit LBA commands (needed beyond 128 GiB).
    pub lba48: bool,
}

impl DriveInfo {
    pub fn size_bytes(&self) -> u64 {
        self.sector_count * SECTOR_SIZE as u64
    }
}

/// An ATA hard disk found by [`detect`]. Reads and writes go straight to the drive; there is no
/// caching here (wrap it in `fat32::CachedDevice` for that).
pub struct AtaDrive {
    channel: Channel,
    slave: bool,
    supports_flush: bool,
    supports_flush_ext: bool,
    info: DriveInfo,
}

/// Finds the ATA hard disks on both legacy IDE channels, skipping CD/DVD drives and other packet
/// devices. Resets each populated channel, so it must not run while a request is in flight.
pub fn detect() -> Vec<AtaDrive> {
    let mut drives = Vec::new();
    for channel in CHANNELS {
        let _lock = CHANNEL_LOCKS[channel.index].lock();

        if channel.is_floating() {
            continue;
        }
        if channel.software_reset().is_err() {
            crate::serial_println!("ata: channel {} does not come out of reset, skipped", channel.index);
            continue;
        }

        for slave in [false, true] {
            let name = DRIVE_NAMES[channel.index * 2 + slave as usize];
            match channel.identify(slave) {
                Probe::Absent => {}
                Probe::Unresponsive => crate::serial_println!("ata: {}: no response to IDENTIFY, skipped", name),
                Probe::Other(kind) => crate::serial_println!("ata: {}: {}, skipped", name, kind),
                Probe::Ata(identify) => match AtaDrive::from_identify(channel, slave, name, &identify) {
                    Ok(drive) => {
                        let info = &drive.info;
                        crate::serial_println!(
                            "ata: {}: \"{}\" serial \"{}\" firmware \"{}\", {} sectors ({} MiB), {}",
                            info.name,
                            info.model,
                            info.serial,
                            info.firmware,
                            info.sector_count,
                            info.size_bytes() / (1024 * 1024),
                            if info.lba48 { "LBA48" } else { "LBA28" }
                        );
                        drives.push(drive);
                    }
                    Err(reason) => crate::serial_println!("ata: {}: {}, skipped", name, reason),
                },
            }
        }
    }
    if drives.is_empty() {
        crate::serial_println!("ata: no hard disks found");
    }
    drives
}

impl AtaDrive {
    fn from_identify(
        channel: Channel,
        slave: bool,
        name: &'static str,
        identify: &[u16; WORDS_PER_SECTOR],
    ) -> Result<Self, &'static str> {
        if identify[IDENTIFY_GENERAL_CONFIGURATION] & GENERAL_CONFIGURATION_NOT_ATA != 0 {
            return Err("not an ATA device");
        }
        if identify[IDENTIFY_CAPABILITIES] & CAPABILITY_LBA == 0 {
            return Err("CHS-only drive (no LBA support)");
        }
        let sector_size_word = identify[IDENTIFY_SECTOR_SIZE];
        if sector_size_word & SECTOR_SIZE_WORD_VALID_MASK == SECTOR_SIZE_WORD_VALID
            && sector_size_word & SECTOR_SIZE_LARGE_LOGICAL != 0
        {
            let words = identify[IDENTIFY_LOGICAL_SECTOR_WORDS] as u32
                | (identify[IDENTIFY_LOGICAL_SECTOR_WORDS + 1] as u32) << 16;
            if words as usize != WORDS_PER_SECTOR {
                return Err("logical sector size is not 512 bytes");
            }
        }

        let lba28_sectors = identify[IDENTIFY_LBA28_SECTORS] as u64
            | (identify[IDENTIFY_LBA28_SECTORS + 1] as u64) << 16;
        let command_sets = identify[IDENTIFY_COMMAND_SETS_SUPPORTED];
        // Word 83 is only meaningful when bits 15:14 read 01.
        let command_sets_valid = command_sets & 0xC000 == 0x4000;
        let lba48 = command_sets_valid && command_sets & COMMAND_SET_LBA48 != 0;
        let lba48_sectors = identify[IDENTIFY_LBA48_SECTORS..IDENTIFY_LBA48_SECTORS + 4]
            .iter()
            .rev()
            .fold(0u64, |total, &word| total << 16 | word as u64);
        let sector_count = if lba48 && lba48_sectors != 0 { lba48_sectors } else { lba28_sectors };
        if sector_count == 0 {
            return Err("drive reports zero sectors");
        }

        Ok(Self {
            channel,
            slave,
            supports_flush: command_sets_valid && command_sets & COMMAND_SET_FLUSH_CACHE != 0,
            supports_flush_ext: lba48 && command_sets & COMMAND_SET_FLUSH_CACHE_EXT != 0,
            info: DriveInfo {
                name,
                model: identify_string(&identify[IDENTIFY_MODEL]),
                serial: identify_string(&identify[IDENTIFY_SERIAL]),
                firmware: identify_string(&identify[IDENTIFY_FIRMWARE]),
                sector_count,
                lba48,
            },
        })
    }

    pub fn info(&self) -> DriveInfo {
        self.info.clone()
    }

    pub fn name(&self) -> &'static str {
        self.info.name
    }

    /// Checks that a request of `length` bytes covers whole sectors that all lie on the drive.
    fn check_request(&self, lba: u64, length: usize) -> Result<(), DeviceError> {
        if length % SECTOR_SIZE != 0 {
            return Err(DeviceError::OutOfRange);
        }
        match lba.checked_add((length / SECTOR_SIZE) as u64) {
            Some(end) if end <= self.info.sector_count => Ok(()),
            _ => Err(DeviceError::OutOfRange),
        }
    }

    /// Picks the 28-bit command unless the transfer touches sectors at or above 2^28.
    fn needs_lba48(&self, lba: u64, count: usize) -> Result<bool, DeviceError> {
        let needs_lba48 = lba + count as u64 > LBA28_LIMIT;
        if needs_lba48 && !self.info.lba48 {
            return Err(DeviceError::OutOfRange);
        }
        Ok(needs_lba48)
    }

    /// Brings the channel back to a known state after a failed command, so the next request (or
    /// the other drive on the channel) does not inherit a half-finished transfer.
    fn recover(&self, failure: Failure) -> DeviceError {
        crate::serial_println!("ata: {}: command failed: {:?}, resetting channel", self.info.name, failure);
        if self.channel.software_reset().is_err() {
            crate::serial_println!("ata: {}: channel did not come out of reset", self.info.name);
        }
        failure.into()
    }

    fn read_chunk(&self, lba: u64, buffer: &mut [u8], lba48: bool) -> Result<(), Failure> {
        let count = buffer.len() / SECTOR_SIZE;
        let command = if lba48 { COMMAND_READ_SECTORS_EXT } else { COMMAND_READ_SECTORS };
        self.channel.issue(self.slave, lba, count, lba48, command)?;
        for sector in buffer.chunks_exact_mut(SECTOR_SIZE) {
            self.channel.wait_data_request(COMMAND_TIMEOUT)?;
            self.channel.read_sector_data(sector);
        }
        self.channel.delay_400ns();
        self.channel.wait_ready(COMMAND_TIMEOUT)?;
        Ok(())
    }

    fn write_chunk(&self, lba: u64, buffer: &[u8], lba48: bool) -> Result<(), Failure> {
        let count = buffer.len() / SECTOR_SIZE;
        let command = if lba48 { COMMAND_WRITE_SECTORS_EXT } else { COMMAND_WRITE_SECTORS };
        self.channel.issue(self.slave, lba, count, lba48, command)?;
        for sector in buffer.chunks_exact(SECTOR_SIZE) {
            self.channel.wait_data_request(COMMAND_TIMEOUT)?;
            self.channel.write_sector_data(sector);
        }
        // The last sector is committed (to the drive's cache) once BSY clears again.
        self.channel.delay_400ns();
        self.channel.wait_ready(COMMAND_TIMEOUT)?;
        Ok(())
    }
}

impl BlockDevice for AtaDrive {
    fn sector_count(&self) -> u64 {
        self.info.sector_count
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        self.check_request(lba, buffer.len())?;
        let _lock = CHANNEL_LOCKS[self.channel.index].lock();
        for (index, chunk) in buffer.chunks_mut(MAX_SECTORS_PER_COMMAND * SECTOR_SIZE).enumerate() {
            let chunk_lba = lba + (index * MAX_SECTORS_PER_COMMAND) as u64;
            let lba48 = self.needs_lba48(chunk_lba, chunk.len() / SECTOR_SIZE)?;
            self.read_chunk(chunk_lba, chunk, lba48).map_err(|failure| self.recover(failure))?;
        }
        Ok(())
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        self.check_request(lba, buffer.len())?;
        let _lock = CHANNEL_LOCKS[self.channel.index].lock();
        for (index, chunk) in buffer.chunks(MAX_SECTORS_PER_COMMAND * SECTOR_SIZE).enumerate() {
            let chunk_lba = lba + (index * MAX_SECTORS_PER_COMMAND) as u64;
            let lba48 = self.needs_lba48(chunk_lba, chunk.len() / SECTOR_SIZE)?;
            self.write_chunk(chunk_lba, chunk, lba48).map_err(|failure| self.recover(failure))?;
        }
        Ok(())
    }

    /// Commits the drive's write cache to the medium. Drives that do not implement FLUSH CACHE
    /// have no volatile cache to flush.
    fn flush(&mut self) -> Result<(), DeviceError> {
        if !self.supports_flush && !self.supports_flush_ext {
            return Ok(());
        }
        let _lock = CHANNEL_LOCKS[self.channel.index].lock();
        let result = (|| {
            self.channel.wait_not_busy(COMMAND_TIMEOUT)?;
            self.channel.select(self.slave, 0);
            self.channel.wait_not_busy(COMMAND_TIMEOUT)?;
            let command = if self.supports_flush_ext { COMMAND_FLUSH_CACHE_EXT } else { COMMAND_FLUSH_CACHE };
            self.channel.write_register(REGISTER_COMMAND, command);
            self.channel.delay_400ns();
            self.channel.wait_ready(FLUSH_TIMEOUT).map(|_| ())
        })();
        result.map_err(|failure| self.recover(failure))
    }
}
