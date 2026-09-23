//! MBR partition tables. PUBLIC API CONTRACT: keep these signatures.

use crate::{BlockDevice, DeviceError, Result};
use alloc::vec::Vec;

/// Partition type for FAT32 with LBA addressing.
pub const PARTITION_TYPE_FAT32_LBA: u8 = 0x0C;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Partition {
    /// 0..=3, position in the MBR table.
    pub index: u8,
    pub partition_type: u8,
    pub bootable: bool,
    pub start_lba: u64,
    pub sector_count: u64,
}

/// Reads the primary partitions. Returns an empty list when sector 0 has no valid MBR signature
/// or when it is a FAT boot sector rather than a partition table (a "superfloppy").
pub fn read_partitions<D: BlockDevice>(device: &mut D) -> Result<Vec<Partition>> {
    todo!()
}

/// Writes a fresh MBR with one non-bootable partition of `partition_type` covering the disk from
/// sector 2048 (1 MiB aligned) to the end, and returns it.
pub fn write_single_partition<D: BlockDevice>(device: &mut D, partition_type: u8) -> Result<Partition> {
    todo!()
}

/// A window onto one partition of a device, with sector 0 = the partition's first sector.
pub struct PartitionDevice<D: BlockDevice> {
    device: D,
    start_lba: u64,
    sector_count: u64,
}

impl<D: BlockDevice> PartitionDevice<D> {
    pub fn new(device: D, partition: &Partition) -> Self {
        Self { device, start_lba: partition.start_lba, sector_count: partition.sector_count }
    }

    pub fn inner(&self) -> &D {
        &self.device
    }

    pub fn into_inner(self) -> D {
        self.device
    }
}

impl<D: BlockDevice> BlockDevice for PartitionDevice<D> {
    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> core::result::Result<(), DeviceError> {
        todo!()
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> core::result::Result<(), DeviceError> {
        todo!()
    }

    fn flush(&mut self) -> core::result::Result<(), DeviceError> {
        self.device.flush()
    }
}
