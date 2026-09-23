//! QA helper: extracts a FAT32 volume from a disk image with the independent `fatfs` crate and
//! cross-checks it against our crate. Read-only; ignored by default because it needs an image:
//!
//! ```text
//! RUSTKERNAL_IMAGE=disk.img RUSTKERNAL_EXTRACT=out_dir cargo test -p fat32 --test qa_extract -- --ignored --nocapture
//! ```
//!
//! `RUSTKERNAL_PARTITION` (1-4) picks the partition, the first one by default. Every file is
//! written below `RUSTKERNAL_EXTRACT` (if set) and a manifest line is printed per entry:
//! `D <path>` or `F <path> <size>`, plus the volume statistics both implementations report.

use fat32::mbr;
use fat32::{BlockDevice, DeviceError, FileSystem, SECTOR_SIZE};
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

struct Ram(Vec<u8>);

impl BlockDevice for Ram {
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

fn walk_fatfs<T: fatfs::ReadWriteSeek>(dir: fatfs::Dir<T>, prefix: &str, out: &mut BTreeMap<String, (bool, Vec<u8>)>, short: &mut Vec<String>) {
    for entry in dir.iter() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let short_name = String::from_utf8_lossy(entry.short_file_name_as_bytes()).to_string();
        let path = format!("{}/{}", prefix, name);
        short.push(format!("{}/{}", prefix, short_name.to_ascii_uppercase()));
        if entry.is_dir() {
            out.insert(path.clone(), (true, Vec::new()));
            walk_fatfs(entry.to_dir(), &path, out, short);
        } else {
            let mut data = Vec::new();
            entry.to_file().read_to_end(&mut data).unwrap();
            assert_eq!(data.len() as u64, entry.len(), "{}: fatfs size", path);
            out.insert(path, (false, data));
        }
    }
}

#[test]
#[ignore]
fn extract() {
    let path = std::env::var("RUSTKERNAL_IMAGE").expect("set RUSTKERNAL_IMAGE");
    let image = std::fs::read(&path).expect("read image");
    let mut disk = Ram(image.clone());
    let partitions = mbr::read_partitions(&mut disk).expect("MBR");
    let wanted: usize = std::env::var("RUSTKERNAL_PARTITION").ok().and_then(|p| p.parse().ok()).unwrap_or(1);
    let bytes = match partitions.iter().find(|p| p.index as usize + 1 == wanted) {
        Some(p) => {
            println!("partition {}: type 0x{:02x} start {} sectors {} active {}", wanted, p.partition_type, p.start_lba, p.sector_count, p.bootable);
            let start = p.start_lba as usize * SECTOR_SIZE;
            image[start..start + p.sector_count as usize * SECTOR_SIZE].to_vec()
        }
        None => {
            println!("no partition {}, using whole disk", wanted);
            image.clone()
        }
    };

    let mut ours_fs = FileSystem::mount(Ram(bytes.clone())).expect("mount with fat32");
    let stats = ours_fs.stats().unwrap();
    println!("ours: clean {} label {:?} total {} free {} cluster {}", ours_fs.was_cleanly_unmounted(), stats.label, stats.total_bytes, stats.free_bytes, stats.cluster_size);
    let mut ours = BTreeMap::new();
    walk_ours(&mut ours_fs, "/", &mut ours);
    let report = ours_fs.check().unwrap();
    println!("check: files {} dirs {} used {} lost {} problems {:?}", report.files, report.directories, report.used_clusters, report.lost_clusters, report.problems);

    let fatfs = fatfs::FileSystem::new(Cursor::new(bytes), fatfs::FsOptions::new()).expect("mount with fatfs");
    let fstats = fatfs.stats().unwrap();
    println!(
        "fatfs: label {:?} cluster {} total_clusters {} free_clusters {} dirty {}",
        fatfs.volume_label(),
        fstats.cluster_size(),
        fstats.total_clusters(),
        fstats.free_clusters(),
        fatfs.read_status_flags().unwrap().dirty()
    );
    let mut theirs = BTreeMap::new();
    let mut short = Vec::new();
    walk_fatfs(fatfs.root_dir(), "", &mut theirs, &mut short);

    let extract = std::env::var("RUSTKERNAL_EXTRACT").ok();
    for (path, (is_dir, data)) in &theirs {
        if *is_dir {
            println!("D {}", path);
        } else {
            println!("F {} {}", path, data.len());
        }
        if let Some(root) = &extract {
            let target = format!("{}{}", root, path);
            if *is_dir {
                std::fs::create_dir_all(&target).unwrap();
            } else {
                std::fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
                std::fs::write(&target, data).unwrap();
            }
        }
    }
    // Short names must be unique per directory.
    let mut seen = std::collections::HashSet::new();
    for name in &short {
        assert!(seen.insert(name.clone()), "duplicate short name {}", name);
    }
    println!("short names unique: {}", short.len());

    let only_ours: Vec<_> = ours.keys().filter(|k| !theirs.contains_key(*k)).collect();
    let only_theirs: Vec<_> = theirs.keys().filter(|k| !ours.contains_key(*k)).collect();
    println!("only in ours: {:?}", only_ours);
    println!("only in fatfs: {:?}", only_theirs);
    let differing: Vec<_> = ours.iter().filter(|(k, v)| theirs.get(*k).is_some_and(|t| t != *v)).map(|(k, _)| k).collect();
    println!("different contents: {:?}", differing);
    println!("entries: ours {} fatfs {}", ours.len(), theirs.len());
    assert!(only_ours.is_empty() && only_theirs.is_empty() && differing.is_empty(), "implementations disagree");
    assert!(report.is_clean(), "check() found problems");
}
