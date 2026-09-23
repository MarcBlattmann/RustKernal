//! MBR partition tables. PUBLIC API CONTRACT: keep these signatures.

use crate::boot_sector::{le16, le32, put32};
use crate::{BlockDevice, DeviceError, Error, Result, SECTOR_SIZE};
use alloc::vec::Vec;

/// Partition type for FAT32 with LBA addressing.
pub const PARTITION_TYPE_FAT32_LBA: u8 = 0x0C;

/// First sector of the partition created by [`write_single_partition`] (1 MiB alignment).
pub const PARTITION_ALIGNMENT: u64 = 2048;

const TABLE_OFFSET: usize = 0x1BE;
const ENTRY_SIZE: usize = 16;
const DISK_SIGNATURE_OFFSET: usize = 0x1B8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Partition {
    /// 0..=3, position in the MBR table.
    pub index: u8,
    pub partition_type: u8,
    pub bootable: bool,
    pub start_lba: u64,
    pub sector_count: u64,
}

/// Whether sector 0 is a FAT boot sector (a partitionless "superfloppy") rather than an MBR.
fn is_fat_boot_sector(sector: &[u8; SECTOR_SIZE]) -> bool {
    let jump = (sector[0] == 0xEB && sector[2] == 0x90) || sector[0] == 0xE9;
    let bytes_per_sector = le16(sector, 11);
    let sectors_per_cluster = sector[13];
    let reserved = le16(sector, 14);
    let fats = sector[16];
    let media = sector[21];
    let bpb_ok = matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096)
        && sectors_per_cluster != 0
        && sectors_per_cluster.is_power_of_two()
        && reserved != 0
        && (1..=4).contains(&fats)
        && (media == 0xF0 || media >= 0xF8);
    let fs_type = &sector[0x52..0x57] == b"FAT32" || &sector[0x36..0x39] == b"FAT";
    jump && bpb_ok && fs_type
}

/// Reads the primary partitions. Returns an empty list when sector 0 has no valid MBR signature
/// or when it is a FAT boot sector rather than a partition table (a "superfloppy").
pub fn read_partitions<D: BlockDevice>(device: &mut D) -> Result<Vec<Partition>> {
    let mut sector = [0u8; SECTOR_SIZE];
    device.read_sectors(0, &mut sector)?;
    let mut partitions = Vec::new();
    if sector[510] != 0x55 || sector[511] != 0xAA || is_fat_boot_sector(&sector) {
        return Ok(partitions);
    }
    for index in 0..4 {
        let entry = &sector[TABLE_OFFSET + index * ENTRY_SIZE..TABLE_OFFSET + (index + 1) * ENTRY_SIZE];
        let status = entry[0];
        if status != 0x00 && status != 0x80 {
            // Not a partition table after all.
            return Ok(Vec::new());
        }
        let partition_type = entry[4];
        let start_lba = le32(entry, 8) as u64;
        let sector_count = le32(entry, 12) as u64;
        if partition_type == 0 || sector_count == 0 || start_lba == 0 {
            continue;
        }
        partitions.push(Partition {
            index: index as u8,
            partition_type,
            bootable: status == 0x80,
            start_lba,
            sector_count,
        });
    }
    Ok(partitions)
}

/// CHS address of `lba` for the classic 255-head, 63-sector geometry, or the "use LBA" marker
/// (0xFE, 0xFF, 0xFF) when it is beyond cylinder 1023.
fn chs(lba: u64) -> [u8; 3] {
    const HEADS: u64 = 255;
    const SECTORS: u64 = 63;
    let cylinder = lba / (HEADS * SECTORS);
    if cylinder > 1023 {
        return [0xFE, 0xFF, 0xFF];
    }
    let head = (lba / SECTORS) % HEADS;
    let sector = lba % SECTORS + 1;
    [head as u8, (sector as u8) | (((cylinder >> 8) as u8 & 0x03) << 6), cylinder as u8]
}

/// Writes a fresh MBR with one non-bootable partition of `partition_type` covering the disk from
/// sector 2048 (1 MiB aligned) to the end, and returns it.
pub fn write_single_partition<D: BlockDevice>(device: &mut D, partition_type: u8) -> Result<Partition> {
    let total = device.sector_count();
    if total <= PARTITION_ALIGNMENT {
        return Err(Error::Unsupported("disk too small for a partition table"));
    }
    if partition_type == 0 {
        return Err(Error::Unsupported("partition type 0 marks an unused entry"));
    }
    let sector_count = (total - PARTITION_ALIGNMENT).min(u32::MAX as u64);
    let last = PARTITION_ALIGNMENT + sector_count - 1;
    let mut mbr = [0u8; SECTOR_SIZE];
    // Disk signature derived from the size (stable, non-zero).
    put32(&mut mbr, DISK_SIGNATURE_OFFSET, (0x5253_0000 ^ total as u32) | 1);
    let entry = &mut mbr[TABLE_OFFSET..TABLE_OFFSET + ENTRY_SIZE];
    entry[0] = 0x00;
    entry[1..4].copy_from_slice(&chs(PARTITION_ALIGNMENT));
    entry[4] = partition_type;
    entry[5..8].copy_from_slice(&chs(last));
    put32(entry, 8, PARTITION_ALIGNMENT as u32);
    put32(entry, 12, sector_count as u32);
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    device.write_sectors(0, &mbr)?;
    device.flush()?;
    Ok(Partition { index: 0, partition_type, bootable: false, start_lba: PARTITION_ALIGNMENT, sector_count })
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

    /// Translates a partition-relative request into a device LBA after bounds checking.
    fn translate(&self, lba: u64, length: usize) -> core::result::Result<u64, DeviceError> {
        if length == 0 || !length.is_multiple_of(SECTOR_SIZE) {
            return Err(DeviceError::OutOfRange);
        }
        let count = (length / SECTOR_SIZE) as u64;
        match lba.checked_add(count) {
            Some(end) if end <= self.sector_count => self.start_lba.checked_add(lba).ok_or(DeviceError::OutOfRange),
            _ => Err(DeviceError::OutOfRange),
        }
    }
}

impl<D: BlockDevice> BlockDevice for PartitionDevice<D> {
    fn sector_count(&self) -> u64 {
        self.sector_count
    }

    fn read_sectors(&mut self, lba: u64, buffer: &mut [u8]) -> core::result::Result<(), DeviceError> {
        let lba = self.translate(lba, buffer.len())?;
        self.device.read_sectors(lba, buffer)
    }

    fn write_sectors(&mut self, lba: u64, buffer: &[u8]) -> core::result::Result<(), DeviceError> {
        let lba = self.translate(lba, buffer.len())?;
        self.device.write_sectors(lba, buffer)
    }

    fn flush(&mut self) -> core::result::Result<(), DeviceError> {
        self.device.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chs_encoding() {
        assert_eq!(chs(0), [0, 1, 0]);
        assert_eq!(chs(2048), [32, 33, 0]);
        assert_eq!(chs(1023 * 255 * 63 + 254 * 63 + 62), [254, 0xFF, 0xFF]);
        assert_eq!(chs(1024 * 255 * 63), [0xFE, 0xFF, 0xFF]);
    }
}
