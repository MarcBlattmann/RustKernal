//! Tests of every public operation and its error cases.

mod common;

use common::*;
use fat32::{BlockDevice, DeviceError, Error, FileSystem, FormatOptions, SECTOR_SIZE, Timestamp};

const SMALL: usize = 40 * MIB;

fn u16_at(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// Raw layout facts read from a boot sector.
struct Layout {
    spc: usize,
    reserved: usize,
    fats: usize,
    fat_size: usize,
    root: u32,
}

impl Layout {
    fn read(data: &[u8]) -> Layout {
        Layout {
            spc: data[13] as usize,
            reserved: u16_at(data, 14) as usize,
            fats: data[16] as usize,
            fat_size: u32_at(data, 36) as usize,
            root: u32_at(data, 44),
        }
    }
    fn cluster_offset(&self, cluster: u32) -> usize {
        (self.reserved + self.fats * self.fat_size + (cluster as usize - 2) * self.spc) * SECTOR_SIZE
    }
    fn cluster_size(&self) -> usize {
        self.spc * SECTOR_SIZE
    }
    fn fat_entry(&self, data: &[u8], copy: usize, cluster: u32) -> u32 {
        u32_at(data, (self.reserved + copy * self.fat_size) * SECTOR_SIZE + cluster as usize * 4)
    }
    fn set_fat_entry(&self, data: &mut [u8], cluster: u32, value: u32) {
        for copy in 0..self.fats {
            let offset = (self.reserved + copy * self.fat_size) * SECTOR_SIZE + cluster as usize * 4;
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    /// Raw 32-byte entries of the first cluster of directory `cluster`.
    fn entries(&self, data: &[u8], cluster: u32) -> Vec<[u8; 32]> {
        let start = self.cluster_offset(cluster);
        (0..self.cluster_size() / 32).map(|i| data[start + i * 32..start + i * 32 + 32].try_into().unwrap()).collect()
    }
    /// Finds a short entry by its 11-byte name in the first cluster of `dir`.
    fn find(&self, data: &[u8], dir: u32, short: &[u8; 11]) -> Option<(usize, [u8; 32])> {
        self.entries(data, dir).into_iter().enumerate().find(|(_, e)| &e[..11] == short)
    }
}

fn entry_cluster(entry: &[u8; 32]) -> u32 {
    ((u16_at(entry, 20) as u32) << 16) | u16_at(entry, 26) as u32
}

fn ts(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Timestamp {
    Timestamp { year, month, day, hour, minute, second }
}

// ----- format / mount ----------------------------------------------------------------------------

#[test]
fn format_writes_spec_compliant_structures() {
    let mut disk = RamDisk::new(SMALL);
    let options = FormatOptions { label: "My Disk".into(), volume_id: 0xCAFE_BABE, sectors_per_cluster: None };
    fat32::format(&mut disk, &options).unwrap();
    let data = &disk.data;
    assert_eq!(&data[510..512], &[0x55, 0xAA]);
    assert_eq!(data[0], 0xEB);
    assert_eq!(u16_at(data, 11), 512);
    assert_eq!(u16_at(data, 14), 32, "reserved sectors");
    assert_eq!(data[16], 2, "number of FATs");
    assert_eq!(u16_at(data, 17), 0);
    assert_eq!(u16_at(data, 19), 0);
    assert_eq!(data[21], 0xF8);
    assert_eq!(u16_at(data, 22), 0);
    assert_eq!(u32_at(data, 28), 0, "hidden sectors");
    assert_eq!(u32_at(data, 32) as usize, SMALL / 512);
    assert_eq!(u16_at(data, 40), 0, "mirroring enabled");
    assert_eq!(u16_at(data, 42), 0);
    assert_eq!(u32_at(data, 44), 2, "root cluster");
    assert_eq!(u16_at(data, 48), 1);
    assert_eq!(u16_at(data, 50), 6);
    assert_eq!(data[66], 0x29);
    assert_eq!(u32_at(data, 67), 0xCAFE_BABE);
    assert_eq!(&data[71..82], b"MY DISK    ");
    assert_eq!(&data[82..90], b"FAT32   ");
    // Backup boot sector and FSInfo copies.
    assert_eq!(&data[..512], &data[6 * 512..7 * 512]);
    for sector in [1usize, 7] {
        let s = &data[sector * 512..(sector + 1) * 512];
        assert_eq!(u32_at(s, 0), 0x4161_5252);
        assert_eq!(u32_at(s, 484), 0x6141_7272);
        assert_eq!(u32_at(s, 508), 0xAA55_0000);
    }
    let layout = Layout::read(data);
    let data_sectors = SMALL / 512 - 32 - 2 * layout.fat_size;
    let clusters = (data_sectors / layout.spc) as u32;
    assert!(clusters >= 65525);
    assert_eq!(u32_at(data, 512 + 488), clusters - 1, "FSInfo free count");
    for copy in 0..2 {
        assert_eq!(layout.fat_entry(data, copy, 0), 0x0FFF_FFF8);
        assert_eq!(layout.fat_entry(data, copy, 1) & 0x0800_0000, 0x0800_0000, "clean bit");
        assert!(layout.fat_entry(data, copy, 2) >= 0x0FFF_FFF8, "root cluster is end of chain");
        assert_eq!(layout.fat_entry(data, copy, 3), 0);
    }
    // Volume label entry in the root directory.
    let root = layout.entries(data, 2);
    assert_eq!(&root[0][..11], b"MY DISK    ");
    assert_eq!(root[0][11], 0x08);
    assert!(root[1..].iter().all(|e| e.iter().all(|&b| b == 0)));

    let mut fs = FileSystem::mount(disk).unwrap();
    let stats = fs.stats().unwrap();
    assert_eq!(stats.label, "MY DISK");
    assert_eq!(stats.volume_id, 0xCAFE_BABE);
    assert_eq!(stats.cluster_size, 512);
    assert_eq!(stats.total_clusters, clusters);
    assert_eq!(stats.free_clusters, clusters - 1);
    assert_eq!(stats.total_bytes, clusters as u64 * 512);
    assert_eq!(stats.free_bytes, (clusters as u64 - 1) * 512);
    assert!(fs.read_dir("/").unwrap().is_empty());
    let root = fs.metadata("/").unwrap();
    assert!(root.is_dir);
    assert_eq!(root.name, "");
    assert_clean(&mut fs);
}

#[test]
fn format_cluster_sizes() {
    // 300 MiB -> 4 KiB clusters by the Microsoft table.
    let mut disk = RamDisk::new(300 * MIB);
    fat32::format(&mut disk, &FormatOptions::default()).unwrap();
    assert_eq!(disk.data[13], 8);
    let mut fs = FileSystem::mount(disk).unwrap();
    assert_eq!(fs.stats().unwrap().cluster_size, 4096);
    assert_clean(&mut fs);

    // Explicit cluster size.
    let mut disk = RamDisk::new(SMALL);
    let options = FormatOptions { sectors_per_cluster: Some(1), ..FormatOptions::default() };
    fat32::format(&mut disk, &options).unwrap();
    assert_eq!(disk.data[13], 1);
    // Too large a cluster for this size would give fewer than 65525 clusters.
    let options = FormatOptions { sectors_per_cluster: Some(8), ..FormatOptions::default() };
    assert!(matches!(fat32::format(&mut disk, &options), Err(Error::Unsupported(_))));
    let options = FormatOptions { sectors_per_cluster: Some(3), ..FormatOptions::default() };
    assert!(matches!(fat32::format(&mut disk, &options), Err(Error::Unsupported(_))));
}

#[test]
fn format_rejects_small_devices_and_bad_labels() {
    let mut disk = RamDisk::new(16 * MIB);
    assert!(matches!(fat32::format(&mut disk, &FormatOptions::default()), Err(Error::Unsupported(_))));
    let mut disk = RamDisk::new(SMALL);
    let options = FormatOptions { label: "WAY TOO LONG LABEL".into(), ..FormatOptions::default() };
    assert_eq!(fat32::format(&mut disk, &options), Err(Error::InvalidName));
    let options = FormatOptions { label: "BAD*".into(), ..FormatOptions::default() };
    assert_eq!(fat32::format(&mut disk, &options), Err(Error::InvalidName));
    // Nothing was written for rejected options.
    assert!(disk.data.iter().all(|&b| b == 0));
    // Empty label: "NO NAME" in the BPB and no label entry.
    let options = FormatOptions { label: String::new(), ..FormatOptions::default() };
    fat32::format(&mut disk, &options).unwrap();
    assert_eq!(&disk.data[71..82], b"NO NAME    ");
    let mut fs = FileSystem::mount(disk).unwrap();
    assert_eq!(fs.stats().unwrap().label, "");
}

#[test]
fn mount_rejects_invalid_volumes() {
    // Blank disk.
    assert!(matches!(FileSystem::mount(RamDisk::new(SMALL)), Err(Error::Corrupt(_))));
    // FAT16 volume made by fatfs.
    let mut image = vec![0u8; 20 * MIB];
    {
        let mut cursor = std::io::Cursor::new(&mut image);
        fatfs::format_volume(&mut cursor, fatfs::FormatVolumeOptions::new().fat_type(fatfs::FatType::Fat16)).unwrap();
    }
    assert!(matches!(FileSystem::mount(RamDisk::from_vec(image)), Err(Error::Unsupported(_))));

    let mut good = RamDisk::new(SMALL);
    fat32::format(&mut good, &FormatOptions::default()).unwrap();
    let corrupt = |f: &dyn Fn(&mut Vec<u8>)| {
        let mut disk = good.clone();
        f(&mut disk.data);
        FileSystem::mount(disk).err().expect("mount should fail")
    };
    assert!(matches!(corrupt(&|d| d[510] = 0), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[11..13].copy_from_slice(&4096u16.to_le_bytes())), Error::Unsupported(_)));
    assert!(matches!(corrupt(&|d| d[11..13].copy_from_slice(&1000u16.to_le_bytes())), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[13] = 3), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[16] = 0), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[14..16].copy_from_slice(&0u16.to_le_bytes())), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[44..48].copy_from_slice(&0u32.to_le_bytes())), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[44..48].copy_from_slice(&u32::MAX.to_le_bytes())), Error::Corrupt(_)));
    assert!(matches!(corrupt(&|d| d[512] = 0), Error::Corrupt(_)), "FSInfo lead signature");
    assert!(matches!(corrupt(&|d| d[512 + 484] = 0), Error::Corrupt(_)), "FSInfo struct signature");
    assert!(matches!(corrupt(&|d| d[512 + 510] = 0), Error::Corrupt(_)), "FSInfo trail signature");
    // Larger than the device.
    assert!(matches!(
        corrupt(&|d| d[32..36].copy_from_slice(&((SMALL / 512 + 1) as u32).to_le_bytes())),
        Error::Corrupt(_)
    ));
    // Too few clusters for FAT32.
    assert!(matches!(
        corrupt(&|d| d[32..36].copy_from_slice(&40_000u32.to_le_bytes())),
        Error::Unsupported(_) | Error::Corrupt(_)
    ));
}

