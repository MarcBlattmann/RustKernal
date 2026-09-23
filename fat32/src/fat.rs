//! File allocation table access, cluster allocation and cluster chains.
//!
//! FAT sectors are cached in a small write-back cache inside the filesystem. Dirty sectors are
//! written to every FAT copy (or only the active one when mirroring is disabled) by
//! [`FileSystem::flush_fat`], which the operations call before making newly linked clusters
//! reachable from a directory entry and at the end of every mutating call.

use crate::boot_sector::{le32, put32};
use crate::{BlockDevice, CHAIN_HINTS, ChainHint, Error, FileSystem, Result, SECTOR_SIZE};
use alloc::vec::Vec;

/// Only the low 28 bits of a FAT32 entry are the cluster value; the top 4 bits are reserved and
/// must be preserved when writing.
pub(crate) const FAT_MASK: u32 = 0x0FFF_FFFF;
/// End-of-chain values are `>= EOC_MIN`.
pub(crate) const EOC_MIN: u32 = 0x0FFF_FFF8;
/// The end-of-chain value this implementation writes.
pub(crate) const EOC: u32 = 0x0FFF_FFFF;
pub(crate) const BAD_CLUSTER: u32 = 0x0FFF_FFF7;
/// FAT[1] bit: set when the volume was cleanly unmounted.
pub(crate) const CLEAN_SHUTDOWN: u32 = 0x0800_0000;
/// FAT[1] bit: set when no disk I/O errors were encountered.
pub(crate) const NO_HARD_ERROR: u32 = 0x0400_0000;

const ENTRIES_PER_SECTOR: u32 = (SECTOR_SIZE / 4) as u32;
const FAT_CACHE_SECTORS: usize = 64;
/// Sectors per device request when scanning the whole FAT.
const SCAN_CHUNK_SECTORS: u32 = 128;

struct FatSlot {
    /// Sector index relative to the start of a FAT copy.
    sector: u32,
    data: [u8; SECTOR_SIZE],
    dirty: bool,
    last_used: u64,
}

pub(crate) struct FatCache {
    slots: Vec<FatSlot>,
    tick: u64,
}

impl FatCache {
    pub fn new() -> Self {
        Self { slots: Vec::new(), tick: 0 }
    }

    /// Drops every cached sector, including unwritten modifications.
    pub fn discard(&mut self) {
        self.slots.clear();
    }
}

impl<D: BlockDevice> FileSystem<D> {
    /// Index of the cache slot holding FAT sector `sector`, loading it if necessary.
    fn fat_slot(&mut self, sector: u32) -> Result<usize> {
        self.fat.tick += 1;
        let tick = self.fat.tick;
        if let Some(index) = self.fat.slots.iter().position(|slot| slot.sector == sector) {
            self.fat.slots[index].last_used = tick;
            return Ok(index);
        }
        if sector >= self.geo.fat_size {
            return Err(Error::Corrupt("FAT access out of range"));
        }
        let mut data = [0u8; SECTOR_SIZE];
        let lba = self.geo.fat_lba(self.geo.active_fat) + sector as u64;
        self.device.read_sectors(lba, &mut data)?;
        let slot = FatSlot { sector, data, dirty: false, last_used: tick };
        if self.fat.slots.len() < FAT_CACHE_SECTORS {
            self.fat.slots.push(slot);
            return Ok(self.fat.slots.len() - 1);
        }
        let victim = (0..self.fat.slots.len()).min_by_key(|&i| self.fat.slots[i].last_used).unwrap_or(0);
        if self.fat.slots[victim].dirty {
            let old = self.fat.slots[victim].sector;
            let bytes = self.fat.slots[victim].data;
            self.write_fat_sectors(old, &bytes)?;
        }
        self.fat.slots[victim] = slot;
        Ok(victim)
    }

    /// Writes consecutive FAT sectors starting at `sector` to every FAT copy in use.
    fn write_fat_sectors(&mut self, sector: u32, bytes: &[u8]) -> Result<()> {
        if self.geo.mirroring {
            for copy in 0..self.geo.num_fats {
                let lba = self.geo.fat_lba(copy) + sector as u64;
                self.device.write_sectors(lba, bytes)?;
            }
        } else {
            let lba = self.geo.fat_lba(self.geo.active_fat) + sector as u64;
            self.device.write_sectors(lba, bytes)?;
        }
        Ok(())
    }

