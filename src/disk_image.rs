//! The persistent data disk the kernel sees as `hd1`.
//!
//! On first use the runner creates a raw disk image with an MBR, one FAT32 partition and a
//! `README.TXT`, using the same `fat32` crate the kernel mounts it with. An existing image is never
//! modified: it holds whatever the user saved from inside the OS.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fat32::{BlockDevice, DeviceError, FileSystem, FormatOptions, PartitionDevice, SECTOR_SIZE, Timestamp};

/// Size of a freshly created data disk.
pub const DEFAULT_DISK_SIZE: u64 = 128 * 1024 * 1024;

/// Environment variable that points the runner at a different data disk image.
pub const DISK_PATH_VARIABLE: &str = "RUSTKERNAL_DISK";

const README_PATH: &str = "/README.TXT";
const README_TEXT: &str = "\
Welcome to RustKernal's data disk!

This is drive hd1: a FAT32 partition on a disk image that lives on the host
(target/disk.img in the repository, or the file named by the RUSTKERNAL_DISK
environment variable). Everything you save here survives reboots and
`cargo run` restarts.

The runner created this disk because none existed. It never touches an
existing image, so delete the file on the host to start over with a fresh,
empty disk. You can also inspect it from the host: it is an ordinary MBR disk
image with one FAT32 partition that most operating systems can mount.
";

/// Where the data disk lives: `$RUSTKERNAL_DISK`, or `target/disk.img` in the workspace.
pub fn disk_path() -> PathBuf {
    match std::env::var_os(DISK_PATH_VARIABLE) {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("disk.img"),
    }
}

/// A raw disk image on the host, addressed in 512-byte sectors.
pub struct FileDisk {
    file: File,
    sector_count: u64,
}

impl FileDisk {
    /// Opens an existing image for reading and writing. Its size must be a multiple of 512 bytes.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        let length = file.metadata()?.len();
        if length % SECTOR_SIZE as u64 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{} is {} bytes, not a whole number of sectors", path.display(), length),
            ));
        }
        Ok(Self { file, sector_count: length / SECTOR_SIZE as u64 })
    }

    fn seek_to(&mut self, lba: u64, length: usize) -> Result<(), DeviceError> {
        let count = (length / SECTOR_SIZE) as u64;
        if length % SECTOR_SIZE != 0 || lba.checked_add(count).is_none_or(|end| end > self.sector_count) {
            return Err(DeviceError::OutOfRange);
        }
        self.file.seek(SeekFrom::Start(lba * SECTOR_SIZE as u64)).map_err(|_| DeviceError::Io)?;
        Ok(())
    }
}

impl BlockDevice for FileDisk {
    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        self.seek_to(lba, buffer.len())?;
        self.file.read_exact(buffer).map_err(|_| DeviceError::Io)
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        self.seek_to(lba, buffer.len())?;
        self.file.write_all(buffer).map_err(|_| DeviceError::Io)
    }

    fn flush(&mut self) -> Result<(), DeviceError> {
        self.file.sync_data().map_err(|_| DeviceError::Io)
    }
}

/// Deletes a half-built image if creation fails or panics, so a broken file is never mistaken
/// for the user's disk.
struct RemoveOnDrop<'a> {
    path: &'a Path,
    armed: bool,
}

impl Drop for RemoveOnDrop<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(self.path);
        }
    }
}

/// Makes sure the data disk at `path` exists, creating and formatting it if it does not.
/// Returns whether a new disk was created.
pub fn ensure_data_disk(path: &Path) -> Result<bool, String> {
    if path.exists() {
        let metadata = fs::metadata(path).map_err(|error| format!("cannot read {}: {}", path.display(), error))?;
        if !metadata.is_file() {
            return Err(format!("{} exists but is not a file", path.display()));
        }
        println!("Data disk: {} ({} MiB, existing, left untouched)", path.display(), metadata.len() / (1024 * 1024));
        return Ok(false);
    }

    println!("Data disk: {} does not exist, creating it", path.display());
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| format!("cannot create {}: {}", parent.display(), error))?;
    }

    // Build the image under a temporary name and rename it into place only when it is complete,
    // so an interrupted run never leaves a half-formatted disk behind at `path`.
    let mut partial_name = path.file_name().unwrap_or_default().to_os_string();
    partial_name.push(".partial");
    let partial = path.with_file_name(partial_name);
    let _ = fs::remove_file(&partial);
    let mut cleanup = RemoveOnDrop { path: &partial, armed: true };

    create_image(&partial, DEFAULT_DISK_SIZE)?;
    fs::rename(&partial, path)
        .map_err(|error| format!("cannot move {} to {}: {}", partial.display(), path.display(), error))?;
    cleanup.armed = false;
    println!("  done: {} is ready", path.display());
    Ok(true)
}

