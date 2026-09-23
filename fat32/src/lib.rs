//! FAT32 filesystem for RustKernal.
//!
//! `no_std` + `alloc`, so the same code runs in the kernel (on the ATA driver) and on the host (in
//! tests and in the QEMU runner, which pre-formats the data disk image).
//!
//! PUBLIC API CONTRACT: the kernel and the runner are written against the signatures in this file.
//! Implementations may add items but must not change or remove the ones declared here.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub mod cache;
pub mod mbr;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Device(DeviceError),
    NotFound,
    AlreadyExists,
    NotADirectory,
    IsADirectory,
    DirectoryNotEmpty,
    /// Name contains characters FAT does not allow, is empty, `.`/`..` where not allowed, or is
    /// longer than 255 UTF-16 units.
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
}

impl From<DeviceError> for Error {
    fn from(error: DeviceError) -> Self {
        Error::Device(error)
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        todo!("human readable message, e.g. 'no such file or directory'")
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
    todo!()
}

/// A mounted FAT32 volume.
///
/// Paths are absolute, `/`-separated and case-insensitive (FAT semantics); `.` and `..`
/// components are resolved. Every mutating call leaves the on-disk structures consistent when it
/// returns (data before directory entries, both FAT copies, FSInfo hint); `sync` additionally
/// flushes the device.
pub struct FileSystem<D: BlockDevice> {
    device: D,
    // implementation state
}

impl<D: BlockDevice> FileSystem<D> {
    /// Validates the boot sector and FSInfo and mounts the volume. Marks the volume dirty (FAT[1]
    /// clean-shutdown bit) until `sync`/`unmount`.
    pub fn mount(device: D) -> Result<Self> {
        todo!()
    }

    /// Flushes everything, marks the volume clean and returns the device.
    pub fn unmount(self) -> Result<D> {
        todo!()
    }

    /// Writes all pending state, marks the volume clean and flushes the device.
    pub fn sync(&mut self) -> Result<()> {
        todo!()
    }

    /// Sets the clock used for created/modified timestamps.
    pub fn set_time_source(&mut self, now: fn() -> Timestamp) {
        todo!()
    }

    pub fn stats(&mut self) -> Result<FsStats> {
        todo!()
    }

    pub fn metadata(&mut self, path: &str) -> Result<Metadata> {
        todo!()
    }

    pub fn exists(&mut self, path: &str) -> Result<bool> {
        todo!()
    }

    /// Entries of a directory, without `.` and `..`, in on-disk order.
    pub fn read_dir(&mut self, path: &str) -> Result<Vec<Metadata>> {
        todo!()
    }

    /// Creates one directory; the parent must exist.
    pub fn create_dir(&mut self, path: &str) -> Result<()> {
        todo!()
    }

    /// Creates a directory and any missing parents; succeeds if it already exists.
    pub fn create_dir_all(&mut self, path: &str) -> Result<()> {
        todo!()
    }

    /// Removes an empty directory.
    pub fn remove_dir(&mut self, path: &str) -> Result<()> {
        todo!()
    }

    /// Removes a directory and everything below it.
    pub fn remove_dir_all(&mut self, path: &str) -> Result<()> {
        todo!()
    }

    /// Creates an empty file if missing; otherwise updates its modified time (like `touch`).
    pub fn touch(&mut self, path: &str) -> Result<()> {
        todo!()
    }

    pub fn read_file(&mut self, path: &str) -> Result<Vec<u8>> {
        todo!()
    }

    /// Reads up to `buffer.len()` bytes at `offset`; returns bytes read (0 at end of file).
    pub fn read_at(&mut self, path: &str, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        todo!()
    }

    /// Creates or replaces a file with `data`.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        todo!()
    }

    /// Appends to a file, creating it if missing.
    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        todo!()
    }

    /// Writes `data` at `offset`, growing the file (zero-filled) if needed. File must exist.
    pub fn write_at(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        todo!()
    }

    /// Shrinks or zero-extends a file to `length` bytes.
    pub fn truncate(&mut self, path: &str, length: u64) -> Result<()> {
        todo!()
    }

    pub fn remove_file(&mut self, path: &str) -> Result<()> {
        todo!()
    }

    /// Renames or moves a file or directory. Fails with `AlreadyExists` if `to` exists, with
    /// `InvalidMove` when moving a directory into itself.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<()> {
        todo!()
    }

    /// Copies a file's contents to a new file (fails if `to` exists).
    pub fn copy_file(&mut self, from: &str, to: &str) -> Result<()> {
        todo!()
    }

    /// Read-only consistency check (like `fsck -n`): walks every directory and cluster chain and
    /// reports cross-linked, lost or out-of-range clusters, bad chain lengths, invalid directory
    /// entries, broken `.`/`..` entries, orphaned long-name entries and a wrong FSInfo free count.
    pub fn check(&mut self) -> Result<CheckReport> {
        todo!()
    }

    pub fn device(&self) -> &D {
        &self.device
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
