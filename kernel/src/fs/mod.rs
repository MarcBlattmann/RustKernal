//! Storage: the ATA drives, their partitions and the FAT32 volume mounted at `/`.
//!
//! At boot [`init`] detects the drives, reads their partition tables and mounts the first FAT32
//! volume on a drive that is not the boot drive. Nothing is ever formatted or repaired
//! automatically; the only writes at boot are the ones mounting itself does (the FAT32
//! "volume in use" flag, set back to clean by the [`sync`] that follows).
//!
//! The mounted volume sits behind a global lock. Shell commands reach it through
//! [`with_volume`]; the lock is not re-entrant, so a closure passed to `with_volume` must never
//! call back into this module.

pub mod path;

pub use path::normalize;

use crate::drivers::ata::{self, AtaDrive};
use crate::drivers::{rtc, timer};
use crate::serial_println;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use fat32::mbr::{self, Partition};
use fat32::{BlockDevice, CachedDevice, DeviceError, FileSystem, FormatOptions, Timestamp, SECTOR_SIZE};
use spin::Mutex;

/// MBR partition types of GPT disks and EFI system partitions (boot disks of UEFI machines).
const PARTITION_TYPE_GPT_PROTECTIVE: u8 = 0xEE;
const PARTITION_TYPE_EFI_SYSTEM: u8 = 0xEF;

/// Sectors kept in the write-back cache of the mounted volume (256 KiB).
const CACHE_SECTORS: usize = 512;

/// Label of volumes created by `format` when none is given.
pub const DEFAULT_LABEL: &str = "RUSTKERNAL";

/// Smallest partition FAT32 fits on: 65525 clusters of one sector plus reserved sectors and two
/// FATs.
const MIN_FAT32_SECTORS: u64 = 65_525 + 32 + 2 * 520;

/// The mounted file system.
pub type Volume = FileSystem<CachedDevice<Disk>>;

/// Every detected drive, by index. Drives are never removed, so an index stays valid.
static DRIVES: Mutex<Vec<AtaDrive>> = Mutex::new(Vec::new());

/// Drive descriptions and the mounted volume. Lock order: `STATE` before `DRIVES`.
static STATE: Mutex<State> = Mutex::new(State { drives: Vec::new(), mounted: None, mount_error: None });

struct State {
    drives: Vec<Drive>,
    mounted: Option<Mounted>,
    /// Why the automatic mount at boot failed, as (source, error).
    mount_error: Option<(String, String)>,
}

struct Mounted {
    fs: Volume,
    /// `hd1p1` or `hd1` (whole-disk file system).
    source: String,
    drive: usize,
    was_clean: bool,
}

/// A window of sectors on one detected drive: a whole disk or one partition.
pub struct Disk {
    drive: usize,
    start: u64,
    sectors: u64,
}

impl Disk {
    fn with_drive<T>(&self, f: impl FnOnce(&mut AtaDrive) -> T) -> T {
        f(&mut DRIVES.lock()[self.drive])
    }

    fn check(&self, lba: u64, length: usize) -> Result<u64, DeviceError> {
        let count = (length / SECTOR_SIZE) as u64;
        match lba.checked_add(count) {
            Some(end) if length.is_multiple_of(SECTOR_SIZE) && end <= self.sectors => Ok(self.start + lba),
            _ => Err(DeviceError::OutOfRange),
        }
    }
}

impl BlockDevice for Disk {
    fn sector_count(&self) -> u64 {
        self.sectors
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        let lba = self.check(lba, buffer.len())?;
        self.with_drive(|drive| drive.read_sectors(lba, buffer))
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        let lba = self.check(lba, buffer.len())?;
        self.with_drive(|drive| drive.write_sectors(lba, buffer))
    }

    fn flush(&mut self) -> Result<(), DeviceError> {
        self.with_drive(|drive| drive.flush())
    }
}

/// What a boot sector says about the file system on it.
#[derive(Clone, Debug)]
pub struct FsProbe {
    /// `FAT32`, `FAT16`, `FAT12` or `FAT`.
    pub kind: String,
    /// The label in the boot sector, if set.
    pub label: Option<String>,
}

impl FsProbe {
    pub fn is_fat32(&self) -> bool {
        self.kind == "FAT32"
    }
}