#[test]
fn clean_shutdown_bit_and_free_count_recovery() {
    let mut fs = fresh(SMALL);
    let layout = Layout::read(&fs.device().data);
    // Mounted for writing: dirty on disk.
    assert_eq!(layout.fat_entry(&fs.device().data, 0, 1) & 0x0800_0000, 0);
    assert_eq!(layout.fat_entry(&fs.device().data, 1, 1) & 0x0800_0000, 0);
    assert!(fs.was_cleanly_unmounted());
    fs.sync().unwrap();
    assert_eq!(layout.fat_entry(&fs.device().data, 0, 1) & 0x0800_0000, 0x0800_0000);
    fs.write_file("/a.txt", b"hello").unwrap();
    assert_eq!(layout.fat_entry(&fs.device().data, 0, 1) & 0x0800_0000, 0, "dirty again after a write");
    let free = fs.stats().unwrap().free_clusters;
    // Reserved top bits of FAT entries are preserved.
    let disk = fs.unmount().unwrap();
    assert_eq!(layout.fat_entry(&disk.data, 0, 1), 0x0FFF_FFFF);
    assert!(disk.flushes >= 1);

    let mut fs = FileSystem::mount(disk).unwrap();
    assert!(fs.check().unwrap().was_cleanly_unmounted);
    fs.write_file("/b.txt", &vec![7u8; 5000]).unwrap();
    // Simulate a crash: take the disk without unmounting and corrupt the FSInfo count.
    let mut disk = fs.device().clone();
    disk.data[512 + 488..512 + 492].copy_from_slice(&12345u32.to_le_bytes());
    let mut fs = FileSystem::mount(disk).unwrap();
    assert!(!fs.was_cleanly_unmounted());
    let report = fs.check().unwrap();
    assert!(!report.was_cleanly_unmounted);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(fs.stats().unwrap().free_clusters, free - 10, "free count recomputed");
    assert_eq!(fs.read_file("/b.txt").unwrap(), vec![7u8; 5000]);

    // Unknown (0xFFFFFFFF) free count on a clean volume is recomputed too.
    let mut disk = fs.unmount().unwrap();
    disk.data[512 + 488..512 + 492].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut fs = FileSystem::mount(disk).unwrap();
    assert_eq!(fs.stats().unwrap().free_clusters, free - 10);
    assert_clean(&mut fs);
}

#[test]
fn read_only_device() {
    let mut fs = fresh(SMALL);
    fs.create_dir("/docs").unwrap();
    fs.write_file("/docs/a.txt", b"read me").unwrap();
    let mut disk = fs.unmount().unwrap();
    disk.read_only = true;
    let mut fs = FileSystem::mount(disk).unwrap();
    assert!(fs.is_read_only());
    assert_eq!(fs.read_file("/docs/a.txt").unwrap(), b"read me");
    assert_eq!(fs.write_file("/x", b"1"), Err(Error::Device(DeviceError::ReadOnly)));
    assert_eq!(fs.create_dir("/x"), Err(Error::Device(DeviceError::ReadOnly)));
    assert_eq!(fs.remove_file("/docs/a.txt"), Err(Error::Device(DeviceError::ReadOnly)));
    fs.sync().unwrap();
    assert!(fs.check().unwrap().problems.is_empty());
}

// ----- files --------------------------------------------------------------------------------------

#[test]
fn write_and_read_files_of_many_sizes() {
    let mut fs = fresh(SMALL);
    let sizes = [0usize, 1, 511, 512, 513, 1023, 1024, 4096, 10_000, 65_536, 100_001, 1 << 20];
    for (i, &size) in sizes.iter().enumerate() {
        let path = format!("/file{}.bin", i);
        let data = pattern(size, i as u64);
        fs.write_file(&path, &data).unwrap();
        assert_eq!(fs.read_file(&path).unwrap(), data, "size {}", size);
        let meta = fs.metadata(&path).unwrap();
        assert_eq!(meta.size, size as u64);
        assert!(!meta.is_dir);
    }
    assert_clean(&mut fs);
    // Contents survive a remount.
    let disk = fs.unmount().unwrap();
    let mut fs = FileSystem::mount(disk).unwrap();
    for (i, &size) in sizes.iter().enumerate() {
        assert_eq!(fs.read_file(&format!("/file{}.bin", i)).unwrap(), pattern(size, i as u64));
    }
}