fn create_image(path: &Path, size: u64) -> Result<(), String> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {}", path.display(), error))?;
    file.set_len(size).map_err(|error| format!("cannot resize {}: {}", path.display(), error))?;
    drop(file);

    let mut disk = FileDisk::open(path).map_err(|error| format!("cannot open {}: {}", path.display(), error))?;
    println!("  created a {} MiB raw disk image", size / (1024 * 1024));

    let partition = fat32::mbr::write_single_partition(&mut disk, fat32::mbr::PARTITION_TYPE_FAT32_LBA)
        .map_err(|error| format!("cannot write the partition table: {:?}", error))?;
    println!(
        "  wrote an MBR with one FAT32 partition: sectors {}..{} ({} MiB)",
        partition.start_lba,
        partition.start_lba + partition.sector_count,
        partition.sector_count * SECTOR_SIZE as u64 / (1024 * 1024)
    );

    let options = FormatOptions::default();
    fat32::format(&mut PartitionDevice::new(&mut disk, &partition), &options)
        .map_err(|error| format!("cannot format the partition: {:?}", error))?;
    println!("  formatted the partition as FAT32 (label {})", options.label);

    let mut filesystem = FileSystem::mount(PartitionDevice::new(&mut disk, &partition))
        .map_err(|error| format!("cannot mount the new file system: {:?}", error))?;
    filesystem.set_time_source(host_time);
    filesystem
        .write_file(README_PATH, README_TEXT.as_bytes())
        .map_err(|error| format!("cannot write {}: {:?}", README_PATH, error))?;
    filesystem.unmount().map_err(|error| format!("cannot unmount the new file system: {:?}", error))?;
    println!("  wrote {} ({} bytes)", README_PATH, README_TEXT.len());

    disk.flush().map_err(|error| format!("cannot flush {}: {:?}", path.display(), error))?;
    Ok(())
}

/// The host's current time (UTC) for file timestamps.
fn host_time() -> Timestamp {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_secs()).unwrap_or(0);
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    let second_of_day = seconds % 86_400;
    Timestamp {
        year: year.clamp(1980, 2107) as u16,
        month,
        day,
        hour: (second_of_day / 3600) as u8,
        minute: (second_of_day / 60 % 60) as u8,
        second: (second_of_day % 60) as u8,
    }
}

/// Converts days since 1970-01-01 to a (year, month, day) date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u8, u8) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u8;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 } as u8;
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_719), (2026, 9, 23));
    }

    #[test]
    fn file_disk_round_trip_and_bounds() {
        let path = std::env::temp_dir().join(format!("rustkernal-filedisk-{}.img", std::process::id()));
        File::create(&path).unwrap().set_len(8 * SECTOR_SIZE as u64).unwrap();
        let mut disk = FileDisk::open(&path).unwrap();
        assert_eq!(disk.sector_count(), 8);

        let data: Vec<u8> = (0..2 * SECTOR_SIZE).map(|index| index as u8).collect();
        disk.write_sectors(6, &data).unwrap();
        disk.flush().unwrap();
        let mut back = vec![0; 2 * SECTOR_SIZE];
        disk.read_sectors(6, &mut back).unwrap();
        assert_eq!(back, data);

        assert_eq!(disk.read_sectors(7, &mut back), Err(DeviceError::OutOfRange));
        assert_eq!(disk.write_sectors(u64::MAX, &data), Err(DeviceError::OutOfRange));
        assert_eq!(disk.read_sectors(0, &mut [0; 100]), Err(DeviceError::OutOfRange));
        drop(disk);
        fs::remove_file(&path).unwrap();
    }
}
