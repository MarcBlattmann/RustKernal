//! Read-only consistency check (`fsck -n`).

use crate::boot_sector::{self, FSINFO_UNKNOWN, le32};
use crate::dir::{DOT_NAME, DOTDOT_NAME, DirEntry, EntryIter};
use crate::fat::{BAD_CLUSTER, EOC_MIN, FAT_MASK};
use crate::name::{is_valid_short_char, short_name_to_string, upper_key};
use crate::{BlockDevice, CheckReport, Error, FileSystem, Result, SECTOR_SIZE};
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// At most this many problems are listed individually.
const MAX_PROBLEMS: usize = 1000;
const CHUNK_SECTORS: u32 = 128;

struct Problems {
    list: Vec<String>,
    suppressed: usize,
}

impl Problems {
    fn add(&mut self, problem: String) {
        if self.list.len() < MAX_PROBLEMS {
            self.list.push(problem);
        } else {
            self.suppressed += 1;
        }
    }
}

struct Bitmap(Vec<u64>);

impl Bitmap {
    fn new(bits: u32) -> Self {
        Bitmap(vec![0; (bits as usize).div_ceil(64)])
    }
    fn get(&self, bit: u32) -> bool {
        self.0[bit as usize / 64] & (1 << (bit % 64)) != 0
    }
    fn set(&mut self, bit: u32) {
        self.0[bit as usize / 64] |= 1 << (bit % 64);
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent == "/" { format!("/{}", name) } else { format!("{}/{}", parent, name) }
}

/// Short-name bytes that are never valid on disk.
fn short_name_is_valid(raw: &[u8]) -> bool {
    if raw[0] == b' ' {
        return false;
    }
    raw[..11].iter().enumerate().all(|(i, &b)| b == b' ' || is_valid_short_char(b) || (i == 0 && b == 0x05))
}

struct Walker<'a, D: BlockDevice> {
    fs: &'a mut FileSystem<D>,
    reached: Bitmap,
    problems: Problems,
}

impl<D: BlockDevice> Walker<'_, D> {
    /// Marks the chain starting at `first` as reached and returns its length.
    fn mark_chain(&mut self, first: u32, path: &str) -> Result<u32> {
        let mut cluster = first;
        let mut count = 0u32;
        loop {
            if self.reached.get(cluster) {
                self.problems.add(format!("{}: cluster {} is cross-linked (or the chain loops)", path, cluster));
                return Ok(count);
            }
            self.reached.set(cluster);
            count += 1;
            let value = self.fs.fat_get(cluster)?;
            if value >= EOC_MIN {
                return Ok(count);
            }
            if value == 0 {
                self.problems.add(format!("{}: chain contains free cluster {}", path, cluster));
                return Ok(count);
            }
            if value == BAD_CLUSTER {
                self.problems.add(format!("{}: chain contains bad cluster {}", path, cluster));
                return Ok(count);
            }
            if !self.fs.geo.is_valid_cluster(value) {
                self.problems.add(format!("{}: cluster {} links to out-of-range cluster {}", path, cluster, value));
                return Ok(count);
            }
            cluster = value;
        }
    }

    fn check_directory(
        &mut self,
        dir: u32,
        parent: u32,
        path: &str,
        queue: &mut Vec<(u32, u32, String)>,
        report: &mut CheckReport,
    ) -> Result<()> {
        let root = self.fs.geo.root_cluster;
        let is_root = dir == root;
        let mut iter = EntryIter::new(dir);
        let mut names: BTreeSet<String> = BTreeSet::new();
        let (mut dot, mut dotdot) = (false, false);
        // Position among the short entries ('.' and '..' must be the first two; some
        // implementations put long-name entries in front of them, which is harmless).
        let mut position = 0usize;
        loop {
            let entry: DirEntry = match iter.next(self.fs) {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(Error::Corrupt(what)) => {
                    self.problems.add(format!("{}: {}", path, what));
                    break;
                }
                Err(error) => return Err(error),
            };
            position += 1;
            if entry.is_dot() {
                if is_root {
                    self.problems.add(format!("{}: '.' or '..' entry in the root directory", path));
                } else if entry.raw[..11] == DOT_NAME {
                    dot = true;
                    if position != 1 || entry.first_cluster() != dir {
                        self.problems.add(format!("{}: broken '.' entry", path));
                    }
                } else if entry.raw[..11] == DOTDOT_NAME {
                    dotdot = true;
                    let target = entry.first_cluster();
                    let expected_ok = if parent == root { target == 0 || target == root } else { target == parent };
                    if position != 2 || !expected_ok {
                        self.problems.add(format!("{}: broken '..' entry", path));
                    }
                }
                continue;
            }
            if entry.is_volume_label() {
                if !is_root {
                    self.problems.add(format!("{}: volume label entry outside the root directory", path));
                }
                continue;
            }
            let name = entry.name();
            let entry_path = join(path, &name);
            if !short_name_is_valid(&entry.raw[..11]) {
                self.problems.add(format!(
                    "{}: invalid short name {:?}",
                    entry_path,
                    short_name_to_string(&entry.short_name(), 0)
                ));
            }
            let mut keys = BTreeSet::new();
            keys.insert(upper_key(&short_name_to_string(&entry.short_name(), 0)));
            if let Some(long) = &entry.long_name {
                keys.insert(upper_key(long));
            }
            for key in keys {
                if !names.insert(key) {
                    self.problems.add(format!("{}: duplicate name in directory", entry_path));
                }
            }
            let first = entry.first_cluster();
            if entry.is_dir() {
                report.directories += 1;
                if !self.fs.geo.is_valid_cluster(first) {
                    self.problems.add(format!("{}: directory starts at invalid cluster {}", entry_path, first));
                    continue;
                }
                if self.reached.get(first) {
                    self.problems.add(format!("{}: directory cluster {} is cross-linked", entry_path, first));
                    continue;
                }
                self.mark_chain(first, &entry_path)?;
                queue.push((first, dir, entry_path));
            } else {
                report.files += 1;
                let size = entry.size() as u64;
                let expected = size.div_ceil(self.fs.geo.cluster_size as u64);
                if first == 0 {
                    if size != 0 {
                        self.problems.add(format!("{}: {} bytes but no clusters", entry_path, size));
                    }
                } else if !self.fs.geo.is_valid_cluster(first) {
                    self.problems.add(format!("{}: first cluster {} is out of range", entry_path, first));
                } else if self.reached.get(first) {
                    self.problems.add(format!("{}: cluster {} is cross-linked", entry_path, first));
                } else {
                    let count = self.mark_chain(first, &entry_path)? as u64;
                    if count != expected {
                        self.problems.add(format!(
                            "{}: chain has {} clusters but the size ({} bytes) needs {}",
                            entry_path, count, size, expected
                        ));
                    }
                }
            }
        }
        if iter.orphans > 0 {
            self.problems.add(format!("{}: {} orphaned long-name entries", path, iter.orphans));
        }
        if !is_root && (!dot || !dotdot) {
            self.problems.add(format!("{}: missing '.' or '..' entry", path));
        }
        Ok(())
    }
}