    /// Writes every dirty cached FAT sector to all FAT copies (in ascending order, batching
    /// consecutive sectors into one request).
    pub(crate) fn flush_fat(&mut self) -> Result<()> {
        let mut dirty: Vec<usize> = (0..self.fat.slots.len()).filter(|&i| self.fat.slots[i].dirty).collect();
        if dirty.is_empty() {
            return Ok(());
        }
        dirty.sort_by_key(|&i| self.fat.slots[i].sector);
        let mut start = 0;
        let mut buffer = Vec::new();
        while start < dirty.len() {
            let mut end = start + 1;
            while end < dirty.len() && self.fat.slots[dirty[end]].sector == self.fat.slots[dirty[end - 1]].sector + 1 {
                end += 1;
            }
            buffer.clear();
            for &index in &dirty[start..end] {
                buffer.extend_from_slice(&self.fat.slots[index].data);
            }
            let first = self.fat.slots[dirty[start]].sector;
            self.write_fat_sectors(first, &buffer)?;
            for &index in &dirty[start..end] {
                self.fat.slots[index].dirty = false;
            }
            start = end;
        }
        Ok(())
    }

    /// Raw 32-bit FAT entry including the reserved top bits.
    pub(crate) fn fat_raw(&mut self, cluster: u32) -> Result<u32> {
        let slot = self.fat_slot(cluster / ENTRIES_PER_SECTOR)?;
        let offset = (cluster % ENTRIES_PER_SECTOR) as usize * 4;
        Ok(le32(&self.fat.slots[slot].data, offset))
    }

    /// Writes a raw 32-bit FAT entry (including the reserved top bits).
    pub(crate) fn fat_set_raw(&mut self, cluster: u32, value: u32) -> Result<()> {
        let slot = self.fat_slot(cluster / ENTRIES_PER_SECTOR)?;
        let offset = (cluster % ENTRIES_PER_SECTOR) as usize * 4;
        let entry = &mut self.fat.slots[slot];
        put32(&mut entry.data, offset, value);
        entry.dirty = true;
        Ok(())
    }

    /// The 28-bit FAT entry of `cluster`.
    pub(crate) fn fat_get(&mut self, cluster: u32) -> Result<u32> {
        Ok(self.fat_raw(cluster)? & FAT_MASK)
    }

    /// Sets the 28-bit FAT entry of `cluster`, preserving the reserved top 4 bits.
    pub(crate) fn fat_set(&mut self, cluster: u32, value: u32) -> Result<()> {
        debug_assert!(self.geo.is_valid_cluster(cluster));
        let raw = self.fat_raw(cluster)?;
        let old = raw & FAT_MASK;
        if self.geo.is_valid_cluster(old) && old != value {
            // A link is being cut: remembered chain positions may now be stale.
            self.chain_hints = [None; CHAIN_HINTS];
        }
        self.fat_set_raw(cluster, (raw & !FAT_MASK) | (value & FAT_MASK))
    }

    /// The cluster following `cluster` in its chain, `None` at the end of the chain.
    pub(crate) fn next_cluster(&mut self, cluster: u32) -> Result<Option<u32>> {
        let value = self.fat_get(cluster)?;
        if value >= EOC_MIN {
            Ok(None)
        } else if value == BAD_CLUSTER {
            Err(Error::Corrupt("bad cluster in a cluster chain"))
        } else if value == 0 {
            Err(Error::Corrupt("free cluster in a cluster chain"))
        } else if !self.geo.is_valid_cluster(value) {
            Err(Error::Corrupt("cluster chain points outside the volume"))
        } else {
            Ok(Some(value))
        }
    }

    fn remember(&mut self, first: u32, index: u32, cluster: u32) {
        if let Some(slot) = self.chain_hints.iter_mut().flatten().find(|hint| hint.first == first) {
            slot.index = index;
            slot.cluster = cluster;
            return;
        }
        self.chain_hints[self.next_hint] = Some(ChainHint { first, index, cluster });
        self.next_hint = (self.next_hint + 1) % CHAIN_HINTS;
    }