#[test]
fn read_at_edges() {
    let mut fs = fresh(SMALL);
    let data = pattern(3000, 1);
    fs.write_file("/f", &data).unwrap();
    let mut buffer = vec![0u8; 700];
    for offset in [0u64, 1, 511, 512, 513, 1000, 2300, 2999] {
        let n = fs.read_at("/f", offset, &mut buffer).unwrap();
        let expected = (3000 - offset as usize).min(700);
        assert_eq!(n, expected);
        assert_eq!(&buffer[..n], &data[offset as usize..offset as usize + n]);
    }
    assert_eq!(fs.read_at("/f", 3000, &mut buffer).unwrap(), 0);
    assert_eq!(fs.read_at("/f", 1 << 40, &mut buffer).unwrap(), 0);
    assert_eq!(fs.read_at("/f", 0, &mut []).unwrap(), 0);
    // Sequential chunked reads reassemble the file.
    let mut out = Vec::new();
    let mut offset = 0;
    loop {
        let n = fs.read_at("/f", offset, &mut buffer[..333]).unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buffer[..n]);
        offset += n as u64;
    }
    assert_eq!(out, data);
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.read_at("/d", 0, &mut buffer), Err(Error::IsADirectory));
    assert_eq!(fs.read_at("/missing", 0, &mut buffer), Err(Error::NotFound));
    assert_eq!(fs.read_file("/d"), Err(Error::IsADirectory));
    assert_eq!(fs.read_file("/"), Err(Error::IsADirectory));
}

#[test]
fn replacing_files_reuses_and_frees_clusters() {
    let mut fs = fresh(SMALL);
    let free0 = fs.stats().unwrap().free_clusters;
    fs.write_file("/f", &pattern(10 * 512, 1)).unwrap();
    assert_eq!(fs.stats().unwrap().free_clusters, free0 - 10);
    fs.write_file("/f", &pattern(3 * 512 + 1, 2)).unwrap();
    assert_eq!(fs.stats().unwrap().free_clusters, free0 - 4);
    assert_eq!(fs.read_file("/f").unwrap(), pattern(3 * 512 + 1, 2));
    fs.write_file("/f", &pattern(20 * 512, 3)).unwrap();
    assert_eq!(fs.stats().unwrap().free_clusters, free0 - 20);
    assert_eq!(fs.read_file("/f").unwrap(), pattern(20 * 512, 3));
    fs.write_file("/f", b"").unwrap();
    assert_eq!(fs.stats().unwrap().free_clusters, free0);
    assert_eq!(fs.read_file("/f").unwrap(), b"");
    assert_clean(&mut fs);
    fs.create_dir("/dir").unwrap();
    assert_eq!(fs.write_file("/dir", b"x"), Err(Error::IsADirectory));
    assert_eq!(fs.write_file("/", b"x"), Err(Error::IsADirectory));
    assert_eq!(fs.write_file("/nope/f", b"x"), Err(Error::NotFound));
    assert_eq!(fs.write_file("/f/g", b"x"), Err(Error::NotADirectory));
}

#[test]
fn append_file() {
    let mut fs = fresh(SMALL);
    let mut expected = Vec::new();
    for i in 0..50 {
        let chunk = pattern(97 * i + 3, i as u64);
        fs.append_file("/log.txt", &chunk).unwrap();
        expected.extend_from_slice(&chunk);
    }
    assert_eq!(fs.read_file("/log.txt").unwrap(), expected);
    fs.append_file("/log.txt", b"").unwrap();
    assert_eq!(fs.metadata("/log.txt").unwrap().size, expected.len() as u64);
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.append_file("/d", b"x"), Err(Error::IsADirectory));
    assert_clean(&mut fs);
}

#[test]
fn write_at_sparse_extension_is_zero_filled() {
    let mut fs = fresh(SMALL);
    fs.write_file("/f", &[0xAA; 2000]).unwrap();
    // Shrink: stale bytes stay in the last cluster on disk and must not reappear.
    fs.truncate("/f", 100).unwrap();
    fs.write_at("/f", 5000, b"tail").unwrap();
    let data = fs.read_file("/f").unwrap();
    assert_eq!(data.len(), 5004);
    assert!(data[..100].iter().all(|&b| b == 0xAA));
    assert!(data[100..5000].iter().all(|&b| b == 0), "gap must be zero-filled");
    assert_eq!(&data[5000..], b"tail");
    // Overwrite in the middle across a cluster boundary.
    fs.write_at("/f", 500, &[1u8; 30]).unwrap();
    let data = fs.read_file("/f").unwrap();
    assert!(data[500..530].iter().all(|&b| b == 1));
    assert_eq!(data.len(), 5004);
    // Write into an empty file at an offset.
    fs.touch("/empty").unwrap();
    fs.write_at("/empty", 1500, b"x").unwrap();
    let data = fs.read_file("/empty").unwrap();
    assert_eq!(data.len(), 1501);
    assert!(data[..1500].iter().all(|&b| b == 0));
    // Empty writes change nothing.
    fs.write_at("/empty", 100_000, b"").unwrap();
    assert_eq!(fs.metadata("/empty").unwrap().size, 1501);
    assert_eq!(fs.write_at("/missing", 0, b"x"), Err(Error::NotFound));
    assert_eq!(fs.write_at("/", 0, b"x"), Err(Error::IsADirectory));
    assert_eq!(fs.write_at("/f", u32::MAX as u64, b"x"), Err(Error::FileTooLarge));
    assert_eq!(fs.write_at("/f", u64::MAX, b"x"), Err(Error::FileTooLarge));
    assert_clean(&mut fs);
}

#[test]
fn truncate_shrinks_and_extends() {
    let mut fs = fresh(SMALL);
    let free0 = fs.stats().unwrap().free_clusters;
    let data = pattern(10_000, 5);
    fs.write_file("/f", &data).unwrap();
    fs.truncate("/f", 1000).unwrap();
    assert_eq!(fs.read_file("/f").unwrap(), &data[..1000]);
    assert_eq!(fs.stats().unwrap().free_clusters, free0 - 2);
    fs.truncate("/f", 3000).unwrap();
    let grown = fs.read_file("/f").unwrap();
    assert_eq!(&grown[..1000], &data[..1000]);
    assert!(grown[1000..].iter().all(|&b| b == 0));
    assert_eq!(fs.stats().unwrap().free_clusters, free0 - 6);
    fs.truncate("/f", 3000).unwrap();
    fs.truncate("/f", 0).unwrap();
    assert_eq!(fs.read_file("/f").unwrap(), b"");
    assert_eq!(fs.stats().unwrap().free_clusters, free0);
    assert_eq!(fs.truncate("/f", u32::MAX as u64 + 1), Err(Error::FileTooLarge));
    assert_eq!(fs.truncate("/", 0), Err(Error::IsADirectory));
    assert_eq!(fs.truncate("/nope", 0), Err(Error::NotFound));
    assert_clean(&mut fs);
    // First cluster is cleared when a file becomes empty.
    let disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let (_, entry) = layout.find(&disk.data, layout.root, b"F          ").unwrap();
    assert_eq!(entry_cluster(&entry), 0);
    assert_eq!(u32_at(&entry, 28), 0);
}