pub(crate) fn check<D: BlockDevice>(fs: &mut FileSystem<D>) -> Result<CheckReport> {
    let mut report = CheckReport { was_cleanly_unmounted: fs.was_clean, ..CheckReport::default() };
    let geo = fs.geo.clone();
    let max = geo.max_cluster();
    let mut walker =
        Walker { fs, reached: Bitmap::new(max + 1), problems: Problems { list: Vec::new(), suppressed: 0 } };

    // Reserved FAT entries.
    let fat0 = walker.fs.fat_get(0)?;
    if fat0 & 0xFF != geo.media as u32 {
        walker.problems.add(format!("FAT[0] ({:#010x}) does not match the media byte {:#04x}", fat0, geo.media));
    }

    // FAT copies must be identical when mirroring.
    if geo.mirroring && geo.num_fats > 1 {
        let chunk_bytes = CHUNK_SECTORS as usize * SECTOR_SIZE;
        let mut first = vec![0u8; chunk_bytes];
        let mut other = vec![0u8; chunk_bytes];
        for copy in 1..geo.num_fats {
            let mut sector = 0u32;
            while sector < geo.fat_size {
                let count = CHUNK_SECTORS.min(geo.fat_size - sector);
                let bytes = count as usize * SECTOR_SIZE;
                walker.fs.device.read_sectors(geo.fat_lba(0) + sector as u64, &mut first[..bytes])?;
                walker.fs.device.read_sectors(geo.fat_lba(copy) + sector as u64, &mut other[..bytes])?;
                if first[..bytes] != other[..bytes] {
                    walker.problems.add(format!("FAT copy {} differs from FAT copy 0", copy));
                    break;
                }
                sector += count;
            }
        }
    }

    // Walk the directory tree.
    let root = geo.root_cluster;
    walker.mark_chain(root, "/")?;
    let mut queue: Vec<(u32, u32, String)> = vec![(root, root, String::from("/"))];
    while let Some((dir, parent, path)) = queue.pop() {
        walker.check_directory(dir, parent, &path, &mut queue, &mut report)?;
    }

    // Compare the FAT with what the tree references.
    let (mut free, mut used, mut lost) = (0u32, 0u32, 0u32);
    let last_sector = max / (SECTOR_SIZE as u32 / 4);
    let mut buffer = vec![0u8; CHUNK_SECTORS as usize * SECTOR_SIZE];
    let mut sector = 0u32;
    while sector <= last_sector {
        let count = CHUNK_SECTORS.min(last_sector + 1 - sector);
        let bytes = &mut buffer[..count as usize * SECTOR_SIZE];
        walker.fs.device.read_sectors(geo.fat_lba(geo.active_fat) + sector as u64, bytes)?;
        for index in 0..count * (SECTOR_SIZE as u32 / 4) {
            let cluster = sector * (SECTOR_SIZE as u32 / 4) + index;
            if cluster < 2 || cluster > max {
                continue;
            }
            let value = le32(bytes, index as usize * 4) & FAT_MASK;
            if value == 0 {
                free += 1;
            } else if value != BAD_CLUSTER {
                used += 1;
                if !walker.reached.get(cluster) {
                    lost += 1;
                }
            }
        }
        sector += count;
    }
    report.used_clusters = used;
    report.lost_clusters = lost;
    if lost > 0 {
        walker.problems.add(format!("{} lost clusters (allocated but not referenced)", lost));
    }

    if let Some(fsinfo_sector) = geo.fsinfo_sector {
        let mut sector = [0u8; SECTOR_SIZE];
        walker.fs.device.read_sectors(fsinfo_sector as u64, &mut sector)?;
        match boot_sector::parse_fsinfo(&sector) {
            Ok((count, _)) if count != FSINFO_UNKNOWN && count != free => {
                walker.problems.add(format!("FSInfo free cluster count is {} but {} clusters are free", count, free));
            }
            Ok(_) => {}
            Err(_) => walker.problems.add(String::from("invalid FSInfo sector")),
        }
    }
    if walker.fs.free_count != free && !walker.fs.read_only {
        walker
            .problems
            .add(format!("cached free cluster count is {} but {} clusters are free", walker.fs.free_count, free));
    }

    let Problems { mut list, suppressed } = walker.problems;
    if suppressed > 0 {
        list.push(format!("... and {} more problems", suppressed));
    }
    report.problems = list;
    Ok(report)
}
