//! Mounting, syncing, path resolution and the file/directory operations behind the public API.

use crate::boot_sector::{self, FSINFO_UNKNOWN, put32};
use crate::dir::{
    ATTR_ARCHIVE, ATTR_DIRECTORY, DOT_NAME, DOTDOT_NAME, DirEntry, DirReader, ENTRY_SIZE, RawEntry, short_entry,
};
use crate::fat::{CLEAN_SHUTDOWN, FatCache};
use crate::name::validate_name;
use crate::path::components;
use crate::time::{self, FatTime};
use crate::{BlockDevice, CHAIN_HINTS, DeviceError, Error, FileSystem, Metadata, Result, SECTOR_SIZE, Timestamp};
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Largest file size FAT32 can record.
const MAX_FILE_SIZE: u64 = u32::MAX as u64;
/// Bytes per step when copying files.
const COPY_CHUNK: usize = 64 * 1024;
/// Short name placeholder in entry templates (replaced by `create_entry`).
const BLANK_NAME: [u8; 11] = [b' '; 11];

/// A resolved path.
pub(crate) enum Node {
    Root,
    Entry(DirEntry),
}

pub(crate) fn mount<D: BlockDevice>(mut device: D) -> Result<FileSystem<D>> {
    let mut sector = [0u8; SECTOR_SIZE];
    device.read_sectors(0, &mut sector)?;
    let geo = boot_sector::parse(&sector, device.sector_count())?;
    let mut fsinfo = None;
    let mut hints = (FSINFO_UNKNOWN, FSINFO_UNKNOWN);
    if let Some(fsinfo_sector) = geo.fsinfo_sector {
        let mut buffer = [0u8; SECTOR_SIZE];
        device.read_sectors(fsinfo_sector as u64, &mut buffer)?;
        // FSInfo is only a hint. A damaged one is ignored (and never rewritten): the free count
        // is recounted below and `check` reports the damage.
        if let Ok(values) = boot_sector::parse_fsinfo(&buffer) {
            hints = values;
            fsinfo = Some(buffer);
        }
    }
    let mut fs = FileSystem {
        device,
        geo,
        fat: FatCache::new(),
        free_count: 0,
        next_free: 2,
        fsinfo,
        fsinfo_on_disk: hints,
        volume_dirty: false,
        was_clean: false,
        read_only: false,
        poisoned: false,
        now: time::default_now,
        chain_hints: [None; CHAIN_HINTS],
        next_hint: 0,
        label: String::new(),
    };
    fs.was_clean = fs.fat_raw(1)? & CLEAN_SHUTDOWN != 0;
    if fs.fat_get(fs.geo.root_cluster)? == 0 {
        return Err(Error::Corrupt("root directory cluster is marked free"));
    }
    let (free, next) = hints;
    // The FSInfo free count is only a hint: trust it only after a clean unmount.
    fs.free_count = if fs.was_clean && free <= fs.geo.cluster_count { free } else { fs.count_free_clusters()? };
    fs.next_free = if fs.geo.is_valid_cluster(next) { next } else { 2 };
    fs.label = match fs.root_label() {
        Ok(Some(label)) => label,
        Ok(None) | Err(Error::Corrupt(_)) => fs.geo.bpb_label.clone(),
        Err(error) => return Err(error),
    };
    match fs.mark_dirty() {
        Ok(()) => {
            if !fs.was_clean {
                // An interrupted FAT update may have reached only the first copy, and an
                // interrupted rename may have left two entries sharing one cluster chain.
                fs.mirror_fat_copies()?;
                fs.repair_rename_leftovers()?;
            }
            fs.write_fsinfo()?
        }
        Err(Error::Device(DeviceError::ReadOnly)) => {
            fs.read_only = true;
            fs.fat.discard();
        }
        Err(error) => return Err(error),
    }
    Ok(fs)
}

impl<D: BlockDevice> FileSystem<D> {
    fn timestamp(&self) -> FatTime {
        time::encode((self.now)())
    }

    /// Write barrier: everything written so far is on the disk before anything written after it.
    /// (A write-back cache such as `CachedDevice` may reorder writes between two flushes.)
    pub(crate) fn barrier(&mut self) -> Result<()> {
        self.device.flush()?;
        Ok(())
    }

