//! FAT32 filesystem for RustKernal.
//!
//! `no_std` + `alloc`, so the same code runs in the kernel (on the ATA driver) and on the host (in
//! tests and in the QEMU runner, which pre-formats the data disk image).
//!
//! PUBLIC API CONTRACT: the kernel and the runner are written against the signatures in this file.
//! Implementations may add items but must not change or remove the ones declared here.
//!
//! # Layout of the implementation
//!
//! * [`boot_sector`]: BPB / FSInfo parsing and validation, volume geometry.
//! * [`fat`]: FAT access through a small write-back sector cache, cluster allocation, chains.
//! * [`io`]: byte-granular I/O on the data area and along cluster chains.
//! * [`name`]: name validation, 8.3 short names, case-insensitive comparison.
//! * [`dir`]: directory entries, VFAT long file names, entry creation and deletion.
//! * [`ops`]: the file and directory operations behind the public API.
//! * [`format`], [`check`]: `mkfs` and a read-only `fsck`.
//!
//! # Crash safety
//!
//! Operations order their writes so that an interruption at any point leaves, at worst, lost
//! clusters (allocated in the FAT but unreachable) rather than corrupt or cross-linked files:
//! clusters are linked in the FAT and data is written before a directory entry points at them,
//! and directory entries are removed before their clusters are freed. The FAT[1] clean-shutdown
//! bit is cleared while the volume has unsynced modifications. The one exception is `rename`,
//! which creates the new entry before it removes the old one: an interruption between the two
//! leaves both names pointing at the same data rather than losing the file. A failed rename
//! removes the new entry again (unless the old one is already gone: then the rename took effect);
//! if even that fails, the volume refuses further changes and stays marked dirty. Mounting a
//! volume that was not cleanly unmounted removes such duplicates (keeping one name, freeing
//! nothing), so removing a file or directory never frees clusters another entry still uses.
//!
//! The order is enforced with write barriers ([`BlockDevice::flush`]) at every point where it
//! matters, so it also holds on top of a write-back cache such as [`CachedDevice`], which may
//! write sectors back in any order between two flushes.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub mod cache;
pub mod mbr;

mod boot_sector;
mod check;
mod dir;
mod fat;
mod format;
mod io;
mod name;
mod ops;
mod path;
mod time;

pub use cache::CachedDevice;
pub use mbr::{Partition, PartitionDevice};

pub const SECTOR_SIZE: usize = 512;

/// A disk addressed in 512-byte sectors.
pub trait BlockDevice {
    /// Total number of sectors.
    fn sector_count(&self) -> u64;
    /// Reads `buffer.len() / SECTOR_SIZE` sectors starting at `lba`. `buffer.len()` is a
    /// non-zero multiple of `SECTOR_SIZE`.
    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> core::result::Result<(), DeviceError>;
    /// Writes `buffer.len() / SECTOR_SIZE` sectors starting at `lba`.
    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> core::result::Result<(), DeviceError>;
    /// Makes all completed writes durable (drive write cache flush).
    fn flush(&mut self) -> core::result::Result<(), DeviceError>;
}