#[test]
fn touch_and_timestamps() {
    let mut fs = fresh(SMALL);
    fs.touch("/new").unwrap();
    let meta = fs.metadata("/new").unwrap();
    assert_eq!(meta.size, 0);
    assert_eq!(meta.created, ts(2026, 1, 1, 0, 0, 0), "default clock");
    assert_eq!(meta.modified, ts(2026, 1, 1, 0, 0, 0));

    fs.set_time_source(|| ts(2030, 7, 15, 13, 45, 31));
    fs.write_file("/timed", b"x").unwrap();
    let meta = fs.metadata("/timed").unwrap();
    assert_eq!(meta.created, ts(2030, 7, 15, 13, 45, 31), "creation time keeps odd seconds");
    assert_eq!(meta.modified, ts(2030, 7, 15, 13, 45, 30), "2-second resolution");

    fs.set_time_source(|| ts(2031, 2, 3, 4, 5, 6));
    fs.append_file("/timed", b"y").unwrap();
    let meta = fs.metadata("/timed").unwrap();
    assert_eq!(meta.created, ts(2030, 7, 15, 13, 45, 31));
    assert_eq!(meta.modified, ts(2031, 2, 3, 4, 5, 6));

    fs.set_time_source(|| ts(2032, 1, 1, 0, 0, 0));
    fs.touch("/timed").unwrap();
    assert_eq!(fs.metadata("/timed").unwrap().modified, ts(2032, 1, 1, 0, 0, 0));
    assert_eq!(fs.read_file("/timed").unwrap(), b"xy", "touch keeps contents");
    fs.create_dir("/dir").unwrap();
    assert_eq!(fs.metadata("/dir").unwrap().created, ts(2032, 1, 1, 0, 0, 0));
    fs.set_time_source(|| ts(2033, 1, 1, 0, 0, 0));
    fs.touch("/dir").unwrap();
    assert_eq!(fs.metadata("/dir").unwrap().modified, ts(2033, 1, 1, 0, 0, 0));
    fs.touch("/").unwrap();
    // Out-of-range clocks are clamped.
    fs.set_time_source(|| ts(1970, 1, 1, 0, 0, 0));
    fs.touch("/old").unwrap();
    assert_eq!(fs.metadata("/old").unwrap().created, ts(1980, 1, 1, 0, 0, 0));
    assert_eq!(fs.touch("/missing/x"), Err(Error::NotFound));
    assert_eq!(fs.touch("/bad*name"), Err(Error::InvalidName));
    assert_clean(&mut fs);
}

#[test]
fn remove_file() {
    let mut fs = fresh(SMALL);
    let free0 = fs.stats().unwrap().free_clusters;
    fs.write_file("/a long file name.txt", &pattern(5000, 1)).unwrap();
    fs.remove_file("/A LONG FILE NAME.TXT").unwrap();
    assert!(!fs.exists("/a long file name.txt").unwrap());
    assert_eq!(fs.stats().unwrap().free_clusters, free0);
    assert_eq!(fs.remove_file("/a long file name.txt"), Err(Error::NotFound));
    // All slots (long-name entries included) are marked deleted.
    {
        let data = &fs.device().data;
        let layout = Layout::read(data);
        let entries = layout.entries(data, layout.root);
        let deleted = entries.iter().filter(|e| e[0] == 0xE5).count();
        assert_eq!(deleted, 3, "2 long-name entries + 1 short entry");
    }
    fs.create_dir("/d").unwrap();
    assert_eq!(fs.remove_file("/d"), Err(Error::IsADirectory));
    assert_eq!(fs.remove_file("/"), Err(Error::IsADirectory));
    assert_clean(&mut fs);
}

#[test]
fn copy_file() {
    let mut fs = fresh(SMALL);
    let data = pattern(200_000, 9);
    fs.write_file("/src.bin", &data).unwrap();
    fs.create_dir("/backup").unwrap();
    let free = fs.stats().unwrap().free_clusters;
    fs.copy_file("/src.bin", "/backup/copy of src.bin").unwrap();
    assert_eq!(fs.read_file("/backup/copy of src.bin").unwrap(), data);
    assert_eq!(fs.stats().unwrap().free_clusters, free - 391);
    fs.touch("/empty").unwrap();
    fs.copy_file("/empty", "/empty2").unwrap();
    assert_eq!(fs.read_file("/empty2").unwrap(), b"");
    assert_eq!(fs.copy_file("/src.bin", "/empty2"), Err(Error::AlreadyExists));
    assert_eq!(fs.copy_file("/backup", "/b2"), Err(Error::IsADirectory));
    assert_eq!(fs.copy_file("/nope", "/b2"), Err(Error::NotFound));
    assert_eq!(fs.copy_file("/src.bin", "/nope/b2"), Err(Error::NotFound));
    assert_eq!(fs.copy_file("/src.bin", "/"), Err(Error::AlreadyExists));
    assert_eq!(fs.copy_file("/src.bin", "/bad|name"), Err(Error::InvalidName));
    // Modifying the copy leaves the original alone.
    fs.write_at("/backup/copy of src.bin", 0, b"changed").unwrap();
    assert_eq!(fs.read_file("/src.bin").unwrap(), data);
    assert_clean(&mut fs);
}

// ----- directories --------------------------------------------------------------------------------

#[test]
fn directories() {
    let mut fs = fresh(SMALL);
    fs.create_dir("/a").unwrap();
    fs.create_dir("/a/b").unwrap();
    fs.create_dir("/a/b/c").unwrap();
    assert_eq!(fs.create_dir("/a"), Err(Error::AlreadyExists));
    assert_eq!(fs.create_dir("/A"), Err(Error::AlreadyExists));
    assert_eq!(fs.create_dir("/"), Err(Error::AlreadyExists));
    assert_eq!(fs.create_dir("/x/y"), Err(Error::NotFound));
    fs.write_file("/a/file", b"1").unwrap();
    assert_eq!(fs.create_dir("/a/file/sub"), Err(Error::NotADirectory));
    assert_eq!(fs.create_dir("/a/file"), Err(Error::AlreadyExists));
    assert_eq!(fs.create_dir("/a/bad?"), Err(Error::InvalidName));
    assert_eq!(fs.create_dir("relative"), Err(Error::InvalidPath));

    let names: Vec<String> = fs.read_dir("/a").unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["b", "file"]);
    assert!(fs.read_dir("/a/b/c").unwrap().is_empty());
    assert_eq!(fs.read_dir("/a/file"), Err(Error::NotADirectory));
    assert_eq!(fs.read_dir("/zzz"), Err(Error::NotFound));
    let meta = fs.metadata("/a/b").unwrap();
    assert!(meta.is_dir);
    assert_eq!(meta.size, 0);

    assert_eq!(fs.remove_dir("/a"), Err(Error::DirectoryNotEmpty));
    assert_eq!(fs.remove_dir("/a/file"), Err(Error::NotADirectory));
    assert_eq!(fs.remove_dir("/"), Err(Error::RootDirectory));
    assert_eq!(fs.remove_dir("/nope"), Err(Error::NotFound));
    fs.remove_dir("/a/b/c").unwrap();
    assert!(!fs.exists("/a/b/c").unwrap());
    assert_clean(&mut fs);

    // '.' and '..' entries: '..' of a directory in the root points to cluster 0.
    let disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let (_, a) = layout.find(&disk.data, layout.root, b"A          ").unwrap();
    let a_cluster = entry_cluster(&a);
    let entries = layout.entries(&disk.data, a_cluster);
    assert_eq!(&entries[0][..11], b".          ");
    assert_eq!(entry_cluster(&entries[0]), a_cluster);
    assert_eq!(entries[0][11] & 0x10, 0x10);
    assert_eq!(&entries[1][..11], b"..         ");
    assert_eq!(entry_cluster(&entries[1]), 0);
    let (_, b) = layout.find(&disk.data, a_cluster, b"B          ").unwrap();
    let b_entries = layout.entries(&disk.data, entry_cluster(&b));
    assert_eq!(entry_cluster(&b_entries[1]), a_cluster, "'..' of /a/b points to /a");
}