#[derive(Clone, Debug)]
pub struct PartitionInfo {
    /// 1..=4, as in `hd1p1`.
    pub number: u8,
    pub partition_type: u8,
    pub bootable: bool,
    pub start_lba: u64,
    pub sector_count: u64,
    pub fs: Option<FsProbe>,
}

impl PartitionInfo {
    pub fn size_bytes(&self) -> u64 {
        self.sector_count * SECTOR_SIZE as u64
    }

    pub fn type_name(&self) -> &'static str {
        partition_type_name(self.partition_type)
    }
}

/// A detected drive and what is on it.
#[derive(Clone, Debug)]
pub struct Drive {
    pub name: &'static str,
    pub model: String,
    pub sector_count: u64,
    /// Looks like the drive the machine boots from: an MBR partition is marked active, or the
    /// disk is partitioned with GPT (a protective 0xEE entry) or holds an EFI system partition.
    /// Such drives are never mounted, formatted or written to.
    pub is_boot: bool,
    pub partitions: Vec<PartitionInfo>,
    /// A file system on the whole disk, without a partition table ("superfloppy").
    pub whole_disk: Option<FsProbe>,
    /// Reading the drive failed while probing it.
    pub error: Option<String>,
}

impl Drive {
    pub fn size_bytes(&self) -> u64 {
        self.sector_count * SECTOR_SIZE as u64
    }

    /// Whether `format` may erase this drive: not the boot drive, readable (a drive whose
    /// partition table could not be read might be the boot drive), and large enough for FAT32.
    pub fn check_formattable(&self) -> Result<(), FsError> {
        if self.is_boot {
            return Err(FsError::BootDrive(self.name));
        }
        if self.error.is_some() {
            return Err(FsError::Unreadable(self.name));
        }
        if self.sector_count < mbr::PARTITION_ALIGNMENT + MIN_FAT32_SECTORS {
            return Err(FsError::TooSmall(self.name));
        }
        Ok(())
    }

    pub fn has_fat32(&self) -> bool {
        self.whole_disk.as_ref().is_some_and(FsProbe::is_fat32)
            || self.partitions.iter().any(|partition| partition.fs.as_ref().is_some_and(FsProbe::is_fat32))
    }
}

/// The mounted volume, for `df`, `about` and the startup message.
#[derive(Clone, Debug)]
pub struct VolumeInfo {
    pub source: String,
    pub label: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub cluster_size: u32,
    pub volume_id: u32,
    /// Whether the volume was cleanly unmounted before this mount.
    pub was_clean: bool,
}

impl VolumeInfo {
    pub fn used_bytes(&self) -> u64 {
        self.total_bytes - self.free_bytes
    }
}

/// Storage state after boot, for the shell's startup message.
pub enum Status {
    Mounted(VolumeInfo),
    MountFailed { source: String, error: String },
    /// A data drive exists but holds no FAT32 file system.
    NoFileSystem(Drive),
    /// Only the boot drive (or nothing) was found.
    NoDataDrive,
}

