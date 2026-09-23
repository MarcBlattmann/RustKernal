//! Shared helpers for the integration tests: an in-memory disk, fatfs glue and tree dumps.
#![allow(dead_code)]

use fat32::{BlockDevice, DeviceError, FileSystem, FormatOptions, SECTOR_SIZE};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

pub const MIB: usize = 1024 * 1024;

/// A RAM disk with request counters and optional fault injection.
#[derive(Clone)]
pub struct RamDisk {
    pub data: Vec<u8>,
    pub reads: u64,
    pub writes: u64,
    pub sectors_read: u64,
    pub sectors_written: u64,
    pub flushes: u64,
    pub read_only: bool,
    /// Fail every write once this many more writes have succeeded.
    pub fail_writes_after: Option<u64>,
}

impl RamDisk {
    pub fn new(bytes: usize) -> Self {
        Self::from_vec(vec![0u8; bytes])
    }

    pub fn from_vec(data: Vec<u8>) -> Self {
        assert_eq!(data.len() % SECTOR_SIZE, 0);
        Self {
            data,
            reads: 0,
            writes: 0,
            sectors_read: 0,
            sectors_written: 0,
            flushes: 0,
            read_only: false,
            fail_writes_after: None,
        }
    }

    pub fn reset_counters(&mut self) {
        self.reads = 0;
        self.writes = 0;
        self.sectors_read = 0;
        self.sectors_written = 0;
        self.flushes = 0;
    }
}

impl BlockDevice for RamDisk {
    fn sector_count(&self) -> u64 {
        (self.data.len() / SECTOR_SIZE) as u64
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        assert!(!buffer.is_empty() && buffer.len().is_multiple_of(SECTOR_SIZE), "bad read length {}", buffer.len());
        let start = lba as usize * SECTOR_SIZE;
        if lba >= self.sector_count() || start + buffer.len() > self.data.len() {
            return Err(DeviceError::OutOfRange);
        }
        buffer.copy_from_slice(&self.data[start..start + buffer.len()]);
        self.reads += 1;
        self.sectors_read += (buffer.len() / SECTOR_SIZE) as u64;
        Ok(())
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        assert!(!buffer.is_empty() && buffer.len().is_multiple_of(SECTOR_SIZE), "bad write length {}", buffer.len());
        if self.read_only {
            return Err(DeviceError::ReadOnly);
        }
        if let Some(remaining) = self.fail_writes_after.as_mut() {
            if *remaining == 0 {
                return Err(DeviceError::Io);
            }
            *remaining -= 1;
        }
        let start = lba as usize * SECTOR_SIZE;
        if lba >= self.sector_count() || start + buffer.len() > self.data.len() {
            return Err(DeviceError::OutOfRange);
        }
        self.data[start..start + buffer.len()].copy_from_slice(buffer);
        self.writes += 1;
        self.sectors_written += (buffer.len() / SECTOR_SIZE) as u64;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), DeviceError> {
        self.flushes += 1;
        Ok(())
    }
}

/// Formats a fresh RAM disk of `bytes` bytes and mounts it.
pub fn fresh(bytes: usize) -> FileSystem<RamDisk> {
    let mut disk = RamDisk::new(bytes);
    fat32::format(&mut disk, &FormatOptions::default()).expect("format");
    FileSystem::mount(disk).expect("mount")
}

/// Asserts that `check()` finds no problems.
pub fn assert_clean<D: BlockDevice>(fs: &mut FileSystem<D>) {
    let report = fs.check().expect("check");
    assert!(report.problems.is_empty(), "check found problems: {:#?}", report.problems);
    assert_eq!(report.lost_clusters, 0);
}