#[test]
fn create_dir_all_and_remove_dir_all() {
    let mut fs = fresh(SMALL);
    let free0 = fs.stats().unwrap().free_clusters;
    fs.create_dir_all("/x/y/z/w").unwrap();
    fs.create_dir_all("/x/y/z/w").unwrap();
    fs.create_dir_all("/x/Y/other").unwrap();
    fs.create_dir_all("/").unwrap();
    assert!(fs.metadata("/x/y/other").unwrap().is_dir);
    fs.write_file("/x/file", b"f").unwrap();
    assert_eq!(fs.create_dir_all("/x/file/deeper"), Err(Error::NotADirectory));
    assert_eq!(fs.create_dir_all("/x/file"), Err(Error::AlreadyExists));
    for i in 0..40 {
        fs.write_file(&format!("/x/y/z/w/file number {}", i), &pattern(i * 300, i as u64)).unwrap();
    }
    for i in 0..5 {
        fs.create_dir_all(&format!("/x/y/deep{}/a/b/c", i)).unwrap();
        fs.write_file(&format!("/x/y/deep{}/a/b/c/data", i), &pattern(3000, 1)).unwrap();
    }
    assert_clean(&mut fs);
    assert_eq!(fs.remove_dir_all("/x/file"), Err(Error::NotADirectory));
    assert_eq!(fs.remove_dir_all("/"), Err(Error::RootDirectory));
    assert_eq!(fs.remove_dir_all("/nope"), Err(Error::NotFound));
    fs.remove_dir_all("/x/y").unwrap();
    assert!(!fs.exists("/x/y").unwrap());
    assert!(fs.exists("/x/file").unwrap());
    fs.remove_dir_all("/X").unwrap();
    assert!(fs.read_dir("/").unwrap().is_empty());
    assert_eq!(fs.stats().unwrap().free_clusters, free0);
    assert_clean(&mut fs);
}

#[test]
fn directory_growth_past_many_clusters_and_slot_reuse() {
    let mut fs = fresh(SMALL);
    fs.create_dir("/big").unwrap();
    let count: usize = 600;
    for i in 0..count {
        fs.write_file(&format!("/big/entry with a long name {:04}.txt", i), &i.to_le_bytes()).unwrap();
    }
    let listing = fs.read_dir("/big").unwrap();
    assert_eq!(listing.len(), count);
    for (i, meta) in listing.iter().enumerate() {
        assert_eq!(meta.name, format!("entry with a long name {:04}.txt", i), "on-disk order");
    }
    for i in (0..count).step_by(7) {
        assert_eq!(fs.read_file(&format!("/BIG/ENTRY WITH A LONG NAME {:04}.TXT", i)).unwrap(), i.to_le_bytes());
    }
    assert_clean(&mut fs);
    // Delete every other entry, then create new ones: freed slots are reused, the directory
    // does not grow.
    let used_before = fs.stats().unwrap().free_clusters;
    for i in (0..count).step_by(2) {
        fs.remove_file(&format!("/big/entry with a long name {:04}.txt", i)).unwrap();
    }
    let after_delete = fs.stats().unwrap().free_clusters;
    assert_eq!(after_delete, used_before + (count as u32 / 2), "one data cluster per file freed");
    for i in 0..count / 2 {
        fs.write_file(&format!("/big/new name {:04}.bin", i), b"n").unwrap();
    }
    assert_eq!(fs.stats().unwrap().free_clusters, used_before, "directory did not grow");
    assert_eq!(fs.read_dir("/big").unwrap().len(), count);
    assert_clean(&mut fs);
    fs.remove_dir_all("/big").unwrap();
    assert_clean(&mut fs);
}

#[test]
fn root_directory_grows_too() {
    let mut fs = fresh(SMALL);
    for i in 0..300 {
        fs.touch(&format!("/root file {}", i)).unwrap();
    }
    assert_eq!(fs.read_dir("/").unwrap().len(), 300);
    assert_clean(&mut fs);
}

// ----- names and paths ----------------------------------------------------------------------------

#[test]
fn path_edge_cases() {
    let mut fs = fresh(SMALL);
    fs.create_dir_all("/Docs/Sub").unwrap();
    fs.write_file("/Docs/Sub/Note.TXT", b"note").unwrap();
    for path in [
        "/Docs/Sub/Note.TXT",
        "/docs/sub/note.txt",
        "/DOCS/SUB/NOTE.TXT",
        "//docs///sub//note.txt",
        "/docs/./sub/./note.txt",
        "/docs/sub/../sub/note.txt",
        "/../docs/sub/note.txt",
        "/docs/sub/note.txt/",
    ] {
        assert_eq!(fs.read_file(path).unwrap(), b"note", "{}", path);
    }
    assert!(fs.metadata("/docs/sub/..").unwrap().is_dir);
    assert_eq!(fs.metadata("/docs/sub/..").unwrap().name, "Docs");
    assert!(fs.metadata("/docs/..").unwrap().is_dir);
    assert_eq!(fs.metadata("/docs/..").unwrap().name, "");
    assert_eq!(fs.read_file("docs/sub/note.txt"), Err(Error::InvalidPath));
    assert_eq!(fs.read_file(""), Err(Error::InvalidPath));
    assert_eq!(fs.exists("relative"), Err(Error::InvalidPath));
    assert!(fs.exists("/").unwrap());
    assert!(fs.exists("/DOCS").unwrap());
    assert!(!fs.exists("/docs/sub/note.txt/more").unwrap());
    assert!(!fs.exists("/nothing/here").unwrap());
    assert_eq!(fs.read_file("/docs/sub/note.txt/more"), Err(Error::NotADirectory));
    // Names keep the case they were created with.
    let listing = fs.read_dir("/docs/sub").unwrap();
    assert_eq!(listing[0].name, "Note.TXT");
}

