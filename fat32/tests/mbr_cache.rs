//! MBR partitions, partition windows and the write-back cache end to end.

mod common;

use common::*;
use fat32::mbr::{PARTITION_TYPE_FAT32_LBA, read_partitions, write_single_partition};
use fat32::{BlockDevice, CachedDevice, DeviceError, FileSystem, FormatOptions, PartitionDevice, SECTOR_SIZE};

#[test]
fn partitioned_disk_round_trip() {
    let mut disk = RamDisk::new(64 * MIB);
    assert!(read_partitions(&mut disk).unwrap().is_empty(), "blank disk has no MBR");
    let partition = write_single_partition(&mut disk, PARTITION_TYPE_FAT32_LBA).unwrap();
    assert_eq!(partition.index, 0);
    assert_eq!(partition.start_lba, 2048);
    assert_eq!(partition.sector_count, (64 * MIB / SECTOR_SIZE) as u64 - 2048);
    assert!(!partition.bootable);
    assert_eq!(&disk.data[510..512], &[0x55, 0xAA]);
    let entry = &disk.data[0x1BE..0x1CE];
    assert_eq!(entry[0], 0);
    assert_eq!(entry[4], 0x0C);
    assert_eq!(u32::from_le_bytes(entry[8..12].try_into().unwrap()), 2048);
    assert_eq!(read_partitions(&mut disk).unwrap(), vec![partition]);

    let mut window = PartitionDevice::new(disk, &partition);
    assert_eq!(window.sector_count(), partition.sector_count);
    fat32::format(&mut window, &FormatOptions::default()).unwrap();
    let mut fs = FileSystem::mount(window).unwrap();
    fs.create_dir("/inside").unwrap();
    fs.write_file("/inside/partition.txt", b"partitioned").unwrap();
    assert_clean(&mut fs);
    let mut disk = fs.unmount().unwrap().into_inner();

    // The MBR survived formatting the partition, and the volume starts at sector 2048.
    assert_eq!(read_partitions(&mut disk).unwrap(), vec![partition]);
    assert_eq!(&disk.data[2048 * 512 + 82..2048 * 512 + 90], b"FAT32   ");
    let partitions = read_partitions(&mut disk).unwrap();
    let mut fs = FileSystem::mount(PartitionDevice::new(disk, &partitions[0])).unwrap();
    assert_eq!(fs.read_file("/inside/partition.txt").unwrap(), b"partitioned");

    // fatfs agrees when given just the partition.
    let disk = fs.unmount().unwrap().into_inner();
    let mut image = disk.data[2048 * 512..].to_vec();
    let tree = dump_fatfs(&mut image);
    assert_eq!(tree["/inside/partition.txt"], Some(b"partitioned".to_vec()));
}

#[test]
fn superfloppy_and_garbage_are_not_partition_tables() {
    let mut disk = RamDisk::new(40 * MIB);
    fat32::format(&mut disk, &FormatOptions::default()).unwrap();
    assert!(read_partitions(&mut disk).unwrap().is_empty(), "FAT boot sector is not an MBR");
    // A fatfs-made superfloppy too.
    let mut image = fatfs_format(40 * MIB, b"FLOPPY     ");
    assert!(read_partitions(&mut RamDisk::from_vec(std::mem::take(&mut image))).unwrap().is_empty());
    // Signature but invalid status bytes.
    let mut disk = RamDisk::new(4 * MIB);
    disk.data[510] = 0x55;
    disk.data[511] = 0xAA;
    disk.data[0x1BE] = 0x12;
    disk.data[0x1BE + 4] = 0x0C;
    disk.data[0x1BE + 8] = 1;
    disk.data[0x1BE + 12] = 1;
    assert!(read_partitions(&mut disk).unwrap().is_empty());
    // Empty entries are skipped; others keep their index and flags.
    let mut disk = RamDisk::new(4 * MIB);
    disk.data[510] = 0x55;
    disk.data[511] = 0xAA;
    let entry = 0x1BE + 2 * 16;
    disk.data[entry] = 0x80;
    disk.data[entry + 4] = 0x83;
    disk.data[entry + 8..entry + 12].copy_from_slice(&100u32.to_le_bytes());
    disk.data[entry + 12..entry + 16].copy_from_slice(&500u32.to_le_bytes());
    let partitions = read_partitions(&mut disk).unwrap();
    assert_eq!(partitions.len(), 1);
    assert_eq!(partitions[0].index, 2);
    assert!(partitions[0].bootable);
    assert_eq!(partitions[0].partition_type, 0x83);
    assert_eq!(partitions[0].start_lba, 100);
    assert_eq!(partitions[0].sector_count, 500);
    // Too small for a partition.
    let mut tiny = RamDisk::new(512 * 1000);
    assert!(write_single_partition(&mut tiny, PARTITION_TYPE_FAT32_LBA).is_err());
}