/// Deterministic pseudo-random bytes.
pub fn pattern(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// Small xorshift PRNG for randomized tests.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

/// A file tree: path -> `Some(contents)` for files, `None` for directories.
pub type Tree = BTreeMap<String, Option<Vec<u8>>>;

fn join(parent: &str, name: &str) -> String {
    if parent == "/" { format!("/{}", name) } else { format!("{}/{}", parent, name) }
}

/// Reads the whole tree with our implementation.
pub fn dump_ours<D: BlockDevice>(fs: &mut FileSystem<D>) -> Tree {
    let mut tree = Tree::new();
    let mut stack = vec![String::from("/")];
    while let Some(dir) = stack.pop() {
        for entry in fs.read_dir(&dir).expect("read_dir") {
            let path = join(&dir, &entry.name);
            if entry.is_dir {
                tree.insert(path.clone(), None);
                stack.push(path);
            } else {
                let data = fs.read_file(&path).expect("read_file");
                assert_eq!(data.len() as u64, entry.size);
                tree.insert(path, Some(data));
            }
        }
    }
    tree
}

/// Reads the whole tree with fatfs.
pub fn dump_fatfs(image: &mut Vec<u8>) -> Tree {
    let mut cursor = Cursor::new(image);
    let fs = fatfs::FileSystem::new(&mut cursor, fatfs::FsOptions::new()).expect("fatfs mount");
    assert_eq!(fs.fat_type(), fatfs::FatType::Fat32);
    let mut tree = Tree::new();
    let mut stack = vec![(String::from("/"), fs.root_dir())];
    while let Some((path, dir)) = stack.pop() {
        for entry in dir.iter() {
            let entry = entry.expect("fatfs iter");
            let name = entry.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let child = join(&path, &name);
            if entry.is_dir() {
                tree.insert(child.clone(), None);
                stack.push((child, entry.to_dir()));
            } else {
                let mut data = Vec::new();
                entry.to_file().read_to_end(&mut data).expect("fatfs read");
                assert_eq!(data.len() as u64, entry.len());
                tree.insert(child, Some(data));
            }
        }
    }
    drop(stack);
    fs.unmount().expect("fatfs unmount");
    tree
}

/// Writes a tree with fatfs (parents before children).
pub fn write_tree_fatfs(image: &mut Vec<u8>, tree: &Tree) {
    let mut cursor = Cursor::new(image);
    let fs = fatfs::FileSystem::new(&mut cursor, fatfs::FsOptions::new()).expect("fatfs mount");
    {
        let root = fs.root_dir();
        for (path, contents) in tree {
            let relative = &path[1..];
            match contents {
                None => {
                    root.create_dir(relative).expect("fatfs mkdir");
                }
                Some(data) => {
                    let mut file = root.create_file(relative).expect("fatfs create");
                    file.truncate().expect("fatfs truncate");
                    file.write_all(data).expect("fatfs write");
                    file.flush().expect("fatfs flush");
                }
            }
        }
    }
    fs.unmount().expect("fatfs unmount");
}

/// Creates a FAT32 image of `bytes` bytes with fatfs.
pub fn fatfs_format(bytes: usize, label: &[u8; 11]) -> Vec<u8> {
    let mut image = vec![0u8; bytes];
    {
        let mut cursor = Cursor::new(&mut image);
        fatfs::format_volume(
            &mut cursor,
            fatfs::FormatVolumeOptions::new()
                .fat_type(fatfs::FatType::Fat32)
                .volume_label(*label)
                .volume_id(0x1234_5678),
        )
        .expect("fatfs format");
    }
    image
}

/// Case-insensitive comparison of two trees (FAT names are case-preserving but the oracle
/// compares exact names, so this is only used where case may legitimately differ).
pub fn assert_trees_equal(ours: &Tree, theirs: &Tree) {
    let left: Vec<&String> = ours.keys().collect();
    let right: Vec<&String> = theirs.keys().collect();
    assert_eq!(left, right, "trees have different paths");
    for (path, contents) in ours {
        assert!(contents == &theirs[path], "contents differ for {}", path);
    }
}