    /// Records that cluster number `index` of the chain starting at `first` is `cluster`.
    pub(crate) fn set_chain_hint(&mut self, first: u32, index: u32, cluster: u32) {
        self.remember(first, index, cluster);
    }

    /// The `index`-th cluster (0-based) of the chain starting at `first`.
    pub(crate) fn cluster_at(&mut self, first: u32, index: u32) -> Result<u32> {
        if !self.geo.is_valid_cluster(first) {
            return Err(Error::Corrupt("invalid first cluster"));
        }
        let (mut position, mut cluster) =
            match self.chain_hints.iter().flatten().find(|hint| hint.first == first && hint.index <= index) {
                Some(hint) => (hint.index, hint.cluster),
                None => (0, first),
            };
        while position < index {
            cluster = self.next_cluster(cluster)?.ok_or(Error::Corrupt("cluster chain is shorter than the file"))?;
            position += 1;
        }
        self.remember(first, index, cluster);
        Ok(cluster)
    }

    /// Number of clusters in the chain starting at `first` and its last cluster.
    pub(crate) fn chain_info(&mut self, first: u32) -> Result<(u32, u32)> {
        if !self.geo.is_valid_cluster(first) {
            return Err(Error::Corrupt("invalid first cluster"));
        }
        let (mut count, mut cluster) = match self.chain_hints.iter().flatten().find(|hint| hint.first == first) {
            Some(hint) => (hint.index + 1, hint.cluster),
            None => (1, first),
        };
        while let Some(next) = self.next_cluster(cluster)? {
            cluster = next;
            count += 1;
            if count > self.geo.cluster_count {
                return Err(Error::Corrupt("cluster chain loops"));
            }
        }
        self.remember(first, count - 1, cluster);
        Ok((count, cluster))
    }

    /// First free cluster in `from..=to`, reading the FAT a sector at a time.
    fn scan_free(&mut self, from: u32, to: u32) -> Result<Option<u32>> {
        let mut cluster = from;
        while cluster <= to {
            let sector = cluster / ENTRIES_PER_SECTOR;
            let slot = self.fat_slot(sector)?;
            let sector_end = ((sector + 1) * ENTRIES_PER_SECTOR - 1).min(to);
            let data = &self.fat.slots[slot].data;
            for candidate in cluster..=sector_end {
                let offset = (candidate % ENTRIES_PER_SECTOR) as usize * 4;
                if le32(data, offset) & FAT_MASK == 0 {
                    return Ok(Some(candidate));
                }
            }
            cluster = sector_end + 1;
        }
        Ok(None)
    }

    /// Finds a free cluster starting at the next-free hint and wrapping around.
    fn find_free(&mut self) -> Result<Option<u32>> {
        let max = self.geo.max_cluster();
        let start = if self.geo.is_valid_cluster(self.next_free) { self.next_free } else { 2 };
        if let Some(cluster) = self.scan_free(start, max)? {
            return Ok(Some(cluster));
        }
        if start > 2 {
            return self.scan_free(2, start - 1);
        }
        Ok(None)
    }

    /// Allocates one cluster, marks it end-of-chain and links it after `previous`.
    fn alloc_one(&mut self, previous: Option<u32>) -> Result<u32> {
        if self.free_count == 0 {
            return Err(Error::NoSpace);
        }
        let cluster = match self.find_free()? {
            Some(cluster) => cluster,
            None => {
                // The free count was wrong; correct it so later calls fail fast.
                self.free_count = 0;
                return Err(Error::NoSpace);
            }
        };
        self.fat_set(cluster, EOC)?;
        if let Some(previous) = previous {
            self.fat_set(previous, cluster)?;
        }
        self.free_count -= 1;
        self.next_free = if cluster >= self.geo.max_cluster() { 2 } else { cluster + 1 };
        Ok(cluster)
    }

    /// Allocates `count` clusters as a chain appended after `last` (the current end of a chain,
    /// or `None` for a new chain). Returns the first new cluster. On failure every cluster
    /// allocated by this call is freed again and `last` is restored as end of chain.
    pub(crate) fn allocate(&mut self, last: Option<u32>, count: u32) -> Result<u32> {
        debug_assert!(count > 0);
        if count > self.free_count {
            return Err(Error::NoSpace);
        }
        let mut first = None;
        let mut previous = last;
        for _ in 0..count {
            match self.alloc_one(previous) {
                Ok(cluster) => {
                    first.get_or_insert(cluster);
                    previous = Some(cluster);
                }
                Err(error) => {
                    if let Some(first) = first {
                        let _ = self.free_chain(first);
                    }
                    if let Some(last) = last {
                        let _ = self.fat_set(last, EOC);
                    }
                    return Err(error);
                }
            }
        }
        Ok(first.unwrap_or(0))
    }