    /// Clears the clean-shutdown bit on disk (once per modification period). The barrier makes
    /// the cleared bit durable before any other write of the period.
    fn mark_dirty(&mut self) -> Result<()> {
        if self.volume_dirty {
            return Ok(());
        }
        let status = self.fat_raw(1)?;
        self.fat_set_raw(1, status & !CLEAN_SHUTDOWN)?;
        self.flush_fat()?;
        self.barrier()?;
        self.volume_dirty = true;
        Ok(())
    }

    /// Writes the FSInfo free count and next-free hint if they changed.
    fn write_fsinfo(&mut self) -> Result<()> {
        let (Some(sector), Some(mut buffer)) = (self.geo.fsinfo_sector, self.fsinfo) else {
            return Ok(());
        };
        let values = (self.free_count, self.next_free);
        if values == self.fsinfo_on_disk {
            return Ok(());
        }
        put32(&mut buffer, 488, values.0);
        put32(&mut buffer, 492, values.1);
        self.device.write_sectors(sector as u64, &buffer)?;
        self.fsinfo = Some(buffer);
        self.fsinfo_on_disk = values;
        Ok(())
    }

    /// Runs a mutating operation: marks the volume dirty first and writes the FAT and FSInfo
    /// afterwards (also when the operation failed, so rolled-back state reaches the disk).
    pub(crate) fn mutate<T>(&mut self, op: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        if self.read_only {
            return Err(Error::Device(DeviceError::ReadOnly));
        }
        if self.poisoned {
            return Err(Error::Corrupt("a failed rename could not be undone; mount the volume again and check it"));
        }
        self.mark_dirty()?;
        let result = op(self);
        let finished = self.flush_fat().and_then(|()| self.write_fsinfo());
        let value = result?;
        finished?;
        Ok(value)
    }

    pub(crate) fn sync_all(&mut self) -> Result<()> {
        if self.read_only {
            return Ok(());
        }
        self.flush_fat()?;
        self.write_fsinfo()?;
        self.device.flush()?;
        // A poisoned volume may hold two entries sharing a chain: it stays marked dirty, so the
        // next mount repairs it.
        if self.volume_dirty && !self.poisoned {
            let status = self.fat_raw(1)?;
            self.fat_set_raw(1, status | CLEAN_SHUTDOWN)?;
            self.flush_fat()?;
            self.device.flush()?;
            self.volume_dirty = false;
        }
        Ok(())
    }

    // ----- path resolution -------------------------------------------------------------------

    /// First cluster of the directory an entry describes.
    pub(crate) fn entry_dir_cluster(&self, entry: &DirEntry) -> Result<u32> {
        let cluster = entry.first_cluster();
        if !self.geo.is_valid_cluster(cluster) {
            return Err(Error::Corrupt("directory entry points to an invalid cluster"));
        }
        Ok(cluster)
    }

    fn node_dir_cluster(&self, node: &Node) -> Result<u32> {
        match node {
            Node::Root => Ok(self.geo.root_cluster),
            Node::Entry(entry) if entry.is_dir() => self.entry_dir_cluster(entry),
            Node::Entry(_) => Err(Error::NotADirectory),
        }
    }

    fn resolve_components(&mut self, parts: &[&str]) -> Result<Node> {
        let mut node = Node::Root;
        for part in parts {
            let dir = self.node_dir_cluster(&node)?;
            node = Node::Entry(self.lookup(dir, part)?.ok_or(Error::NotFound)?);
        }
        Ok(node)
    }

    pub(crate) fn resolve_path(&mut self, path: &str) -> Result<Node> {
        let parts = components(path)?;
        self.resolve_components(&parts)
    }