#[test]
fn names_and_short_name_aliases() {
    let mut fs = fresh(SMALL);
    let long_name: String = std::iter::repeat_n('n', 255).collect();
    let names = [
        "README.TXT",
        "readme2.txt",
        "Mixed Case Name.Data",
        "ünïcödé ✓ name.txt",
        "emoji 😀.bin",
        ".hidden",
        "a+b=c;d,e[f].txt",
        "many.dots.in.name.tar.gz",
        " leading space",
        long_name.as_str(),
    ];
    for (i, name) in names.iter().enumerate() {
        fs.write_file(&format!("/{}", name), &[i as u8]).unwrap();
    }
    let listing: Vec<String> = fs.read_dir("/").unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(listing, names);
    for (i, name) in names.iter().enumerate() {
        assert_eq!(fs.read_file(&format!("/{}", name.to_uppercase())).unwrap(), [i as u8], "{}", name);
    }
    // Short-name alias of a long name.
    fs.write_file("/long file name.txt", b"alias").unwrap();
    assert_eq!(fs.read_file("/LONGFI~1.TXT").unwrap(), b"alias");
    assert_eq!(fs.read_file("/longfi~1.txt").unwrap(), b"alias");
    // A name equal to an existing alias is taken.
    assert_eq!(fs.create_dir("/LONGFI~1.TXT"), Err(Error::AlreadyExists));
    assert_eq!(fs.write_file(&format!("/{}", "x".repeat(256)), b""), Err(Error::InvalidName));
    for bad in ["/a*", "/a?", "/a:b", "/a<b", "/a>b", "/a|b", "/a\"b", "/a\\b", "/trailing.", "/trailing ", "/tab\t"] {
        assert_eq!(fs.touch(bad), Err(Error::InvalidName), "{}", bad);
    }
    assert_clean(&mut fs);

    // On disk: README.TXT is a bare short entry; readme2.txt has a long name.
    let disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let entries = layout.entries(&disk.data, layout.root);
    let (index, _) = layout.find(&disk.data, layout.root, b"README  TXT").unwrap();
    assert_ne!(entries[index - 1][11], 0x0F, "no long-name entry before README.TXT");
    let (index, _) = layout.find(&disk.data, layout.root, b"README2 TXT").unwrap();
    assert_eq!(entries[index - 1][11], 0x0F, "long-name entry before readme2.txt");
    assert_eq!(entries[index - 1][0], 0x41);
}

#[test]
fn twenty_similar_long_names_get_unique_short_names() {
    let mut fs = fresh(SMALL);
    for i in 1..=20 {
        fs.write_file(&format!("/long file name {}.txt", i), format!("{}", i).as_bytes()).unwrap();
    }
    for i in 1..=20 {
        assert_eq!(fs.read_file(&format!("/long file name {}.txt", i)).unwrap(), format!("{}", i).as_bytes());
    }
    let disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let mut shorts: Vec<[u8; 11]> = Vec::new();
    // Walk the root directory chain (it spans several clusters).
    let mut cluster = layout.root;
    loop {
        for e in layout.entries(&disk.data, cluster) {
            if e[0] != 0 && e[0] != 0xE5 && e[11] != 0x0F && e[11] & 0x08 == 0 {
                shorts.push(e[..11].try_into().unwrap());
            }
        }
        let next = layout.fat_entry(&disk.data, 0, cluster) & 0x0FFF_FFFF;
        if next >= 0x0FFF_FFF8 {
            break;
        }
        cluster = next;
    }
    assert_eq!(shorts.len(), 20);
    let mut unique = shorts.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 20);
    assert!(shorts.contains(b"LONGFI~1TXT"));
    assert!(shorts.contains(b"LONGF~20TXT"));
    // fatfs sees the same names.
    let mut image = disk.data;
    let tree = dump_fatfs(&mut image);
    assert_eq!(tree.len(), 20);
    assert_eq!(tree["/long file name 7.txt"], Some(b"7".to_vec()));
}

#[test]
fn nt_case_flags_and_0x05_escape_on_read() {
    let fs = fresh(SMALL);
    let mut disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let root = layout.cluster_offset(layout.root);
    let mut entry = [0u8; 32];
    entry[..11].copy_from_slice(b"LOWER   TXT");
    entry[11] = 0x20;
    entry[12] = 0x18; // lower-case base and extension
    disk.data[root + 32..root + 64].copy_from_slice(&entry);
    entry[..11].copy_from_slice(b"\x05SC     TXT");
    entry[12] = 0;
    disk.data[root + 64..root + 96].copy_from_slice(&entry);
    let mut fs = FileSystem::mount(disk).unwrap();
    let names: Vec<String> = fs.read_dir("/").unwrap().into_iter().map(|m| m.name).collect();
    assert_eq!(names, ["lower.txt", "σSC.TXT"]);
    assert_eq!(fs.read_file("/LOWER.TXT").unwrap(), b"");
    assert_clean(&mut fs);
}

// ----- rename -------------------------------------------------------------------------------------

#[test]
fn rename_files_and_directories() {
    let mut fs = fresh(SMALL);
    fs.set_time_source(|| ts(2027, 3, 4, 5, 6, 8));
    fs.create_dir_all("/src/inner").unwrap();
    fs.create_dir("/dst").unwrap();
    fs.write_file("/src/inner/file.txt", b"payload").unwrap();
    fs.write_file("/top.txt", b"top").unwrap();
    let before = fs.metadata("/top.txt").unwrap();
    fs.set_time_source(|| ts(2028, 1, 1, 0, 0, 0));

    // Same directory.
    fs.rename("/top.txt", "/renamed top.txt").unwrap();
    assert!(!fs.exists("/top.txt").unwrap());
    assert_eq!(fs.read_file("/renamed top.txt").unwrap(), b"top");
    let after = fs.metadata("/renamed top.txt").unwrap();
    assert_eq!(after.created, before.created);
    assert_eq!(after.modified, before.modified);
    // Across directories.
    fs.rename("/renamed top.txt", "/dst/moved.txt").unwrap();
    assert_eq!(fs.read_file("/dst/moved.txt").unwrap(), b"top");
    // Directory move: '..' must follow.
    fs.rename("/src/inner", "/dst/inner2").unwrap();
    assert_eq!(fs.read_file("/dst/inner2/file.txt").unwrap(), b"payload");
    assert!(fs.read_dir("/src").unwrap().is_empty());
    assert_clean(&mut fs);
    fs.rename("/dst/inner2", "/inner at root").unwrap();
    assert_clean(&mut fs);
    fs.rename("/inner at root", "/src/back").unwrap();
    assert_clean(&mut fs);

    // Errors.
    assert_eq!(fs.rename("/src", "/src/back/x"), Err(Error::InvalidMove));
    assert_eq!(fs.rename("/src", "/src/y"), Err(Error::InvalidMove));
    assert_eq!(fs.rename("/src", "/SRC/back/deeper/z"), Err(Error::NotFound));
    assert_eq!(fs.rename("/src/back", "/dst"), Err(Error::AlreadyExists));
    assert_eq!(fs.rename("/dst/moved.txt", "/src"), Err(Error::AlreadyExists));
    assert_eq!(fs.rename("/nope", "/x"), Err(Error::NotFound));
    assert_eq!(fs.rename("/", "/x"), Err(Error::RootDirectory));
    assert_eq!(fs.rename("/dst", "/"), Err(Error::RootDirectory));
    assert_eq!(fs.rename("/dst/moved.txt", "/missing/x"), Err(Error::NotFound));
    assert_eq!(fs.rename("/dst/moved.txt", "/dst/moved.txt/x"), Err(Error::NotADirectory));
    assert_eq!(fs.rename("/dst/moved.txt", "/bad:name"), Err(Error::InvalidName));

    // Case-only renames of the same entry.
    fs.rename("/dst/moved.txt", "/dst/MOVED.TXT").unwrap();
    assert_eq!(fs.read_dir("/dst").unwrap()[0].name, "MOVED.TXT");
    fs.rename("/dst/MOVED.TXT", "/dst/Moved.txt").unwrap();
    assert_eq!(fs.read_dir("/dst").unwrap()[0].name, "Moved.txt");
    fs.rename("/dst/moved.txt", "/dst/Moved.txt").unwrap();
    fs.rename("/src", "/SRC").unwrap();
    assert_eq!(fs.read_dir("/").unwrap().iter().filter(|m| m.name == "SRC").count(), 1);
    assert_eq!(fs.read_file("/src/back/file.txt").unwrap(), b"payload");
    assert_clean(&mut fs);

    // fatfs agrees with the result.
    let disk = fs.unmount().unwrap();
    let mut image = disk.data;
    let tree = dump_fatfs(&mut image);
    assert_eq!(tree.get("/dst/Moved.txt"), Some(&Some(b"top".to_vec())));
    assert_eq!(tree.get("/SRC/back/file.txt"), Some(&Some(b"payload".to_vec())));
}

