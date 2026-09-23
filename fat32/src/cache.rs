//! Write-back sector cache. PUBLIC API CONTRACT: keep these signatures.

use crate::{BlockDevice, DeviceError, SECTOR_SIZE};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

const NIL: usize = usize::MAX;
/// Requests longer than this many sectors bypass the cache (they are usually file data that
/// would only evict the metadata the cache is good for). Writes still update cached copies.
const MAX_BYPASS_THRESHOLD: usize = 64;
/// Longest single request `flush` issues when writing back consecutive dirty sectors.
const MAX_FLUSH_RUN: usize = 128;

struct Slot {
    lba: u64,
    data: Box<[u8; SECTOR_SIZE]>,
    dirty: bool,
    /// Towards the most recently used end.
    prev: usize,
    /// Towards the least recently used end.
    next: usize,
}

/// Hit/miss counters of a [`CachedDevice`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    /// Sectors transferred by requests that bypassed the cache.
    pub bypassed: u64,
    pub write_backs: u64,
}

/// Caches up to `capacity` sectors in memory with LRU eviction. Writes stay in memory until the
/// sector is evicted or `flush` is called; `flush` writes all dirty sectors (in ascending LBA
/// order) and then flushes the inner device.
pub struct CachedDevice<D: BlockDevice> {
    device: D,
    capacity: usize,
    slots: Vec<Slot>,
    /// LBA -> slot index.
    index: BTreeMap<u64, usize>,
    /// Most recently used slot.
    head: usize,
    /// Least recently used slot.
    tail: usize,
    stats: CacheStats,
}

impl<D: BlockDevice> CachedDevice<D> {
    pub fn new(device: D, capacity: usize) -> Self {
        Self {
            device,
            capacity,
            slots: Vec::new(),
            index: BTreeMap::new(),
            head: NIL,
            tail: NIL,
            stats: CacheStats::default(),
        }
    }

    pub fn inner(&self) -> &D {
        &self.device
    }

    /// Flushes and returns the inner device.
    pub fn into_inner(mut self) -> core::result::Result<D, DeviceError> {
        self.flush()?;
        Ok(self.device)
    }

    /// Maximum number of cached sectors.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of sectors currently cached.
    pub fn cached_sectors(&self) -> usize {
        self.index.len()
    }

    /// Number of cached sectors not yet written to the device.
    pub fn dirty_sectors(&self) -> usize {
        self.slots.iter().filter(|slot| slot.dirty).count()
    }

    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    fn bypass_threshold(&self) -> usize {
        (self.capacity / 2).clamp(1, MAX_BYPASS_THRESHOLD)
    }

    fn check_range(&self, lba: u64, length: usize) -> Result<u64, DeviceError> {
        if length == 0 || !length.is_multiple_of(SECTOR_SIZE) {
            return Err(DeviceError::OutOfRange);
        }
        let count = (length / SECTOR_SIZE) as u64;
        match lba.checked_add(count) {
            Some(end) if end <= self.device.sector_count() => Ok(count),
            _ => Err(DeviceError::OutOfRange),
        }
    }

    fn unlink(&mut self, slot: usize) {
        let (prev, next) = (self.slots[slot].prev, self.slots[slot].next);
        if prev != NIL {
            self.slots[prev].next = next;
        } else {
            self.head = next;
        }
        if next != NIL {
            self.slots[next].prev = prev;
        } else {
            self.tail = prev;
        }
        self.slots[slot].prev = NIL;
        self.slots[slot].next = NIL;
    }

    fn push_front(&mut self, slot: usize) {
        self.slots[slot].prev = NIL;
        self.slots[slot].next = self.head;
        if self.head != NIL {
            self.slots[self.head].prev = slot;
        }
        self.head = slot;
        if self.tail == NIL {
            self.tail = slot;
        }
    }

    fn touch(&mut self, slot: usize) {
        if self.head != slot {
            self.unlink(slot);
            self.push_front(slot);
        }
    }