impl<D: BlockDevice + ?Sized> BlockDevice for &mut D {
    fn sector_count(&self) -> u64 {
        (**self).sector_count()
    }
    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> core::result::Result<(), DeviceError> {
        (**self).read_sectors(lba, buffer)
    }
    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> core::result::Result<(), DeviceError> {
        (**self).write_sectors(lba, buffer)
    }
    fn flush(&mut self) -> core::result::Result<(), DeviceError> {
        (**self).flush()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceError {
    /// The request reaches past the end of the device.
    OutOfRange,
    /// The drive reported an error.
    Io,
    /// The drive did not respond in time.
    Timeout,
    ReadOnly,
    NotReady,
}

impl core::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            DeviceError::OutOfRange => "sector out of range",
            DeviceError::Io => "i/o error",
            DeviceError::Timeout => "device timed out",
            DeviceError::ReadOnly => "read-only device",
            DeviceError::NotReady => "device not ready",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Device(DeviceError),
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    DirectoryNotEmpty,
    /// Name contains characters FAT does not allow, is empty, `.`/`..` where not allowed, or ends
    /// with a dot or a space. (Names longer than 255 UTF-16 units are `NameTooLong`.)
    InvalidName,
    /// Path is not absolute or otherwise malformed.
    InvalidPath,
    /// Moving a directory into itself or one of its descendants.
    InvalidMove,
    NoSpace,
    /// Files are limited to 4 GiB - 1 byte on FAT32.
    FileTooLarge,
    /// The volume is not FAT32 or its structures are inconsistent.
    Corrupt(&'static str),
    Unsupported(&'static str),
    /// Operation on the root directory that is not allowed (remove, rename).
    RootDirectory,
    /// Name is longer than 255 UTF-16 units.
    NameTooLong,
}

impl From<DeviceError> for Error {
    fn from(error: DeviceError) -> Self {
        Error::Device(error)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Device(DeviceError::Io) => f.write_str("i/o error"),
            Error::Device(DeviceError::ReadOnly) => f.write_str("read-only file system"),
            Error::Device(error) => write!(f, "i/o error: {}", error),
            Error::NotFound => f.write_str("no such file or directory"),
            Error::AlreadyExists => f.write_str("file exists"),
            Error::NotADirectory => f.write_str("not a directory"),
            Error::IsADirectory => f.write_str("is a directory"),
            Error::DirectoryNotEmpty => f.write_str("directory not empty"),
            Error::InvalidName => f.write_str("invalid file name"),
            Error::InvalidPath => f.write_str("invalid path"),
            Error::InvalidMove => f.write_str("cannot move a directory into itself"),
            Error::NoSpace => f.write_str("no space left on device"),
            Error::FileTooLarge => f.write_str("file too large"),
            Error::Corrupt(what) => write!(f, "file system corrupt: {}", what),
            Error::Unsupported(what) => write!(f, "unsupported: {}", what),
            Error::RootDirectory => f.write_str("operation not permitted on the root directory"),
            Error::NameTooLong => f.write_str("file name too long (at most 255 characters)"),
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;

/// Local date and time as stored by FAT (2-second resolution, years 1980..=2107).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Long name if present, otherwise the 8.3 name. Empty for the root directory.
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub created: Timestamp,
    pub modified: Timestamp,
    pub read_only: bool,
    pub hidden: bool,
    pub system: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsStats {
    pub label: String,
    pub volume_id: u32,
    pub cluster_size: u32,
    pub total_clusters: u32,
    pub free_clusters: u32,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Up to 11 ASCII characters; stored upper-case.
    pub label: String,
    pub volume_id: u32,
    /// `None` picks the size from the volume size (and guarantees >= 65525 clusters).
    pub sectors_per_cluster: Option<u8>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self { label: String::from("RUSTKERNAL"), volume_id: 0x5255_5354, sectors_per_cluster: None }
    }
}

/// Writes an empty FAT32 volume covering the whole `device`. FAT32 needs at least 65525
/// clusters; returns `Error::Unsupported` for devices that are too small.
pub fn format<D: BlockDevice>(device: &mut D, options: &FormatOptions) -> Result<()> {
    format::format(device, options)
}

/// Checks whether `name` can be used for a new file or directory (one path component): not
/// empty, not `.`/`..`, none of the characters FAT forbids (`" * / : < > ? \ |` and control
/// characters), no trailing dot or space, and at most 255 UTF-16 units. Returns `InvalidName` or
/// `NameTooLong` otherwise.
pub fn validate_name(name: &str) -> Result<()> {
    name::validate_name(name)
}

/// Remembers where the last walk along a cluster chain ended so sequential access does not
/// re-walk the chain from its first cluster every time.
#[derive(Debug, Clone, Copy)]
struct ChainHint {
    first: u32,
    index: u32,
    cluster: u32,
}

const CHAIN_HINTS: usize = 4;

/// A mounted FAT32 volume.
///
/// Paths are absolute, `/`-separated and case-insensitive (FAT semantics); `.` and `..`
/// components are resolved. Every mutating call leaves the on-disk structures consistent when it
/// returns (data before directory entries, both FAT copies, FSInfo hint); `sync` additionally
/// flushes the device.
pub struct FileSystem<D: BlockDevice> {
    device: D,
    geo: boot_sector::Geometry,
    fat: fat::FatCache,
    /// Free cluster count (always known after mount; recomputed when FSInfo is unreliable).
    free_count: u32,
    /// Where the next free-cluster search starts.
    next_free: u32,
    /// In-memory copy of the FSInfo sector (`None` when the volume has none).
    fsinfo: Option<[u8; SECTOR_SIZE]>,
    /// `(free_count, next_free)` as last written to the FSInfo sector.
    fsinfo_on_disk: (u32, u32),
    /// Whether the clean-shutdown bit is currently cleared on disk.
    volume_dirty: bool,
    /// Whether the clean-shutdown bit was set when the volume was mounted.
    was_clean: bool,
    /// The device rejected writes at mount time; every mutating call fails.
    read_only: bool,
    /// A failed rename could not be undone (see the crate docs); every mutating call fails and the
    /// volume is not marked clean, so nothing frees clusters that two entries share before the
    /// next mount removes the duplicate.
    poisoned: bool,
    now: fn() -> Timestamp,
    chain_hints: [Option<ChainHint>; CHAIN_HINTS],
    next_hint: usize,
    label: String,
}

impl<D: BlockDevice> FileSystem<D> {
    /// Validates the boot sector and FSInfo and mounts the volume. Marks the volume dirty (FAT[1]
    /// clean-shutdown bit) until `sync`/`unmount`. After an unclean shutdown it also re-mirrors
    /// the FAT copies and removes the duplicate entry an interrupted rename may have left (this
    /// walks the whole directory tree).
    pub fn mount(device: D) -> Result<Self> {
        ops::mount(device)
    }

    /// Flushes everything, marks the volume clean and returns the device.
    pub fn unmount(mut self) -> Result<D> {
        self.sync()?;
        Ok(self.device)
    }

    /// Writes all pending state, marks the volume clean and flushes the device.
    pub fn sync(&mut self) -> Result<()> {
        self.sync_all()
    }

    /// Sets the clock used for created/modified timestamps.
    pub fn set_time_source(&mut self, now: fn() -> Timestamp) {
        self.now = now;
    }

    pub fn stats(&mut self) -> Result<FsStats> {
        let cluster_size = self.geo.cluster_size;
        Ok(FsStats {
            label: self.label.clone(),
            volume_id: self.geo.volume_id,
            cluster_size,
            total_clusters: self.geo.cluster_count,
            free_clusters: self.free_count,
            total_bytes: self.geo.cluster_count as u64 * cluster_size as u64,
            free_bytes: self.free_count as u64 * cluster_size as u64,
        })
    }

    pub fn metadata(&mut self, path: &str) -> Result<Metadata> {
        self.op_metadata(path)
    }

    pub fn exists(&mut self, path: &str) -> Result<bool> {
        match self.resolve_path(path) {
            Ok(_) => Ok(true),
            Err(Error::NotFound) | Err(Error::NotADirectory) => Ok(false),
            Err(error) => Err(error),
        }
    }

    /// Entries of a directory, without `.` and `..`, in on-disk order.
    pub fn read_dir(&mut self, path: &str) -> Result<Vec<Metadata>> {
        self.op_read_dir(path)
    }

    /// Creates one directory; the parent must exist.
    pub fn create_dir(&mut self, path: &str) -> Result<()> {
        self.mutate(|fs| fs.op_create_dir(path))
    }

    /// Creates a directory and any missing parents; succeeds if it already exists.
    pub fn create_dir_all(&mut self, path: &str) -> Result<()> {
        self.mutate(|fs| fs.op_create_dir_all(path))
    }

    /// Removes an empty directory.
    pub fn remove_dir(&mut self, path: &str) -> Result<()> {
        self.mutate(|fs| fs.op_remove_dir(path))
    }

    /// Removes a directory and everything below it.
    pub fn remove_dir_all(&mut self, path: &str) -> Result<()> {
        self.mutate(|fs| fs.op_remove_dir_all(path))
    }

    /// Creates an empty file if missing; otherwise updates its modified time (like `touch`).
    pub fn touch(&mut self, path: &str) -> Result<()> {
        self.mutate(|fs| fs.op_touch(path))
    }

    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>> {
        self.op_read_file(path)
    }

    /// Reads up to `buffer.len()` bytes at `offset`; returns bytes read (0 at end of file).
    pub fn read_at(&mut self, path: &str, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        self.op_read_at(path, offset, buffer)
    }

    /// Creates or replaces a file with `data`.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.mutate(|fs| fs.op_write_file(path, data))
    }

    /// Appends to a file, creating it if missing.
    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.mutate(|fs| fs.op_append_file(path, data))
    }

    /// Writes `data` at `offset`, growing the file (zero-filled) if needed. File must exist.
    pub fn write_at(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        self.mutate(|fs| fs.op_write_at(path, offset, data))
    }

    /// Shrinks or zero-extends a file to `length` bytes.
    pub fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        self.mutate(|fs| fs.op_truncate(path, length))
    }

    pub fn remove_file(&mut self, path: &str) -> Result<()> {
        self.mutate(|fs| fs.op_remove_file(path))
    }

    /// Renames or moves a file or directory. Fails with `AlreadyExists` if `to` exists, with
    /// `InvalidMove` when moving a directory into itself. After any other error the old name
    /// remains, except when the old entry was already removed when the error occurred: then only
    /// the new name remains (at worst with orphaned long-name entries left behind).
    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        self.mutate(|fs| fs.op_rename(from, to))
    }