#[test]
fn rename_directory_updates_dotdot() {
    let mut fs = fresh(SMALL);
    fs.create_dir_all("/a/b").unwrap();
    fs.create_dir("/c").unwrap();
    fs.rename("/a/b", "/c/b").unwrap();
    fs.rename("/c", "/a/c").unwrap();
    assert_clean(&mut fs);
    let disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let (_, a) = layout.find(&disk.data, layout.root, b"A          ").unwrap();
    let (_, c) = layout.find(&disk.data, entry_cluster(&a), b"C          ").unwrap();
    let (_, b) = layout.find(&disk.data, entry_cluster(&c), b"B          ").unwrap();
    assert_eq!(entry_cluster(&layout.entries(&disk.data, entry_cluster(&c))[1]), entry_cluster(&a));
    assert_eq!(entry_cluster(&layout.entries(&disk.data, entry_cluster(&b))[1]), entry_cluster(&c));
}

// ----- space exhaustion ---------------------------------------------------------------------------

#[test]
fn no_space_rolls_back_and_space_is_restored() {
    let mut fs = fresh(34 * MIB);
    let free0 = fs.stats().unwrap().free_clusters;
    let chunk = pattern(1 << 20, 3);
    let mut written = 0;
    loop {
        match fs.write_file(&format!("/fill{}", written), &chunk) {
            Ok(()) => written += 1,
            Err(Error::NoSpace) => break,
            Err(other) => panic!("unexpected {:?}", other),
        }
    }
    assert!(written >= 30);
    // The failed file does not exist and nothing leaked.
    assert!(!fs.exists(&format!("/fill{}", written)).unwrap());
    assert_clean(&mut fs);
    let free = fs.stats().unwrap().free_clusters;
    assert!(free < 2048);

    // Growing an existing file beyond the free space fails without changing it.
    fs.write_file("/small", b"abc").unwrap();
    assert_eq!(fs.append_file("/small", &vec![1u8; (free as usize + 10) * 512]), Err(Error::NoSpace));
    assert_eq!(fs.read_file("/small").unwrap(), b"abc");
    assert_eq!(fs.write_at("/small", 10 << 20, b"x"), Err(Error::NoSpace));
    assert_eq!(fs.truncate("/small", 100 << 20), Err(Error::NoSpace));
    assert_eq!(fs.metadata("/small").unwrap().size, 3);
    assert_eq!(fs.copy_file("/fill0", "/copy"), Err(Error::NoSpace));
    assert!(!fs.exists("/copy").unwrap());
    assert_clean(&mut fs);

    // Use up every remaining cluster, then directory creation and growth fail cleanly.
    let rest = fs.stats().unwrap().free_clusters as usize;
    fs.write_file("/rest", &vec![2u8; (rest - 2) * 512]).unwrap();
    let mut extra = 0;
    while fs.write_file(&format!("/x{}", extra), b"x").is_ok() {
        extra += 1;
    }
    let mut created = 0;
    loop {
        match fs.touch(&format!("/empty file with long name {}", created)) {
            Ok(()) => created += 1,
            Err(Error::NoSpace) => break,
            Err(other) => panic!("unexpected {:?}", other),
        }
    }
    assert_eq!(fs.stats().unwrap().free_clusters, 0);
    assert_eq!(fs.create_dir("/newdir"), Err(Error::NoSpace));
    assert_eq!(fs.write_file("/one more", b"x"), Err(Error::NoSpace));
    assert_clean(&mut fs);

    // Deleting everything restores all space.
    let names: Vec<String> = fs.read_dir("/").unwrap().into_iter().map(|m| m.name).collect();
    for name in names {
        fs.remove_file(&format!("/{}", name)).unwrap();
    }
    let grown = dir_clusters(&mut fs);
    assert_eq!(fs.stats().unwrap().free_clusters, free0 - grown);
    assert_clean(&mut fs);
    let disk = fs.unmount().unwrap();
    let mut fs = FileSystem::mount(disk).unwrap();
    assert_clean(&mut fs);
}

/// Clusters used by the root directory beyond its first one.
fn dir_clusters(fs: &mut FileSystem<RamDisk>) -> u32 {
    let data = &fs.device().data;
    let layout = Layout::read(data);
    let mut count = 0;
    let mut cluster = layout.root;
    loop {
        let next = layout.fat_entry(data, 0, cluster) & 0x0FFF_FFFF;
        if next >= 0x0FFF_FFF8 {
            return count;
        }
        count += 1;
        cluster = next;
    }
}

// ----- device errors and I/O efficiency ------------------------------------------------------------

#[test]
fn device_errors_are_reported() {
    let mut fs = fresh(SMALL);
    fs.write_file("/keep", b"keep").unwrap();
    let mut disk = fs.unmount().unwrap();
    disk.fail_writes_after = Some(3);
    let mut fs = FileSystem::mount(disk).unwrap();
    let result = fs.write_file("/big", &pattern(100_000, 1));
    assert!(matches!(result, Err(Error::Device(DeviceError::Io))), "{:?}", result);
    assert_eq!(fs.read_file("/keep").unwrap(), b"keep");
}

#[test]
fn large_io_uses_few_requests() {
    let mut fs = fresh(300 * MIB);
    let data = pattern(8 << 20, 11);
    fs.write_file("/big", &b""[..]).unwrap();
    let before = fs.device().writes;
    fs.write_file("/big", &data).unwrap();
    let writes = fs.device().writes - before;
    assert!(writes < 40, "writing 8 MiB took {} requests", writes);
    let before = fs.device().reads;
    assert_eq!(fs.read_file("/big").unwrap(), data);
    let reads = fs.device().reads - before;
    assert!(reads < 60, "reading 8 MiB took {} requests", reads);
    // Sequential read_at in 4 KiB steps does not rescan the chain from the start each time.
    let before = fs.device().reads;
    let mut buffer = vec![0u8; 4096];
    let mut offset = 0;
    while offset < data.len() as u64 {
        let n = fs.read_at("/big", offset, &mut buffer).unwrap();
        assert_eq!(&buffer[..n], &data[offset as usize..offset as usize + n]);
        offset += n as u64;
    }
    let reads = fs.device().reads - before;
    assert!(reads < 2 * 2048 + 200, "chunked read took {} requests", reads);
}