    /// Stores a sector in the cache (as most recently used), evicting the least recently used
    /// sector if the cache is full. A dirty victim is written back first; if that fails the
    /// victim stays cached and the error is returned.
    fn insert(&mut self, lba: u64, data: &[u8], dirty: bool) -> Result<(), DeviceError> {
        if let Some(&slot) = self.index.get(&lba) {
            self.slots[slot].data.copy_from_slice(data);
            self.slots[slot].dirty |= dirty;
            self.touch(slot);
            return Ok(());
        }
        let slot = if self.slots.len() < self.capacity {
            let mut buffer = Box::new([0u8; SECTOR_SIZE]);
            buffer.copy_from_slice(data);
            self.slots.push(Slot { lba, data: buffer, dirty, prev: NIL, next: NIL });
            self.slots.len() - 1
        } else {
            let victim = self.tail;
            if self.slots[victim].dirty {
                let victim_lba = self.slots[victim].lba;
                self.device.write_sectors(victim_lba, &self.slots[victim].data[..])?;
                self.stats.write_backs += 1;
                self.slots[victim].dirty = false;
            }
            self.unlink(victim);
            self.index.remove(&self.slots[victim].lba);
            let entry = &mut self.slots[victim];
            entry.lba = lba;
            entry.data.copy_from_slice(data);
            entry.dirty = dirty;
            victim
        };
        self.index.insert(lba, slot);
        self.push_front(slot);
        Ok(())
    }
}

