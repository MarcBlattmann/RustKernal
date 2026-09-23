//! Write-back sector cache. PUBLIC API CONTRACT: keep these signatures.

use crate::{BlockDevice, DeviceError};

/// Caches up to `capacity` sectors in memory with LRU eviction. Writes stay in memory until the
/// sector is evicted or `flush` is called; `flush` writes all dirty sectors (in ascending LBA
/// order) and then flushes the inner device.
pub struct CachedDevice<D: BlockDevice> {
    device: D,
    // implementation state
}

impl<D: BlockDevice> CachedDevice<D> {
    pub fn new(device: D, capacity: usize) -> Self {
        todo!()
    }

    pub fn inner(&self) -> &D {
        &self.device
    }

    /// Flushes and returns the inner device.
    pub fn into_inner(self) -> core::result::Result<D, DeviceError> {
        todo!()
    }
}

impl<D: BlockDevice> BlockDevice for CachedDevice<D> {
    fn sector_count(&self) -> u64 {
        self.device.sector_count()
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> Result<(), DeviceError> {
        todo!()
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> Result<(), DeviceError> {
        todo!()
    }

    fn flush(&mut self) -> Result<(), DeviceError> {
        todo!()
    }
}