#[derive(Debug)]
pub enum FsError {
    NotMounted,
    Fs(fat32::Error),
    NoSuchDrive(String),
    /// `format` was given a partition (`hd1p1`); it works on whole drives.
    NotAWholeDrive(String),
    BootDrive(&'static str),
    /// The drive's partition table could not be read.
    Unreadable(&'static str),
    AlreadyMounted(String),
    NoFileSystem(String),
    TooSmall(&'static str),
}

impl From<fat32::Error> for FsError {
    fn from(error: fat32::Error) -> Self {
        FsError::Fs(error)
    }
}

impl From<DeviceError> for FsError {
    fn from(error: DeviceError) -> Self {
        FsError::Fs(fat32::Error::Device(error))
    }
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FsError::NotMounted => f.write_str("no file system mounted (see 'lsblk' and 'mount')"),
            FsError::Fs(error) => write!(f, "{}", error),
            FsError::NoSuchDrive(name) => write!(f, "no drive or partition named '{}' (see 'lsblk')", name),
            FsError::NotAWholeDrive(name) => {
                write!(f, "{} is a partition; format erases a whole drive (use 'format {}')", name, drive_part(name))
            }
            FsError::BootDrive(name) => write!(
                f,
                "{} looks like the boot drive (it has an active, GPT or EFI partition) and is left alone",
                name
            ),
            FsError::Unreadable(name) => write!(f, "cannot read the partition table of {}, so it is left alone", name),
            FsError::AlreadyMounted(source) => write!(f, "{} is already mounted on / (run 'umount' first)", source),
            FsError::NoFileSystem(name) => write!(f, "no FAT32 file system on {} (run 'format {}' to create one)", name, drive_part(name)),
            FsError::TooSmall(name) => write!(f, "{} is too small for FAT32 (it needs about 34 MiB)", name),
        }
    }
}

/// `hd1` for `hd1p2`.
fn drive_part(name: &str) -> &str {
    match name.find('p') {
        Some(index) => &name[..index],
        None => name,
    }
}

/// Current time for file timestamps. The hardware clock runs in UTC, so files get UTC times.
fn now() -> Timestamp {
    let time = rtc::now();
    Timestamp { year: time.year, month: time.month, day: time.day, hour: time.hour, minute: time.minute, second: time.second }
}

/// Detects the drives and mounts the first FAT32 volume found on a drive other than the boot
/// drive at `/`.
pub fn init() {
    let found = ata::detect();
    let count = found.len();
    *DRIVES.lock() = found;

    let mut state = STATE.lock();
    state.drives = (0..count).map(probe_drive).collect();
    for drive in &state.drives {
        log_drive(drive);
    }

    let mut candidates = Vec::new();
    for (index, drive) in state.drives.iter().enumerate().filter(|(_, drive)| !drive.is_boot) {
        for partition in &drive.partitions {
            if partition.fs.as_ref().is_some_and(FsProbe::is_fat32) {
                candidates.push((index, Some(partition.number)));
            }
        }
        if drive.whole_disk.as_ref().is_some_and(FsProbe::is_fat32) {
            candidates.push((index, None));
        }
    }

    for (index, partition) in candidates {
        let source = source_name(state.drives[index].name, partition);
        match mount_locked(&mut state, index, partition) {
            Ok(info) => {
                serial_println!(
                    "fs: mounted {} ({}) on /, {} of {} bytes free{}",
                    info.source,
                    info.label,
                    info.free_bytes,
                    info.total_bytes,
                    if info.was_clean { "" } else { ", was not cleanly unmounted" }
                );
                state.mount_error = None;
                return;
            }
            Err(error) => {
                serial_println!("fs: cannot mount {}: {}", source, error);
                state.mount_error = Some((source, error.to_string()));
            }
        }
    }
    if state.mounted.is_none() && state.mount_error.is_none() {
        serial_println!("fs: no FAT32 volume on a data drive, nothing mounted");
    }
}

fn log_drive(drive: &Drive) {
    serial_println!(
        "fs: {}: {} MiB{}, {} partition(s){}",
        drive.name,
        drive.size_bytes() / (1024 * 1024),
        if drive.is_boot { ", boot drive" } else { "" },
        drive.partitions.len(),
        match &drive.error {
            Some(error) => format!(", probe failed: {}", error),
            None => String::new(),
        }
    );
    for partition in &drive.partitions {
        serial_println!(
            "fs:   {}p{}: type 0x{:02x}{}, start {}, {} sectors, {}",
            drive.name,
            partition.number,
            partition.partition_type,
            if partition.bootable { " (active)" } else { "" },
            partition.start_lba,
            partition.sector_count,
            match &partition.fs {
                Some(probe) => format!("{} {:?}", probe.kind, probe.label),
                None => String::from("no FAT file system"),
            }
        );
    }
    if let Some(probe) = &drive.whole_disk {
        serial_println!("fs:   {}: whole-disk {} {:?}", drive.name, probe.kind, probe.label);
    }
}

fn source_name(drive: &str, partition: Option<u8>) -> String {
    match partition {
        Some(number) => format!("{}p{}", drive, number),
        None => String::from(drive),
    }
}

fn whole_disk(index: usize) -> Disk {
    let sectors = DRIVES.lock()[index].info().sector_count;
    Disk { drive: index, start: 0, sectors }
}

/// Reads a drive's partition table and the boot sector of every partition. Read-only.
fn probe_drive(index: usize) -> Drive {
    let info = DRIVES.lock()[index].info();
    let mut drive = Drive {
        name: info.name,
        model: info.model,
        sector_count: info.sector_count,
        is_boot: false,
        partitions: Vec::new(),
        whole_disk: None,
        error: None,
    };

    let mut disk = whole_disk(index);
    let partitions = match mbr::read_partitions(&mut disk) {
        Ok(partitions) => partitions,
        Err(error) => {
            drive.error = Some(error.to_string());
            return drive;
        }
    };
    drive.is_boot = partitions.iter().any(|partition| {
        partition.bootable
            || partition.partition_type == PARTITION_TYPE_GPT_PROTECTIVE
            || partition.partition_type == PARTITION_TYPE_EFI_SYSTEM
    });
    if partitions.is_empty() {
        drive.whole_disk = probe_fs(&mut disk, 0);
    }
    for partition in partitions {
        let fs = if partition.start_lba < info.sector_count { probe_fs(&mut disk, partition.start_lba) } else { None };
        drive.partitions.push(PartitionInfo {
            number: partition.index + 1,
            partition_type: partition.partition_type,
            bootable: partition.bootable,
            start_lba: partition.start_lba,
            sector_count: partition.sector_count,
            fs,
        });
    }
    drive
}

/// Recognizes a FAT boot sector at `lba`.
fn probe_fs(disk: &mut Disk, lba: u64) -> Option<FsProbe> {
    let mut sector = [0u8; SECTOR_SIZE];
    disk.read_sectors(lba, &mut sector).ok()?;
    let jump = sector[0] == 0xEB || sector[0] == 0xE9;
    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
    if !jump || sector[510] != 0x55 || sector[511] != 0xAA || bytes_per_sector != SECTOR_SIZE as u16 {
        return None;
    }
    let text = |range: core::ops::Range<usize>| String::from(core::str::from_utf8(&sector[range]).unwrap_or("").trim());
    let (kind, label) = if &sector[0x52..0x5A] == b"FAT32   " {
        (String::from("FAT32"), text(0x47..0x52))
    } else if &sector[0x36..0x39] == b"FAT" {
        (text(0x36..0x3E), text(0x2B..0x36))
    } else {
        return None;
    };
    let label = if label.is_empty() || label == "NO NAME" { None } else { Some(label) };
    Some(FsProbe { kind, label })
}

fn find_drive(state: &State, name: &str) -> Result<(usize, Option<u8>), FsError> {
    let name = name.trim().to_ascii_lowercase();
    let (drive_name, partition) = match name.find('p') {
        Some(index) => {
            let number = name[index + 1..].parse::<u8>().map_err(|_| FsError::NoSuchDrive(name.clone()))?;
            (&name[..index], Some(number))
        }
        None => (name.as_str(), None),
    };
    let index = state
        .drives
        .iter()
        .position(|drive| drive.name == drive_name)
        .ok_or_else(|| FsError::NoSuchDrive(name.clone()))?;
    if let Some(number) = partition
        && !state.drives[index].partitions.iter().any(|partition| partition.number == number)
    {
        return Err(FsError::NoSuchDrive(name));
    }
    Ok((index, partition))
}

/// Mounts partition `number` of drive `index` (or the whole disk) at `/`.
fn mount_locked(state: &mut State, index: usize, number: Option<u8>) -> Result<VolumeInfo, FsError> {
    if let Some(mounted) = &state.mounted {
        return Err(FsError::AlreadyMounted(mounted.source.clone()));
    }
    let drive = &state.drives[index];
    if drive.is_boot {
        return Err(FsError::BootDrive(drive.name));
    }
    let disk = match number {
        Some(number) => {
            let partition = drive.partitions.iter().find(|partition| partition.number == number).ok_or_else(|| FsError::NoSuchDrive(source_name(drive.name, Some(number))))?;
            if partition.start_lba.saturating_add(partition.sector_count) > drive.sector_count {
                return Err(FsError::Fs(fat32::Error::Corrupt("partition extends past the end of the disk")));
            }
            Disk { drive: index, start: partition.start_lba, sectors: partition.sector_count }
        }
        None => whole_disk(index),
    };
    let source = source_name(drive.name, number);

    let mut fs = FileSystem::mount(CachedDevice::new(disk, CACHE_SECTORS))?;
    fs.set_time_source(now);
    // Mounting marks the volume in use; write that back to "clean" until the first change.
    fs.sync()?;
    let was_clean = fs.was_cleanly_unmounted();
    state.mounted = Some(Mounted { fs, source, drive: index, was_clean });
    Ok(volume_info_locked(state).expect("just mounted"))
}

fn volume_info_locked(state: &mut State) -> Option<VolumeInfo> {
    let mounted = state.mounted.as_mut()?;
    let stats = mounted.fs.stats().ok()?;
    Some(VolumeInfo {
        source: mounted.source.clone(),
        label: stats.label,
        total_bytes: stats.total_bytes,
        free_bytes: stats.free_bytes,
        cluster_size: stats.cluster_size,
        volume_id: stats.volume_id,
        was_clean: mounted.was_clean,
    })
}

fn unmount_locked(state: &mut State) -> Result<String, FsError> {
    let mounted = state.mounted.as_mut().ok_or(FsError::NotMounted)?;
    // Sync first so a failure leaves the volume mounted instead of losing unwritten data.
    mounted.fs.sync()?;
    let mounted = state.mounted.take().expect("checked above");
    let source = mounted.source;
    if let Err(error) = mounted.fs.unmount() {
        serial_println!("fs: unmounting {} after a successful sync failed: {}", source, error);
    }
    Ok(source)
}

/// Runs `f` on the mounted volume. `f` must not call other functions of this module.
pub fn with_volume<T>(f: impl FnOnce(&mut Volume) -> fat32::Result<T>) -> Result<T, FsError> {
    let mut state = STATE.lock();
    let mounted = state.mounted.as_mut().ok_or(FsError::NotMounted)?;
    f(&mut mounted.fs).map_err(FsError::Fs)
}

/// Like [`with_volume`], for closures with their own error type.
pub fn lock_volume<T>(f: impl FnOnce(&mut Volume) -> T) -> Result<T, FsError> {
    let mut state = STATE.lock();
    let mounted = state.mounted.as_mut().ok_or(FsError::NotMounted)?;
    Ok(f(&mut mounted.fs))
}

pub fn is_mounted() -> bool {
    STATE.lock().mounted.is_some()
}

pub fn volume_info() -> Option<VolumeInfo> {
    volume_info_locked(&mut STATE.lock())
}

/// Writes everything cached to the disk and marks the volume clean. Does nothing when no volume
/// is mounted.
pub fn sync() -> Result<(), FsError> {
    match STATE.lock().mounted.as_mut() {
        Some(mounted) => mounted.fs.sync().map_err(FsError::Fs),
        None => Ok(()),
    }
}

/// Syncs and unmounts the volume at `/`. Returns where it was mounted from.
pub fn unmount() -> Result<String, FsError> {
    unmount_locked(&mut STATE.lock())
}

/// Mounts `target` at `/`: a drive (`hd1`, its first FAT32 partition or whole-disk file system)
/// or one partition (`hd1p2`).
pub fn mount(target: &str) -> Result<VolumeInfo, FsError> {
    let mut state = STATE.lock();
    let (index, number) = find_drive(&state, target)?;
    state.drives[index] = probe_drive(index);
    let drive = &state.drives[index];
    if drive.is_boot {
        return Err(FsError::BootDrive(drive.name));
    }
    if drive.error.is_some() {
        return Err(FsError::Unreadable(drive.name));
    }
    let number = match number {
        Some(number) => Some(number),
        None => {
            let fat32_partition = drive.partitions.iter().find(|partition| partition.fs.as_ref().is_some_and(FsProbe::is_fat32));
            match fat32_partition {
                Some(partition) => Some(partition.number),
                None if drive.whole_disk.as_ref().is_some_and(FsProbe::is_fat32) => None,
                None => return Err(FsError::NoFileSystem(String::from(drive.name))),
            }
        }
    };
    mount_locked(&mut state, index, number)
}

/// Every detected drive, freshly probed.
pub fn drives() -> Vec<Drive> {
    let mut state = STATE.lock();
    for index in 0..state.drives.len() {
        state.drives[index] = probe_drive(index);
    }
    state.drives.clone()
}

/// A drive by name, without re-reading it.
pub fn drive(name: &str) -> Result<Drive, FsError> {
    let state = STATE.lock();
    let (index, number) = find_drive(&state, name)?;
    if number.is_some() {
        return Err(FsError::NotAWholeDrive(name.trim().to_ascii_lowercase()));
    }
    Ok(state.drives[index].clone())
}

/// Where the mounted volume comes from, e.g. `hd1p1`.
pub fn mounted_source() -> Option<String> {
    STATE.lock().mounted.as_ref().map(|mounted| mounted.source.clone())
}

/// Erases drive `name`: writes an MBR with one FAT32 partition spanning the disk, creates an
/// empty FAT32 file system labelled `label` on it and mounts it at `/` if nothing else is
/// mounted. Refuses the boot drive. A volume mounted from this drive is unmounted first.
pub fn format(name: &str, label: &str) -> Result<Option<VolumeInfo>, FsError> {
    let mut state = STATE.lock();
    let (index, number) = find_drive(&state, name)?;
    if number.is_some() {
        return Err(FsError::NotAWholeDrive(name.trim().to_ascii_lowercase()));
    }
    state.drives[index] = probe_drive(index);
    let drive = &state.drives[index];
    if drive.is_boot {
        return Err(FsError::BootDrive(drive.name));
    }
    drive.check_formattable()?;
    let drive_name = drive.name;
    if state.mounted.as_ref().is_some_and(|mounted| mounted.drive == index) {
        let source = unmount_locked(&mut state)?;
        serial_println!("fs: unmounted {} to format {}", source, drive_name);
    }

    let mut disk = whole_disk(index);
    let partition: Partition = mbr::write_single_partition(&mut disk, mbr::PARTITION_TYPE_FAT32_LBA)?;
    let mut volume = Disk { drive: index, start: partition.start_lba, sectors: partition.sector_count };
    let options = FormatOptions { label: String::from(label), volume_id: new_volume_id(), sectors_per_cluster: None };
    fat32::format(&mut volume, &options)?;
    volume.flush()?;
    serial_println!("fs: formatted {}p1 ({} sectors) as FAT32 '{}'", drive_name, partition.sector_count, label);

    state.drives[index] = probe_drive(index);
    state.mount_error = None;
    if state.mounted.is_some() {
        return Ok(None);
    }
    mount_locked(&mut state, index, Some(partition.index + 1)).map(Some)
}

/// A volume serial number from the date, time and uptime, like DOS does.
fn new_volume_id() -> u32 {
    let time = rtc::now();
    let date = (time.year as u32) << 16 | (time.month as u32) << 8 | time.day as u32;
    let clock = (time.hour as u32) << 16 | (time.minute as u32) << 8 | time.second as u32;
    let uptime = timer::uptime_nanoseconds();
    date.wrapping_add(clock << 8) ^ (uptime as u32).rotate_left(13) ^ (uptime >> 32) as u32
}

/// Writes all pending data before the machine reboots or powers off.
pub fn prepare_power_off() -> Result<(), FsError> {
    sync()
}

/// Storage state for the startup message.
pub fn status() -> Status {
    let mut state = STATE.lock();
    if let Some(info) = volume_info_locked(&mut state) {
        return Status::Mounted(info);
    }
    if let Some((source, error)) = &state.mount_error {
        return Status::MountFailed { source: source.clone(), error: error.clone() };
    }
    match state.drives.iter().find(|drive| !drive.is_boot && drive.error.is_none() && !drive.has_fat32()) {
        Some(drive) => Status::NoFileSystem(drive.clone()),
        None => Status::NoDataDrive,
    }
}

/// Human-readable name of an MBR partition type.
pub fn partition_type_name(partition_type: u8) -> &'static str {
    match partition_type {
        0x01 => "FAT12",
        0x04 | 0x06 | 0x0E => "FAT16",
        0x05 | 0x0F | 0x85 => "extended",
        0x07 => "NTFS/exFAT",
        0x0B => "FAT32 (CHS)",
        0x0C => "FAT32 (LBA)",
        0x82 => "Linux swap",
        0x83 => "Linux",
        0xEE => "GPT protective",
        0xEF => "EFI system",
        _ => "unknown",
    }
}