impl<D: BlockDevice> BlockDevice for CachedDevice<D> {
    fn sector_count(&self) -> u64 {
        self.device.sector_count()
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        let count = self.check_range(lba, buffer.len())? as usize;
        if self.capacity == 0 {
            return self.device.read_sectors(lba, buffer);
        }
        if count > self.bypass_threshold() {
            // Large read: one device request, then overlay cached (possibly dirty) sectors.
            self.device.read_sectors(lba, buffer)?;
            self.stats.bypassed += count as u64;
            let end = lba + count as u64;
            for (&cached, &slot) in self.index.range(lba..end) {
                let offset = (cached - lba) as usize * SECTOR_SIZE;
                buffer[offset..offset + SECTOR_SIZE].copy_from_slice(&self.slots[slot].data[..]);
            }
            return Ok(());
        }
        let mut i = 0;
        while i < count {
            let sector = lba + i as u64;
            if let Some(&slot) = self.index.get(&sector) {
                buffer[i * SECTOR_SIZE..(i + 1) * SECTOR_SIZE].copy_from_slice(&self.slots[slot].data[..]);
                self.touch(slot);
                self.stats.hits += 1;
                i += 1;
                continue;
            }
            // Read the whole run of uncached sectors with one request.
            let mut run = 1;
            while i + run < count && !self.index.contains_key(&(sector + run as u64)) {
                run += 1;
            }
            let bytes = &mut buffer[i * SECTOR_SIZE..(i + run) * SECTOR_SIZE];
            self.device.read_sectors(sector, bytes)?;
            self.stats.misses += run as u64;
            for j in 0..run {
                let start = (i + j) * SECTOR_SIZE;
                let mut data = [0u8; SECTOR_SIZE];
                data.copy_from_slice(&buffer[start..start + SECTOR_SIZE]);
                self.insert(sector + j as u64, &data, false)?;
            }
            i += run;
        }
        Ok(())
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        let count = self.check_range(lba, buffer.len())? as usize;
        if self.capacity == 0 {
            return self.device.write_sectors(lba, buffer);
        }
        if count > self.bypass_threshold() {
            // Large write: straight to the device; cached copies become clean and current.
            self.device.write_sectors(lba, buffer)?;
            self.stats.bypassed += count as u64;
            let end = lba + count as u64;
            let cached: Vec<(u64, usize)> = self.index.range(lba..end).map(|(&l, &s)| (l, s)).collect();
            for (sector, slot) in cached {
                let offset = (sector - lba) as usize * SECTOR_SIZE;
                self.slots[slot].data.copy_from_slice(&buffer[offset..offset + SECTOR_SIZE]);
                self.slots[slot].dirty = false;
            }
            return Ok(());
        }
        for i in 0..count {
            self.insert(lba + i as u64, &buffer[i * SECTOR_SIZE..(i + 1) * SECTOR_SIZE], true)?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), DeviceError> {
        let dirty: Vec<(u64, usize)> =
            self.index.iter().filter(|&(_, &slot)| self.slots[slot].dirty).map(|(&l, &s)| (l, s)).collect();
        let mut buffer = Vec::new();
        let mut start = 0;
        while start < dirty.len() {
            let mut end = start + 1;
            while end < dirty.len() && end - start < MAX_FLUSH_RUN && dirty[end].0 == dirty[end - 1].0 + 1 {
                end += 1;
            }
            buffer.clear();
            for &(_, slot) in &dirty[start..end] {
                buffer.extend_from_slice(&self.slots[slot].data[..]);
            }
            self.device.write_sectors(dirty[start].0, &buffer)?;
            self.stats.write_backs += (end - start) as u64;
            for &(_, slot) in &dirty[start..end] {
                self.slots[slot].dirty = false;
            }
            start = end;
        }
        self.device.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAM disk that records every request.
    struct Recorder {
        data: Vec<u8>,
        log: Vec<(char, u64, usize)>,
        flushes: usize,
    }

    impl Recorder {
        fn new(sectors: usize) -> Self {
            let mut data = vec![0u8; sectors * SECTOR_SIZE];
            for (i, byte) in data.iter_mut().enumerate() {
                *byte = (i / SECTOR_SIZE) as u8;
            }
            Self { data, log: Vec::new(), flushes: 0 }
        }
    }

    impl BlockDevice for Recorder {
        fn sector_count(&self) -> u64 {
            (self.data.len() / SECTOR_SIZE) as u64
        }
        fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
            let start = lba as usize * SECTOR_SIZE;
            buffer.copy_from_slice(&self.data[start..start + buffer.len()]);
            self.log.push(('r', lba, buffer.len() / SECTOR_SIZE));
            Ok(())
        }
        fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
            let start = lba as usize * SECTOR_SIZE;
            self.data[start..start + buffer.len()].copy_from_slice(buffer);
            self.log.push(('w', lba, buffer.len() / SECTOR_SIZE));
            Ok(())
        }
        fn flush(&mut self) -> Result<(), DeviceError> {
            self.flushes += 1;
            Ok(())
        }
    }

    fn sector(value: u8) -> Vec<u8> {
        vec![value; SECTOR_SIZE]
    }