    /// Frees every cluster of the chain starting at `first`. Stops quietly at anything that is
    /// not a valid link (so a damaged chain can still be deleted) and never frees a cluster twice.
    pub(crate) fn free_chain(&mut self, first: u32) -> Result<()> {
        let mut cluster = first;
        let mut freed = 0u32;
        while self.geo.is_valid_cluster(cluster) {
            let value = self.fat_get(cluster)?;
            if value == 0 || value == BAD_CLUSTER {
                break;
            }
            self.fat_set(cluster, 0)?;
            self.free_count = (self.free_count + 1).min(self.geo.cluster_count);
            freed += 1;
            if value >= EOC_MIN || freed >= self.geo.cluster_count {
                break;
            }
            cluster = value;
        }
        self.chain_hints = [None; CHAIN_HINTS];
        Ok(())
    }

    /// Cuts the chain starting at `first` after `keep` clusters (`keep >= 1`) and frees the rest.
    pub(crate) fn truncate_chain(&mut self, first: u32, keep: u32) -> Result<()> {
        let last_kept = self.cluster_at(first, keep - 1)?;
        let value = self.fat_get(last_kept)?;
        if value >= EOC_MIN {
            return Ok(());
        }
        self.fat_set(last_kept, EOC)?;
        if self.geo.is_valid_cluster(value) {
            self.free_chain(value)?;
        }
        Ok(())
    }

    /// Copies the first FAT over the other copies wherever they differ (after an unclean
    /// shutdown the first copy is authoritative: every FAT update writes it first).
    pub(crate) fn mirror_fat_copies(&mut self) -> Result<()> {
        if !self.geo.mirroring || self.geo.num_fats < 2 {
            return Ok(());
        }
        self.flush_fat()?;
        let mut first = alloc::vec![0u8; SCAN_CHUNK_SECTORS as usize * SECTOR_SIZE];
        let mut other = alloc::vec![0u8; SCAN_CHUNK_SECTORS as usize * SECTOR_SIZE];
        let mut sector = 0u32;
        while sector < self.geo.fat_size {
            let count = SCAN_CHUNK_SECTORS.min(self.geo.fat_size - sector);
            let bytes = count as usize * SECTOR_SIZE;
            self.device.read_sectors(self.geo.fat_lba(0) + sector as u64, &mut first[..bytes])?;
            for copy in 1..self.geo.num_fats {
                let lba = self.geo.fat_lba(copy) + sector as u64;
                self.device.read_sectors(lba, &mut other[..bytes])?;
                if first[..bytes] != other[..bytes] {
                    self.device.write_sectors(lba, &first[..bytes])?;
                }
            }
            sector += count;
        }
        Ok(())
    }

    /// Counts free clusters by reading the active FAT directly in large chunks.
    pub(crate) fn count_free_clusters(&mut self) -> Result<u32> {
        self.flush_fat()?;
        let max = self.geo.max_cluster();
        let last_sector = max / ENTRIES_PER_SECTOR;
        let mut free = 0u32;
        let mut buffer = alloc::vec![0u8; SCAN_CHUNK_SECTORS as usize * SECTOR_SIZE];
        let mut sector = 0u32;
        while sector <= last_sector {
            let count = SCAN_CHUNK_SECTORS.min(last_sector + 1 - sector);
            let bytes = &mut buffer[..count as usize * SECTOR_SIZE];
            self.device.read_sectors(self.geo.fat_lba(self.geo.active_fat) + sector as u64, bytes)?;
            for index in 0..count * ENTRIES_PER_SECTOR {
                let cluster = sector * ENTRIES_PER_SECTOR + index;
                if cluster < 2 || cluster > max {
                    continue;
                }
                if le32(bytes, index as usize * 4) & FAT_MASK == 0 {
                    free += 1;
                }
            }
            sector += count;
        }
        Ok(free)
    }
}