    /// Copies a file's contents to a new file (fails if `to` exists).
    pub fn copy_file(&mut self, from: &str, to: &str) -> Result<()> {
        self.mutate(|fs| fs.op_copy_file(from, to))
    }

    /// Read-only consistency check (like `fsck -n`): walks every directory and cluster chain and
    /// reports cross-linked, lost or out-of-range clusters, bad chain lengths, invalid directory
    /// entries, broken `.`/`..` entries, orphaned long-name entries and a wrong FSInfo free count.
    pub fn check(&mut self) -> Result<CheckReport> {
        check::check(self)
    }

    pub fn device(&self) -> &D {
        &self.device
    }

    /// Whether the device refused writes at mount time (every mutating call then fails with
    /// `Error::Device(DeviceError::ReadOnly)`).
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the clean-shutdown bit was set when the volume was mounted.
    pub fn was_cleanly_unmounted(&self) -> bool {
        self.was_clean
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Human readable descriptions of every problem found; empty means the volume is consistent.
    pub problems: Vec<String>,
    pub files: u32,
    pub directories: u32,
    pub used_clusters: u32,
    /// Clusters marked used in the FAT but not reachable from any directory entry.
    pub lost_clusters: u32,
    /// Whether the clean-shutdown bit was set when mounted.
    pub was_cleanly_unmounted: bool,
}

impl CheckReport {
    /// True when no problems were found.
    pub fn is_clean(&self) -> bool {
        self.problems.is_empty()
    }
}