    /// Resolves the parent directory of `path`. `None` when `path` is the root.
    fn resolve_parent<'p>(&mut self, path: &'p str) -> Result<Option<(u32, &'p str)>> {
        let parts = components(path)?;
        let Some((name, parent)) = parts.split_last() else {
            return Ok(None);
        };
        let node = self.resolve_components(parent)?;
        let dir = self.node_dir_cluster(&node)?;
        Ok(Some((dir, *name)))
    }

    /// Resolves a path that must name a regular file.
    fn resolve_file(&mut self, path: &str) -> Result<DirEntry> {
        match self.resolve_path(path)? {
            Node::Root => Err(Error::IsADirectory),
            Node::Entry(entry) if entry.is_dir() => Err(Error::IsADirectory),
            Node::Entry(entry) => Ok(entry),
        }
    }

    /// First data cluster of a file, `None` for files without clusters.
    fn file_chain(&self, entry: &DirEntry) -> Result<Option<u32>> {
        let cluster = entry.first_cluster();
        if cluster == 0 {
            if entry.size() != 0 {
                return Err(Error::Corrupt("non-empty file without clusters"));
            }
            return Ok(None);
        }
        if !self.geo.is_valid_cluster(cluster) {
            return Err(Error::Corrupt("file points to an invalid cluster"));
        }
        Ok(Some(cluster))
    }

    // ----- queries ---------------------------------------------------------------------------

    pub(crate) fn op_metadata(&mut self, path: &str) -> Result<Metadata> {
        Ok(match self.resolve_path(path)? {
            Node::Root => Metadata {
                name: String::new(),
                is_dir: true,
                size: 0,
                created: Timestamp::default(),
                modified: Timestamp::default(),
                read_only: false,
                hidden: false,
                system: false,
            },
            Node::Entry(entry) => entry.metadata(),
        })
    }

    pub(crate) fn op_read_dir(&mut self, path: &str) -> Result<Vec<Metadata>> {
        let node = self.resolve_path(path)?;
        let dir = self.node_dir_cluster(&node)?;
        Ok(self.list_dir(dir)?.iter().map(DirEntry::metadata).collect())
    }

    pub(crate) fn op_read_file(&mut self, path: &str) -> Result<Vec<u8>> {
        let entry = self.resolve_file(path)?;
        let size = entry.size() as usize;
        let mut data = vec![0u8; size];
        if let Some(first) = self.file_chain(&entry)? {
            self.read_chain(first, 0, &mut data)?;
        }
        Ok(data)
    }

    pub(crate) fn op_read_at(&mut self, path: &str, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        let entry = self.resolve_file(path)?;
        let size = entry.size() as u64;
        if offset >= size || buffer.is_empty() {
            return Ok(0);
        }
        let count = (buffer.len() as u64).min(size - offset) as usize;
        let first = self.file_chain(&entry)?.ok_or(Error::Corrupt("non-empty file without clusters"))?;
        self.read_chain(first, offset, &mut buffer[..count])?;
        Ok(count)
    }

    // ----- directories -----------------------------------------------------------------------

    /// Creates a subdirectory (cluster with `.` and `..`, then its entry in `parent`).
    fn make_dir(&mut self, parent: u32, name: &str) -> Result<DirEntry> {
        validate_name(name)?;
        let now = self.timestamp();
        let cluster = self.allocate(None, 1)?;
        let mut buffer = vec![0u8; self.geo.cluster_size as usize];
        let parent_ref = if parent == self.geo.root_cluster { 0 } else { parent };
        buffer[..ENTRY_SIZE].copy_from_slice(&short_entry(DOT_NAME, ATTR_DIRECTORY, cluster, 0, now, now));
        buffer[ENTRY_SIZE..2 * ENTRY_SIZE].copy_from_slice(&short_entry(
            DOTDOT_NAME,
            ATTR_DIRECTORY,
            parent_ref,
            0,
            now,
            now,
        ));
        if let Err(error) = self.write_cluster(cluster, &buffer) {
            let _ = self.free_chain(cluster);
            return Err(error);
        }
        let template = short_entry(BLANK_NAME, ATTR_DIRECTORY, cluster, 0, now, now);
        self.create_entry_for_chain(parent, name, template, cluster)
    }

    /// Creates the entry for a new chain starting at `first` (0: no chain). If that fails, the
    /// chain is freed again, unless the entry itself may already be on disk (then it is kept:
    /// at worst it becomes lost clusters, never clusters a visible entry points to).
    fn create_entry_for_chain(&mut self, dir: u32, name: &str, template: RawEntry, first: u32) -> Result<DirEntry> {
        match self.create_entry(dir, name, template, None) {
            Ok(entry) => Ok(entry),
            Err(failure) => {
                if first != 0 && failure.maybe_written.is_none() {
                    let _ = self.free_chain(first);
                }
                Err(failure.error)
            }
        }
    }

    /// Frees the chain at `first` after the entry that referenced it was removed: the barrier
    /// puts the entry change on disk before the clusters can be handed out again.
    fn release_chain(&mut self, first: u32) -> Result<()> {
        self.barrier()?;
        self.free_chain(first)
    }

    pub(crate) fn op_create_dir(&mut self, path: &str) -> Result<()> {
        let Some((parent, name)) = self.resolve_parent(path)? else {
            return Err(Error::AlreadyExists);
        };
        validate_name(name)?;
        if self.lookup(parent, name)?.is_some() {
            return Err(Error::AlreadyExists);
        }
        self.make_dir(parent, name).map(|_| ())
    }

    pub(crate) fn op_create_dir_all(&mut self, path: &str) -> Result<()> {
        let parts = components(path)?;
        let mut dir = self.geo.root_cluster;
        for (i, part) in parts.iter().enumerate() {
            dir = match self.lookup(dir, part)? {
                Some(entry) if entry.is_dir() => self.entry_dir_cluster(&entry)?,
                Some(_) if i + 1 == parts.len() => return Err(Error::AlreadyExists),
                Some(_) => return Err(Error::NotADirectory),
                None => {
                    let entry = self.make_dir(dir, part)?;
                    entry.first_cluster()
                }
            };
        }
        Ok(())
    }

    pub(crate) fn op_remove_dir(&mut self, path: &str) -> Result<()> {
        let entry = match self.resolve_path(path)? {
            Node::Root => return Err(Error::RootDirectory),
            Node::Entry(entry) => entry,
        };
        if !entry.is_dir() {
            return Err(Error::NotADirectory);
        }
        let cluster = self.entry_dir_cluster(&entry)?;
        if !self.dir_is_empty(cluster)? {
            return Err(Error::DirectoryNotEmpty);
        }
        self.delete_entry(&entry)?;
        self.release_chain(cluster)
    }

    /// First clusters of every chain below (and including) directory `dir`.
    fn subtree_chains(&mut self, dir: u32) -> Result<Vec<u32>> {
        let mut chains = vec![dir];
        let mut stack = vec![dir];
        let mut seen = BTreeSet::new();
        seen.insert(dir);
        while let Some(current) = stack.pop() {
            for entry in self.list_dir(current)? {
                let cluster = entry.first_cluster();
                if !self.geo.is_valid_cluster(cluster) {
                    continue;
                }
                if entry.is_dir() {
                    if seen.insert(cluster) {
                        stack.push(cluster);
                        chains.push(cluster);
                    }
                } else {
                    chains.push(cluster);
                }
            }
        }
        Ok(chains)
    }

    pub(crate) fn op_remove_dir_all(&mut self, path: &str) -> Result<()> {
        let entry = match self.resolve_path(path)? {
            Node::Root => return Err(Error::RootDirectory),
            Node::Entry(entry) => entry,
        };
        if !entry.is_dir() {
            return Err(Error::NotADirectory);
        }
        let cluster = self.entry_dir_cluster(&entry)?;
        let chains = self.subtree_chains(cluster)?;
        // Unlink the whole tree with one entry deletion, then release its clusters.
        self.delete_entry(&entry)?;
        self.barrier()?;
        for first in chains {
            self.free_chain(first)?;
        }
        Ok(())
    }

    // ----- files -----------------------------------------------------------------------------

    pub(crate) fn op_touch(&mut self, path: &str) -> Result<()> {
        let Some((parent, name)) = self.resolve_parent(path)? else {
            return Ok(());
        };
        let now = self.timestamp();
        match self.lookup(parent, name)? {
            Some(mut entry) => {
                entry.set_modified(now);
                self.update_entry(&entry)
            }
            None => {
                let template = short_entry(BLANK_NAME, ATTR_ARCHIVE, 0, 0, now, now);
                self.create_entry_for_chain(parent, name, template, 0).map(|_| ())
            }
        }
    }

    pub(crate) fn op_remove_file(&mut self, path: &str) -> Result<()> {
        let entry = self.resolve_file(path)?;
        self.delete_entry(&entry)?;
        let cluster = entry.first_cluster();
        if self.geo.is_valid_cluster(cluster) {
            self.release_chain(cluster)?;
        }
        Ok(())
    }

    /// Frees clusters of the chain at `first` beyond the first `keep`.
    fn shrink_chain(&mut self, first: u32, keep: u32) -> Result<()> {
        if keep == 0 { self.free_chain(first) } else { self.truncate_chain(first, keep) }
    }

    /// Makes sure the file's chain can hold `size` bytes. Returns the first cluster and the
    /// previous cluster count (for rolling back).
    fn ensure_clusters(&mut self, entry: &DirEntry, size: u64) -> Result<(u32, u32)> {
        let needed = self.geo.clusters_for(size) as u32;
        // Refuse to write through a damaged entry rather than guess where its data lives.
        let first = self.file_chain(entry)?.unwrap_or(0);
        let (have, last) = if first != 0 { self.chain_info(first)? } else { (0, 0) };
        if needed <= have {
            return Ok((first, have));
        }
        if have == 0 {
            Ok((self.allocate(None, needed)?, 0))
        } else {
            self.allocate(Some(last), needed - have)?;
            Ok((first, have))
        }
    }

    /// Records the new size/first cluster/modification time in the directory entry. The FAT is
    /// flushed first and a barrier puts the data and the links on disk, so the entry never
    /// references unlinked clusters or data that has not been written.
    fn commit_entry(&mut self, entry: &mut DirEntry, first: u32, size: u64) -> Result<()> {
        self.flush_fat()?;
        self.barrier()?;
        entry.set_first_cluster(first);
        entry.set_size(size as u32);
        entry.set_modified(self.timestamp());
        entry.raw[11] |= ATTR_ARCHIVE;
        self.update_entry(entry)
    }

    /// Writes `data` at `offset` of an existing file, zero-filling any gap after the old end.
    fn write_range(&mut self, entry: &mut DirEntry, offset: u64, data: &[u8]) -> Result<()> {
        let end = offset.checked_add(data.len() as u64).ok_or(Error::FileTooLarge)?;
        if end > MAX_FILE_SIZE {
            return Err(Error::FileTooLarge);
        }
        if data.is_empty() {
            return Ok(());
        }
        let old_size = entry.size() as u64;
        let new_size = old_size.max(end);
        let (first, old_count) = self.ensure_clusters(entry, new_size)?;
        let written = (|| {
            if offset > old_size {
                self.zero_chain(first, old_size, offset - old_size)?;
            }
            self.write_chain(first, offset, data)
        })();
        if let Err(error) = written {
            let _ = self.shrink_chain(first, old_count);
            return Err(error);
        }
        self.commit_entry(entry, first, new_size)
    }

    /// Replaces a file's contents, reusing its clusters.
    fn replace_contents(&mut self, entry: &mut DirEntry, data: &[u8]) -> Result<()> {
        let size = data.len() as u64;
        let needed = self.geo.clusters_for(size) as u32;
        let old_first = self.file_chain(entry)?.unwrap_or(0);
        let had = if old_first != 0 { self.chain_info(old_first)?.0 } else { 0 };
        let first = if size == 0 {
            0
        } else {
            let (first, old_count) = self.ensure_clusters(entry, size)?;
            if let Err(error) = self.write_chain(first, 0, data) {
                let _ = self.shrink_chain(first, old_count);
                return Err(error);
            }
            first
        };
        // The entry shrinks (on disk) before the clusters it no longer covers are released.
        self.commit_entry(entry, first, size)?;
        if had > needed {
            self.barrier()?;
            self.shrink_chain(old_first, needed)?;
        }
        Ok(())
    }

    /// Writes a new chain holding `data` and creates its entry.
    fn create_file(&mut self, parent: u32, name: &str, data: &[u8]) -> Result<()> {
        validate_name(name)?;
        let now = self.timestamp();
        let first = if data.is_empty() {
            0
        } else {
            let first = self.allocate(None, self.geo.clusters_for(data.len() as u64) as u32)?;
            if let Err(error) = self.write_chain(first, 0, data) {
                let _ = self.free_chain(first);
                return Err(error);
            }
            first
        };
        let template = short_entry(BLANK_NAME, ATTR_ARCHIVE, first, data.len() as u32, now, now);
        self.create_entry_for_chain(parent, name, template, first).map(|_| ())
    }

    pub(crate) fn op_write_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        if data.len() as u64 > MAX_FILE_SIZE {
            return Err(Error::FileTooLarge);
        }
        let Some((parent, name)) = self.resolve_parent(path)? else {
            return Err(Error::IsADirectory);
        };
        match self.lookup(parent, name)? {
            Some(entry) if entry.is_dir() => Err(Error::IsADirectory),
            Some(mut entry) => self.replace_contents(&mut entry, data),
            None => self.create_file(parent, name, data),
        }
    }

    pub(crate) fn op_append_file(&mut self, path: &str, data: &[u8]) -> Result<()> {
        let Some((parent, name)) = self.resolve_parent(path)? else {
            return Err(Error::IsADirectory);
        };
        match self.lookup(parent, name)? {
            Some(entry) if entry.is_dir() => Err(Error::IsADirectory),
            Some(mut entry) => {
                let size = entry.size() as u64;
                self.write_range(&mut entry, size, data)
            }
            None => {
                if data.len() as u64 > MAX_FILE_SIZE {
                    return Err(Error::FileTooLarge);
                }
                self.create_file(parent, name, data)
            }
        }
    }

    pub(crate) fn op_write_at(&mut self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        let mut entry = self.resolve_file(path)?;
        self.write_range(&mut entry, offset, data)
    }

    pub(crate) fn op_truncate(&mut self, path: &str, length: u64) -> Result<()> {
        if length > MAX_FILE_SIZE {
            return Err(Error::FileTooLarge);
        }
        let mut entry = self.resolve_file(path)?;
        let old_size = entry.size() as u64;
        if length > old_size {
            let (first, old_count) = self.ensure_clusters(&entry, length)?;
            if let Err(error) = self.zero_chain(first, old_size, length - old_size) {
                let _ = self.shrink_chain(first, old_count);
                return Err(error);
            }
            return self.commit_entry(&mut entry, first, length);
        }
        let first = self.file_chain(&entry)?.unwrap_or(0);
        let keep = self.geo.clusters_for(length) as u32;
        let had = if first != 0 { self.chain_info(first)?.0 } else { 0 };
        if length == old_size && had <= keep {
            return Ok(());
        }
        // Shrink the entry first (on disk), then release the clusters it no longer covers.
        self.commit_entry(&mut entry, if keep == 0 { 0 } else { first }, length)?;
        if had > keep {
            self.barrier()?;
            self.shrink_chain(first, keep)?;
        }
        Ok(())
    }

    pub(crate) fn op_copy_file(&mut self, from: &str, to: &str) -> Result<()> {
        let source = self.resolve_file(from)?;
        let Some((parent, name)) = self.resolve_parent(to)? else {
            return Err(Error::AlreadyExists);
        };
        validate_name(name)?;
        if self.lookup(parent, name)?.is_some() {
            return Err(Error::AlreadyExists);
        }
        let size = source.size() as u64;
        let source_first = self.file_chain(&source)?;
        let now = self.timestamp();
        let first = match source_first {
            Some(source_first) if size > 0 => {
                let first = self.allocate(None, self.geo.clusters_for(size) as u32)?;
                let cluster_size = self.geo.cluster_size as usize;
                let chunk = (COPY_CHUNK / cluster_size).max(1) * cluster_size;
                let mut buffer = vec![0u8; chunk.min(size as usize)];
                let mut offset = 0u64;
                while offset < size {
                    let count = ((size - offset) as usize).min(buffer.len());
                    let copied = self
                        .read_chain(source_first, offset, &mut buffer[..count])
                        .and_then(|()| self.write_chain(first, offset, &buffer[..count]));
                    if let Err(error) = copied {
                        let _ = self.free_chain(first);
                        return Err(error);
                    }
                    offset += count as u64;
                }
                first
            }
            _ => 0,
        };
        let mut template = short_entry(BLANK_NAME, ATTR_ARCHIVE, first, size as u32, now, now);
        template[11] |= source.attr() & !(ATTR_DIRECTORY | crate::dir::ATTR_VOLUME_ID);
        self.create_entry_for_chain(parent, name, template, first).map(|_| ())
    }

    /// Points the `..` entry of directory `dir` at `parent`.
    fn set_dotdot(&mut self, dir: u32, parent: u32) -> Result<()> {
        let target = if parent == self.geo.root_cluster { 0 } else { parent };
        let mut reader = DirReader::new(dir);
        for _ in 0..16 {
            let Some((slot, raw)) = reader.next_slot(self)? else {
                break;
            };
            if raw[0] == 0 {
                break;
            }
            if raw[..11] == DOTDOT_NAME {
                let mut entry: RawEntry = raw;
                entry[20..22].copy_from_slice(&((target >> 16) as u16).to_le_bytes());
                entry[26..28].copy_from_slice(&(target as u16).to_le_bytes());
                return self.write_slots(&[(slot, entry)]);
            }
        }
        Ok(())
    }

    pub(crate) fn op_rename(&mut self, from: &str, to: &str) -> Result<()> {
        let source = match self.resolve_path(from)? {
            Node::Root => return Err(Error::RootDirectory),
            Node::Entry(entry) => entry,
        };
        let to_parts = components(to)?;
        let Some((name, parent_parts)) = to_parts.split_last() else {
            return Err(Error::RootDirectory);
        };
        validate_name(name)?;
        // Resolve the destination directory, remembering every directory on the way.
        let mut target_dir = self.geo.root_cluster;
        let mut ancestors = vec![target_dir];
        for part in parent_parts {
            let entry = self.lookup(target_dir, part)?.ok_or(Error::NotFound)?;
            if !entry.is_dir() {
                return Err(Error::NotADirectory);
            }
            target_dir = self.entry_dir_cluster(&entry)?;
            ancestors.push(target_dir);
        }
        if source.is_dir() && ancestors.contains(&source.first_cluster()) {
            return Err(Error::InvalidMove);
        }
        let mut exclude = None;
        if let Some(existing) = self.lookup(target_dir, name)? {
            if existing.dir_cluster != source.dir_cluster || existing.sfn_slot() != source.sfn_slot() {
                return Err(Error::AlreadyExists);
            }
            if existing.name() == *name {
                return Ok(());
            }
            // Case-only rename of the same entry: its own short name may be reused.
            exclude = Some(source.short_name());
        }
        // New entry first, then the old one goes away: a crash leaves two names, never none.
        let mut template = source.raw;
        template[11] |= if source.is_dir() { 0 } else { ATTR_ARCHIVE };
        let moved_dir = if source.is_dir() && target_dir != source.dir_cluster {
            Some(self.entry_dir_cluster(&source)?)
        } else {
            None
        };
        let created = match self.create_entry(target_dir, name, template, exclude) {
            Ok(entry) => entry,
            Err(failure) => {
                if let Some(entry) = failure.maybe_written {
                    self.undo_rename(&entry, None);
                }
                return Err(failure.error);
            }
        };
        // Both names exist now; the barrier keeps the old one until the new one is on disk (and
        // a moved directory's `..` is updated before its old entry goes away).
        let finished = (|| {
            self.barrier()?;
            if let Some(cluster) = moved_dir {
                self.set_dotdot(cluster, target_dir)?;
                self.barrier()?;
            }
            self.delete_entry(&source)
        })();
        if let Err(error) = finished {
            match self.entry_is_live(&source) {
                // Two entries sharing one cluster chain must not survive: removing either of them
                // would free data the other one still points to.
                Ok(true) => self.undo_rename(&created, moved_dir.map(|cluster| (cluster, source.dir_cluster))),
                // `delete_entry` wrote the old short entry's sector before it failed: only the new
                // name is left, so the rename took effect and must not be undone (that would
                // remove the last name of the data). Finish removing the old long-name entries.
                Ok(false) => {
                    if self.delete_long_entries(&source).and_then(|()| self.barrier()).is_ok() {
                        return Ok(());
                    }
                }
                // Unknown whether the old name is still there: keep the new one (never risk losing
                // both) and refuse changes until a mount repairs a possible duplicate.
                Err(_) => self.poisoned = true,
            }
            return Err(error);
        }
        Ok(())
    }

    /// Rolls back a rename that failed after its new entry was (possibly) written: points a moved
    /// directory's `..` back at its old parent and removes the new entry again (without freeing
    /// anything). If the new entry may still be there afterwards, the volume refuses further
    /// changes until it is mounted again (and the mount removes the duplicate).
    fn undo_rename(&mut self, created: &DirEntry, dotdot: Option<(u32, u32)>) {
        let undone = (|| {
            if let Some((cluster, old_parent)) = dotdot {
                self.set_dotdot(cluster, old_parent)?;
                self.barrier()?;
            }
            if self.entry_is_live(created)? {
                self.delete_entry(created)?;
            } else {
                // Only the long-name entries were written (in other sectors than the short one).
                self.delete_long_entries(created)?;
            }
            self.barrier()
        })();
        if undone.is_err() && self.entry_is_live(created).unwrap_or(true) {
            self.poisoned = true;
        }
    }

    // ----- repair after an unclean shutdown --------------------------------------------------

    /// Calls `visit` for every visible entry of the directory tree. Directories that cannot be
    /// read as a directory (reported by `check`) are skipped; each directory is visited once.
    fn for_each_entry(&mut self, mut visit: impl FnMut(&DirEntry)) -> Result<()> {
        let root = self.geo.root_cluster;
        let mut stack = vec![root];
        let mut seen = BTreeSet::new();
        seen.insert(root);
        while let Some(dir) = stack.pop() {
            let entries = match self.list_dir(dir) {
                Ok(entries) => entries,
                Err(Error::Corrupt(_)) => continue,
                Err(error) => return Err(error),
            };
            for entry in entries {
                visit(&entry);
                let cluster = entry.first_cluster();
                if entry.is_dir() && self.geo.is_valid_cluster(cluster) && seen.insert(cluster) {
                    stack.push(cluster);
                }
            }
        }
        Ok(())
    }

    /// Where the `..` entry of directory `dir` points (the root as its cluster number).
    fn dotdot_target(&mut self, dir: u32) -> Result<Option<u32>> {
        let mut iter = crate::dir::EntryIter::new(dir);
        for _ in 0..2 {
            match iter.next(self)? {
                Some(entry) if entry.raw[..11] == DOTDOT_NAME => {
                    let target = entry.first_cluster();
                    return Ok(Some(if target == 0 { self.geo.root_cluster } else { target }));
                }
                Some(_) => {}
                None => break,
            }
        }
        Ok(None)
    }

    /// Removes the duplicate entries a rename leaves behind when it is cut off between creating
    /// the new entry and removing the old one (a crash, or a failed rollback that poisoned the
    /// volume): two entries pointing at the same cluster chain, identical apart from the name.
    /// Left alone, removing either name would free the data the other one still references. One
    /// entry is kept (for a directory the one its `..` points into); the others are removed
    /// without freeing any clusters. Run on mounts after an unclean shutdown.
    fn repair_rename_leftovers(&mut self) -> Result<()> {
        let mut firsts = BTreeSet::new();
        let mut shared = BTreeSet::new();
        let geo = self.geo.clone();
        self.for_each_entry(|entry| {
            let cluster = entry.first_cluster();
            if geo.is_valid_cluster(cluster) && !firsts.insert(cluster) {
                shared.insert(cluster);
            }
        })?;
        if shared.is_empty() {
            return Ok(());
        }
        drop(firsts);
        let mut groups: Vec<(u32, Vec<DirEntry>)> = shared.iter().map(|&cluster| (cluster, Vec::new())).collect();
        self.for_each_entry(|entry| {
            let cluster = entry.first_cluster();
            if let Ok(i) = groups.binary_search_by_key(&cluster, |group| group.0) {
                groups[i].1.push(entry.clone());
            }
        })?;
        for (cluster, group) in groups {
            let mut keep = 0;
            let mut dotdot = None;
            if group[0].is_dir() {
                dotdot = self.dotdot_target(cluster)?;
                keep = group.iter().position(|entry| Some(entry.dir_cluster) == dotdot).unwrap_or(0);
            }
            let kept = group[keep].clone();
            let mut removed = false;
            for (i, entry) in group.iter().enumerate() {
                if i != keep && is_rename_copy(&kept, entry) {
                    self.delete_entry(entry)?;
                    removed = true;
                }
            }
            if removed && kept.is_dir() && dotdot != Some(kept.dir_cluster) {
                self.set_dotdot(cluster, kept.dir_cluster)?;
            }
        }
        self.barrier()
    }
}

/// Whether two entries are a rename's old and new entry: everything but the name (and the
/// archive bit and case flags, which a rename may set) is identical.
fn is_rename_copy(a: &DirEntry, b: &DirEntry) -> bool {
    a.is_dir() == b.is_dir() && (a.attr() | ATTR_ARCHIVE) == (b.attr() | ATTR_ARCHIVE) && a.raw[13..] == b.raw[13..]
}
