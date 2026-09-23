//! Manual checks of a real disk image, e.g. the data disk after a session in the kernel.
//! Ignored by default because they need an image file:
//!
//! ```text
//! RUSTKERNAL_IMAGE=path/to/disk.img cargo test -p fat32 --test disk_image inspect -- --ignored --nocapture
//! ```
//!
//! * `inspect` (read-only) lists every file with our crate and with the independent `fatfs`
//!   crate, requires both to agree and runs `check()`, which must report no problems.
//! * `seed` MODIFIES the image: it adds `/host/binary.bin` and `/host/big.txt` and leaves the
//!   volume without a final sync (the "volume in use" flag stays set), so the next mount sees an
//!   unclean shutdown. Only run it on a scratch copy.

use fat32::mbr::{self, PartitionDevice};
use fat32::{BlockDevice, DeviceError, FileSystem, SECTOR_SIZE};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

struct FileDisk {
    file: File,
    sectors: u64,
}

impl FileDisk {
    fn open(path: &str, writable: bool) -> Self {
        let file = OpenOptions::new().read(true).write(writable).open(path).expect("open image");
        let sectors = file.metadata().unwrap().len() / SECTOR_SIZE as u64;
        Self { file, sectors }
    }
}

impl BlockDevice for FileDisk {
    fn sector_count(&self) -> u64 {
        self.sectors
    }
    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        self.file.seek(SeekFrom::Start(lba * SECTOR_SIZE as u64)).map_err(|_| DeviceError::Io)?;
        self.file.read_exact(buffer).map_err(|_| DeviceError::Io)
    }
    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        self.file.seek(SeekFrom::Start(lba * SECTOR_SIZE as u64)).map_err(|_| DeviceError::Io)?;
        self.file.write_all(buffer).map_err(|_| DeviceError::Io)
    }
    fn flush(&mut self) -> Result<(), DeviceError> {
        self.file.sync_data().map_err(|_| DeviceError::Io)
    }
}

fn image_path() -> String {
    std::env::var("RUSTKERNAL_IMAGE").expect("set RUSTKERNAL_IMAGE to a disk image")
}

/// The first partition of the image (or the whole image for a partitionless volume).
fn volume(path: &str, writable: bool) -> (PartitionDevice<FileDisk>, mbr::Partition) {
    let mut disk = FileDisk::open(path, writable);
    let partitions = mbr::read_partitions(&mut disk).expect("read MBR");
    let partition = partitions.first().copied().unwrap_or(mbr::Partition {
        index: 0,
        partition_type: 0x0C,
        bootable: false,
        start_lba: 0,
        sector_count: disk.sector_count(),
    });
    (PartitionDevice::new(disk, &partition), partition)
}

/// path -> (is_dir, contents) with our crate.
fn walk_ours<D: BlockDevice>(fs: &mut FileSystem<D>, dir: &str, out: &mut BTreeMap<String, (bool, Vec<u8>)>) {
    for entry in fs.read_dir(dir).unwrap() {
        let path = if dir == "/" { format!("/{}", entry.name) } else { format!("{}/{}", dir, entry.name) };
        if entry.is_dir {
            out.insert(path.clone(), (true, Vec::new()));
            walk_ours(fs, &path, out);
        } else {
            let data = fs.read_file(&path).unwrap();
            assert_eq!(data.len() as u64, entry.size, "{}: size", path);
            out.insert(path, (false, data));
        }
    }
}

/// The same with fatfs.
fn walk_fatfs<T: fatfs::ReadWriteSeek>(dir: fatfs::Dir<T>, prefix: &str, out: &mut BTreeMap<String, (bool, Vec<u8>)>) {
    for entry in dir.iter() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let path = format!("{}/{}", prefix, name);
        if entry.is_dir() {
            out.insert(path.clone(), (true, Vec::new()));
            walk_fatfs(entry.to_dir(), &path, out);
        } else {
            let mut data = Vec::new();
            entry.to_file().read_to_end(&mut data).unwrap();
            out.insert(path, (false, data));
        }
    }
}