#[test]
fn error_messages() {
    assert_eq!(Error::NotFound.to_string(), "no such file or directory");
    assert_eq!(Error::NotADirectory.to_string(), "not a directory");
    assert_eq!(Error::IsADirectory.to_string(), "is a directory");
    assert_eq!(Error::DirectoryNotEmpty.to_string(), "directory not empty");
    assert_eq!(Error::NoSpace.to_string(), "no space left on device");
    assert_eq!(Error::AlreadyExists.to_string(), "file exists");
    assert_eq!(Error::InvalidName.to_string(), "invalid file name");
    assert_eq!(Error::InvalidPath.to_string(), "invalid path");
    assert_eq!(Error::FileTooLarge.to_string(), "file too large");
    assert_eq!(Error::Corrupt("bad fat").to_string(), "file system corrupt: bad fat");
    assert_eq!(Error::Unsupported("fat16").to_string(), "unsupported: fat16");
    assert_eq!(Error::Device(DeviceError::Io).to_string(), "i/o error");
    assert_eq!(Error::Device(DeviceError::Timeout).to_string(), "i/o error: device timed out");
    assert_eq!(Error::Device(DeviceError::ReadOnly).to_string(), "read-only file system");
    assert!(!Error::InvalidMove.to_string().is_empty());
    assert!(!Error::RootDirectory.to_string().is_empty());
}

// ----- check() finds damage -----------------------------------------------------------------------

#[test]
fn check_detects_damage() {
    let mut fs = fresh(SMALL);
    fs.create_dir("/dir").unwrap();
    fs.write_file("/dir/a", &pattern(3000, 1)).unwrap();
    fs.write_file("/b", &pattern(3000, 2)).unwrap();
    let clean = fs.unmount().unwrap();
    let layout = Layout::read(&clean.data);
    let problems = |disk: RamDisk| {
        let mut fs = FileSystem::mount(disk).unwrap();
        fs.check().unwrap()
    };

    // Lost cluster.
    let mut disk = clean.clone();
    layout.set_fat_entry(&mut disk.data, 60_000, 0x0FFF_FFFF);
    let report = problems(disk);
    assert_eq!(report.lost_clusters, 1);
    assert!(!report.problems.is_empty());

    // Cross-link: /b's first cluster points into /dir/a's chain.
    let mut disk = clean.clone();
    let (_, b) = layout.find(&disk.data, layout.root, b"B          ").unwrap();
    let (_, dir) = layout.find(&disk.data, layout.root, b"DIR        ").unwrap();
    let (_, a) = layout.find(&disk.data, entry_cluster(&dir), b"A          ").unwrap();
    layout.set_fat_entry(&mut disk.data, entry_cluster(&b), entry_cluster(&a));
    let report = problems(disk);
    assert!(report.problems.iter().any(|p| p.contains("cross-linked")), "{:?}", report.problems);

    // Wrong size for the chain.
    let mut disk = clean.clone();
    let (index, mut entry) = layout.find(&disk.data, layout.root, b"B          ").unwrap();
    entry[28..32].copy_from_slice(&100_000u32.to_le_bytes());
    let offset = layout.cluster_offset(layout.root) + index * 32;
    disk.data[offset..offset + 32].copy_from_slice(&entry);
    let report = problems(disk);
    assert!(report.problems.iter().any(|p| p.contains("/b")), "{:?}", report.problems);

    // Broken '..'.
    let mut disk = clean.clone();
    let offset = layout.cluster_offset(entry_cluster(&dir)) + 32 + 26;
    disk.data[offset..offset + 2].copy_from_slice(&77u16.to_le_bytes());
    let report = problems(disk);
    assert!(report.problems.iter().any(|p| p.contains("'..'")), "{:?}", report.problems);

    // Orphaned long-name entry.
    let mut disk = clean.clone();
    let root = layout.cluster_offset(layout.root);
    let free_slot = layout.entries(&disk.data, layout.root).iter().position(|e| e[0] == 0).unwrap();
    let mut lfn = [0xFFu8; 32];
    lfn[0] = 0x41;
    lfn[11] = 0x0F;
    lfn[12] = 0;
    lfn[13] = 0x12;
    lfn[26] = 0;
    lfn[27] = 0;
    disk.data[root + free_slot * 32..root + free_slot * 32 + 32].copy_from_slice(&lfn);
    let report = problems(disk);
    assert!(report.problems.iter().any(|p| p.contains("orphaned")), "{:?}", report.problems);

    // FAT copies differ.
    let mut disk = clean.clone();
    let offset = (layout.reserved + layout.fat_size) * SECTOR_SIZE + 50_000 * 4;
    disk.data[offset] = 5;
    let report = problems(disk);
    assert!(report.problems.iter().any(|p| p.contains("FAT copy")), "{:?}", report.problems);

    // Clean volume is clean.
    let report = problems(clean);
    assert!(report.problems.is_empty());
    assert_eq!(report.files, 2);
    assert_eq!(report.directories, 1);
    assert_eq!(report.used_clusters, 1 + 1 + 6 + 6);
    assert!(report.was_cleanly_unmounted);
}

#[test]
fn device_accessor_and_mount_through_mut_reference() {
    let mut disk = RamDisk::new(SMALL);
    fat32::format(&mut disk, &FormatOptions::default()).unwrap();
    {
        let mut fs = FileSystem::mount(&mut disk).unwrap();
        fs.write_file("/via ref", b"ok").unwrap();
        assert_eq!(fs.device().sector_count() as usize, SMALL / 512);
        fs.unmount().unwrap();
    }
    let mut fs = FileSystem::mount(disk).unwrap();
    assert_eq!(fs.read_file("/via ref").unwrap(), b"ok");
}

#[test]
fn damaged_entries_are_refused_but_removable() {
    let mut fs = fresh(SMALL);
    fs.write_file("/damaged", &pattern(3000, 1)).unwrap();
    fs.write_file("/fine", b"fine").unwrap();
    let mut disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let (index, _) = layout.find(&disk.data, layout.root, b"DAMAGED    ").unwrap();
    let offset = layout.cluster_offset(layout.root) + index * 32;
    // Size says 3000 bytes but the first cluster is gone.
    disk.data[offset + 20..offset + 22].copy_from_slice(&0u16.to_le_bytes());
    disk.data[offset + 26..offset + 28].copy_from_slice(&0u16.to_le_bytes());
    let mut fs = FileSystem::mount(disk).unwrap();
    assert!(matches!(fs.read_file("/damaged"), Err(Error::Corrupt(_))));
    assert!(matches!(fs.write_at("/damaged", 0, b"x"), Err(Error::Corrupt(_))));
    assert!(matches!(fs.append_file("/damaged", b"x"), Err(Error::Corrupt(_))));
    assert!(matches!(fs.truncate("/damaged", 10), Err(Error::Corrupt(_))));
    assert!(matches!(fs.write_file("/damaged", b"x"), Err(Error::Corrupt(_))));
    assert!(matches!(fs.copy_file("/damaged", "/copy"), Err(Error::Corrupt(_))));
    let report = fs.check().unwrap();
    assert!(report.problems.iter().any(|p| p.contains("/damaged")), "{:?}", report.problems);
    assert_eq!(report.lost_clusters, 6);
    fs.remove_file("/damaged").unwrap();
    assert_eq!(fs.read_file("/fine").unwrap(), b"fine");

    // A chain pointing outside the volume is reported, not followed.
    fs.write_file("/chain", &pattern(2000, 2)).unwrap();
    let mut disk = fs.unmount().unwrap();
    let layout = Layout::read(&disk.data);
    let (_, entry) = layout.find(&disk.data, layout.root, b"CHAIN      ").unwrap();
    layout.set_fat_entry(&mut disk.data, entry_cluster(&entry), 0x0FFF_FFF0);
    let mut fs = FileSystem::mount(disk).unwrap();
    assert!(matches!(fs.read_file("/chain"), Err(Error::Corrupt(_))));
    let report = fs.check().unwrap();
    assert!(report.problems.iter().any(|p| p.contains("out-of-range")), "{:?}", report.problems);
    fs.remove_file("/chain").unwrap();
}