    #[test]
    fn read_hits_after_miss() {
        let mut cache = CachedDevice::new(Recorder::new(64), 8);
        let mut buffer = sector(0);
        cache.read_sectors(3, &mut buffer).unwrap();
        assert_eq!(buffer, sector(3));
        cache.read_sectors(3, &mut buffer).unwrap();
        assert_eq!(cache.inner().log.len(), 1);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 1);
    }

    #[test]
    fn writes_are_deferred_and_flushed_in_order() {
        let mut cache = CachedDevice::new(Recorder::new(64), 16);
        for lba in [9u64, 3, 4, 5, 20] {
            cache.write_sectors(lba, &sector(0xA0 + lba as u8)).unwrap();
        }
        assert!(cache.inner().log.is_empty());
        assert_eq!(cache.dirty_sectors(), 5);
        let mut buffer = sector(0);
        cache.read_sectors(4, &mut buffer).unwrap();
        assert_eq!(buffer, sector(0xA4));
        cache.flush().unwrap();
        assert_eq!(cache.inner().log, vec![('w', 3, 3), ('w', 9, 1), ('w', 20, 1)]);
        assert_eq!(cache.inner().flushes, 1);
        assert_eq!(cache.dirty_sectors(), 0);
        let device = cache.into_inner().unwrap();
        assert_eq!(&device.data[4 * SECTOR_SIZE..5 * SECTOR_SIZE], &sector(0xA4)[..]);
    }

    #[test]
    fn lru_eviction_writes_back_dirty_victim() {
        let mut cache = CachedDevice::new(Recorder::new(64), 2);
        cache.write_sectors(1, &sector(0x11)).unwrap();
        let mut buffer = sector(0);
        cache.read_sectors(2, &mut buffer).unwrap();
        // Touch 1 so that 2 becomes the LRU victim.
        cache.read_sectors(1, &mut buffer).unwrap();
        cache.read_sectors(3, &mut buffer).unwrap();
        assert_eq!(cache.cached_sectors(), 2);
        assert!(!cache.inner().log.contains(&('w', 1, 1)));
        // Now 1 is the LRU and dirty: evicting it writes it back.
        cache.read_sectors(4, &mut buffer).unwrap();
        assert!(cache.inner().log.contains(&('w', 1, 1)));
        assert_eq!(&cache.inner().data[SECTOR_SIZE..2 * SECTOR_SIZE], &sector(0x11)[..]);
    }

    #[test]
    fn large_requests_bypass_but_stay_coherent() {
        let mut cache = CachedDevice::new(Recorder::new(256), 8);
        cache.write_sectors(10, &sector(0xEE)).unwrap();
        let mut big = vec![0u8; 32 * SECTOR_SIZE];
        cache.read_sectors(0, &mut big).unwrap();
        assert_eq!(&big[10 * SECTOR_SIZE..11 * SECTOR_SIZE], &sector(0xEE)[..]);
        assert_eq!(&big[11 * SECTOR_SIZE..12 * SECTOR_SIZE], &sector(11)[..]);
        assert_eq!(cache.cached_sectors(), 1);
        // A large write over the cached sector replaces it and leaves it clean.
        let payload = vec![0x55u8; 32 * SECTOR_SIZE];
        cache.write_sectors(0, &payload).unwrap();
        assert_eq!(cache.dirty_sectors(), 0);
        let mut buffer = sector(0);
        cache.read_sectors(10, &mut buffer).unwrap();
        assert_eq!(buffer, sector(0x55));
    }

    #[test]
    fn mixed_run_reads_uncached_sectors_in_one_request() {
        let mut cache = CachedDevice::new(Recorder::new(64), 32);
        cache.write_sectors(5, &sector(0x77)).unwrap();
        let mut buffer = vec![0u8; 8 * SECTOR_SIZE];
        cache.read_sectors(2, &mut buffer).unwrap();
        assert_eq!(cache.inner().log, vec![('r', 2, 3), ('r', 6, 4)]);
        assert_eq!(&buffer[3 * SECTOR_SIZE..4 * SECTOR_SIZE], &sector(0x77)[..]);
    }

    #[test]
    fn range_checks() {
        let mut cache = CachedDevice::new(Recorder::new(16), 4);
        let mut buffer = sector(0);
        assert_eq!(cache.read_sectors(16, &mut buffer), Err(DeviceError::OutOfRange));
        assert_eq!(cache.write_sectors(u64::MAX, &buffer), Err(DeviceError::OutOfRange));
        assert_eq!(cache.read_sectors(0, &mut []), Err(DeviceError::OutOfRange));
        assert_eq!(cache.read_sectors(0, &mut [0u8; 100]), Err(DeviceError::OutOfRange));
    }

    #[test]
    fn zero_capacity_passes_through() {
        let mut cache = CachedDevice::new(Recorder::new(16), 0);
        cache.write_sectors(1, &sector(9)).unwrap();
        assert_eq!(cache.inner().log, vec![('w', 1, 1)]);
        let mut buffer = sector(0);
        cache.read_sectors(1, &mut buffer).unwrap();
        assert_eq!(buffer, sector(9));
    }
}