#[test]
#[ignore]
fn inspect() {
    let path = image_path();
    let (device, partition) = volume(&path, false);
    println!("partition {}: type 0x{:02x}, start {}, {} sectors", partition.index + 1, partition.partition_type, partition.start_lba, partition.sector_count);

    // Read the partition into memory: fatfs gets its own copy, and nothing can touch the file.
    let mut bytes = vec![0u8; (partition.sector_count as usize) * SECTOR_SIZE];
    let mut device = device;
    device.read_sectors(0, &mut bytes).unwrap();

    let mut fs = FileSystem::mount(fat32_ram(bytes.clone())).expect("mount with fat32");
    println!("was cleanly unmounted: {}", fs.was_cleanly_unmounted());
    let stats = fs.stats().unwrap();
    println!("label {:?}, {} of {} bytes free, cluster size {}", stats.label, stats.free_bytes, stats.total_bytes, stats.cluster_size);

    let mut ours = BTreeMap::new();
    walk_ours(&mut fs, "/", &mut ours);
    for (path, (is_dir, data)) in &ours {
        if *is_dir {
            println!("  {}/", path);
        } else if data.len() <= 200 && data.iter().all(|&b| b == b'\n' || b == b'\t' || (0x20..0x7F).contains(&b)) {
            println!("  {} ({} bytes): {:?}", path, data.len(), String::from_utf8_lossy(data));
        } else {
            let fnv = data.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, &byte| (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3));
            println!("  {} ({} bytes, binary or long, FNV-1a {:016x})", path, data.len(), fnv);
        }
    }

    let report = fs.check().unwrap();
    println!("check: {} files, {} directories, {} used clusters, {} lost, problems: {:?}", report.files, report.directories, report.used_clusters, report.lost_clusters, report.problems);

    let fatfs = fatfs::FileSystem::new(Cursor::new(bytes), fatfs::FsOptions::new()).expect("mount with fatfs");
    let mut theirs = BTreeMap::new();
    walk_fatfs(fatfs.root_dir(), "", &mut theirs);
    assert_eq!(ours.keys().collect::<Vec<_>>(), theirs.keys().collect::<Vec<_>>(), "fatfs sees different names");
    assert!(ours == theirs, "fatfs reads different contents");
    println!("fatfs agrees on {} entries", theirs.len());
    assert!(report.is_clean(), "check() found problems: {:?}", report.problems);
}

#[test]
#[ignore]
fn seed() {
    let path = image_path();
    let (device, _) = volume(&path, true);
    let mut fs = FileSystem::mount(device).expect("mount");
    fs.create_dir_all("/host").unwrap();
    let binary: Vec<u8> = (0..=255u8).chain(0..=255u8).collect();
    fs.write_file("/host/binary.bin", &binary).unwrap();
    let mut text = String::new();
    for line in 0..20000 {
        text.push_str(&format!("line {:05} of a large text file written on the host\n", line));
    }
    fs.write_file("/host/big.txt", text.as_bytes()).unwrap();
    println!("seeded /host/binary.bin (512 bytes) and /host/big.txt ({} bytes); volume left unsynced", text.len());
    // No sync/unmount: the next mount must notice the unclean shutdown.
    std::mem::forget(fs);
}

/// An in-memory copy of a volume.
fn fat32_ram(bytes: Vec<u8>) -> RamVolume {
    RamVolume(bytes)
}

struct RamVolume(Vec<u8>);

impl BlockDevice for RamVolume {
    fn sector_count(&self) -> u64 {
        (self.0.len() / SECTOR_SIZE) as u64
    }
    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        let start = lba as usize * SECTOR_SIZE;
        buffer.copy_from_slice(self.0.get(start..start + buffer.len()).ok_or(DeviceError::OutOfRange)?);
        Ok(())
    }
    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        let start = lba as usize * SECTOR_SIZE;
        self.0.get_mut(start..start + buffer.len()).ok_or(DeviceError::OutOfRange)?.copy_from_slice(buffer);
        Ok(())
    }
    fn flush(&mut self) -> Result<(), DeviceError> {
        Ok(())
    }
}