#[test]
fn partition_device_bounds() {
    let disk = RamDisk::new(4 * MIB);
    let partition =
        fat32::Partition { index: 0, partition_type: 0x0C, bootable: false, start_lba: 100, sector_count: 50 };
    let mut window = PartitionDevice::new(disk, &partition);
    let mut sector = vec![0u8; SECTOR_SIZE];
    window.write_sectors(0, &vec![0xAB; SECTOR_SIZE]).unwrap();
    window.write_sectors(49, &vec![0xCD; SECTOR_SIZE]).unwrap();
    assert_eq!(window.inner().data[100 * 512], 0xAB);
    assert_eq!(window.inner().data[149 * 512], 0xCD);
    window.read_sectors(49, &mut sector).unwrap();
    assert_eq!(sector[0], 0xCD);
    assert_eq!(window.read_sectors(50, &mut sector), Err(DeviceError::OutOfRange));
    assert_eq!(window.write_sectors(49, &vec![0; 2 * SECTOR_SIZE]), Err(DeviceError::OutOfRange));
    assert_eq!(window.read_sectors(u64::MAX, &mut sector), Err(DeviceError::OutOfRange));
    assert_eq!(window.inner().data[150 * 512], 0, "nothing written past the partition");
}

#[test]
fn cached_device_persists_through_unmount_and_remount() {
    let mut disk = RamDisk::new(64 * MIB);
    fat32::format(&mut disk, &FormatOptions::default()).unwrap();
    let mut fs = FileSystem::mount(CachedDevice::new(disk, 256)).unwrap();
    let mut tree = Tree::new();
    fs.create_dir_all("/a/b/c").unwrap();
    for dir in ["/a", "/a/b", "/a/b/c"] {
        tree.insert(dir.into(), None);
    }
    for i in 0..100 {
        let path = format!("/a/b/c/cached file {}.bin", i);
        let data = pattern(i * 123, i as u64);
        fs.write_file(&path, &data).unwrap();
        tree.insert(path, Some(data));
    }
    fs.rename("/a/b/c/cached file 1.bin", "/a/renamed.bin").unwrap();
    let data = tree.remove("/a/b/c/cached file 1.bin").unwrap();
    tree.insert("/a/renamed.bin".into(), data);
    assert_eq!(dump_ours(&mut fs), tree);
    assert!(fs.device().stats().hits > 0);
    assert!(fs.device().dirty_sectors() > 0, "writes are held back by the cache");

    // unmount flushes the filesystem, into_inner flushes the cache.
    let cache = fs.unmount().unwrap();
    assert_eq!(cache.dirty_sectors(), 0, "sync flushes the device");
    let disk = cache.into_inner().unwrap();
    assert!(disk.flushes >= 1);
    let mut fs = FileSystem::mount(disk).unwrap();
    assert!(fs.was_cleanly_unmounted());
    assert_eq!(dump_ours(&mut fs), tree);
    assert_clean(&mut fs);
    let mut image = fs.unmount().unwrap().data;
    assert_eq!(dump_fatfs(&mut image), tree);
}

#[test]
fn cached_device_loses_unflushed_writes_only() {
    // Without sync the backing disk may be stale, but it is never inconsistent in a way that
    // breaks mounting: the last synced state stays readable.
    let mut disk = RamDisk::new(40 * MIB);
    fat32::format(&mut disk, &FormatOptions::default()).unwrap();
    let mut fs = FileSystem::mount(CachedDevice::new(disk, 4096)).unwrap();
    fs.write_file("/synced.txt", b"durable").unwrap();
    fs.sync().unwrap();
    let snapshot = fs.device().inner().clone();
    fs.write_file("/unsynced.txt", b"volatile").unwrap();
    let mut fs2 = FileSystem::mount(snapshot).unwrap();
    assert_eq!(fs2.read_file("/synced.txt").unwrap(), b"durable");
    assert!(!fs2.exists("/unsynced.txt").unwrap());
    assert_clean(&mut fs2);
}
