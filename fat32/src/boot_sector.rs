//! Boot sector (BPB + FAT32 extended BPB) and FSInfo parsing and validation.

use crate::{Error, Result, SECTOR_SIZE};
use alloc::string::String;

pub(crate) const FSINFO_LEAD_SIG: u32 = 0x4161_5252;
pub(crate) const FSINFO_STRUCT_SIG: u32 = 0x6141_7272;
pub(crate) const FSINFO_TRAIL_SIG: u32 = 0xAA55_0000;
/// FSInfo value meaning "unknown".
pub(crate) const FSINFO_UNKNOWN: u32 = 0xFFFF_FFFF;

/// A volume with fewer clusters is FAT12/FAT16 by definition.
pub(crate) const MIN_CLUSTERS: u32 = 65525;
/// Cluster numbers must stay below the reserved values 0x0FFFFFF7..=0x0FFFFFFF.
pub(crate) const MAX_CLUSTERS: u32 = 0x0FFF_FFF5;

pub(crate) fn le16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub(crate) fn le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

pub(crate) fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// Everything the filesystem needs to know about the volume layout.
#[derive(Debug, Clone)]
pub(crate) struct Geometry {
    pub sectors_per_cluster: u32,
    /// Bytes per cluster.
    pub cluster_size: u32,
    pub reserved_sectors: u32,
    pub num_fats: u32,
    /// Sectors per FAT copy.
    pub fat_size: u32,
    pub first_data_sector: u32,
    /// Number of data clusters; valid cluster numbers are `2..=cluster_count + 1`.
    pub cluster_count: u32,
    pub root_cluster: u32,
    pub fsinfo_sector: Option<u32>,
    /// FAT mirroring enabled (all copies are written); otherwise only `active_fat` is used.
    pub mirroring: bool,
    pub active_fat: u32,
    pub media: u8,
    pub volume_id: u32,
    /// Label from the extended BPB, trimmed ("NO NAME" becomes empty).
    pub bpb_label: String,
}

impl Geometry {
    pub fn max_cluster(&self) -> u32 {
        self.cluster_count + 1
    }

    pub fn is_valid_cluster(&self, cluster: u32) -> bool {
        cluster >= 2 && cluster <= self.max_cluster()
    }

    /// First sector of FAT copy `index`.
    pub fn fat_lba(&self, index: u32) -> u64 {
        self.reserved_sectors as u64 + index as u64 * self.fat_size as u64
    }

    /// First sector of data cluster `cluster`.
    pub fn cluster_lba(&self, cluster: u32) -> u64 {
        self.first_data_sector as u64 + (cluster as u64 - 2) * self.sectors_per_cluster as u64
    }

    /// Byte position of data cluster `cluster` on the device.
    pub fn cluster_pos(&self, cluster: u32) -> u64 {
        self.cluster_lba(cluster) * SECTOR_SIZE as u64
    }

    /// Number of clusters needed to hold `bytes` bytes.
    pub fn clusters_for(&self, bytes: u64) -> u64 {
        bytes.div_ceil(self.cluster_size as u64)
    }
}

/// Trims a space-padded 11-byte label; "NO NAME" (the "no label" marker) becomes empty.
pub(crate) fn label_to_string(raw: &[u8]) -> String {
    let mut label = String::new();
    for &byte in raw {
        label.push(crate::name::oem_to_char(byte));
    }
    let trimmed = label.trim_end_matches(' ');
    if trimmed == "NO NAME" { String::new() } else { String::from(trimmed) }
}

/// Validates the boot sector and derives the volume geometry.
pub(crate) fn parse(sector: &[u8; SECTOR_SIZE], device_sectors: u64) -> Result<Geometry> {
    if sector[510] != 0x55 || sector[511] != 0xAA {
        return Err(Error::Corrupt("no FAT boot sector signature"));
    }
    if sector[0] != 0xEB && sector[0] != 0xE9 {
        return Err(Error::Corrupt("not a FAT boot sector"));
    }
    let bytes_per_sector = le16(sector, 11);
    if bytes_per_sector != SECTOR_SIZE as u16 {
        if bytes_per_sector.is_power_of_two() && (512..=4096).contains(&bytes_per_sector) {
            return Err(Error::Unsupported("sector size other than 512 bytes"));
        }
        return Err(Error::Corrupt("invalid bytes per sector"));
    }
    let sectors_per_cluster = sector[13] as u32;
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        return Err(Error::Corrupt("invalid sectors per cluster"));
    }
    let reserved_sectors = le16(sector, 14) as u32;
    if reserved_sectors == 0 {
        return Err(Error::Corrupt("no reserved sectors"));
    }
    let num_fats = sector[16] as u32;
    if num_fats == 0 {
        return Err(Error::Corrupt("no FAT copies"));
    }
    let root_entries = le16(sector, 17);
    let fat_size_16 = le16(sector, 22);
    if root_entries != 0 || fat_size_16 != 0 {
        return Err(Error::Unsupported("not a FAT32 volume (FAT12/FAT16)"));
    }
    let total_16 = le16(sector, 19) as u32;
    let total_32 = le32(sector, 32);
    let total_sectors = if total_16 != 0 { total_16 } else { total_32 };
    if total_sectors == 0 {
        return Err(Error::Corrupt("volume has no sectors"));
    }
    let media = sector[21];
    let fat_size = le32(sector, 36);
    if fat_size == 0 {
        return Err(Error::Corrupt("FAT size is zero"));
    }
    let ext_flags = le16(sector, 40);
    let mirroring = ext_flags & 0x80 == 0;
    let active_fat = (ext_flags & 0x0F) as u32;
    if !mirroring && active_fat >= num_fats {
        return Err(Error::Corrupt("active FAT index out of range"));
    }
    if le16(sector, 42) != 0 {
        return Err(Error::Unsupported("unknown FAT32 version"));
    }
    let root_cluster = le32(sector, 44);
    let fsinfo = le16(sector, 48) as u32;

    if total_sectors as u64 > device_sectors {
        return Err(Error::Corrupt("volume is larger than the device"));
    }
    let first_data_sector = reserved_sectors as u64 + num_fats as u64 * fat_size as u64;
    if first_data_sector >= total_sectors as u64 {
        return Err(Error::Corrupt("volume has no data area"));
    }
    let cluster_count = ((total_sectors as u64 - first_data_sector) / sectors_per_cluster as u64) as u32;
    if cluster_count < MIN_CLUSTERS {
        return Err(Error::Unsupported("not a FAT32 volume (too few clusters)"));
    }
    if cluster_count > MAX_CLUSTERS {
        return Err(Error::Corrupt("too many clusters"));
    }
    if (fat_size as u64) * (SECTOR_SIZE as u64 / 4) < cluster_count as u64 + 2 {
        return Err(Error::Corrupt("FAT is too small for the cluster count"));
    }
    if root_cluster < 2 || root_cluster > cluster_count + 1 {
        return Err(Error::Corrupt("invalid root directory cluster"));
    }
    let fsinfo_sector = match fsinfo {
        0 | 0xFFFF => None,
        n if n < reserved_sectors => Some(n),
        _ => return Err(Error::Corrupt("FSInfo sector outside the reserved area")),
    };
    let (volume_id, bpb_label) =
        if sector[66] == 0x29 { (le32(sector, 67), label_to_string(&sector[71..82])) } else { (0, String::new()) };

    Ok(Geometry {
        sectors_per_cluster,
        cluster_size: sectors_per_cluster * SECTOR_SIZE as u32,
        reserved_sectors,
        num_fats,
        fat_size,
        first_data_sector: first_data_sector as u32,
        cluster_count,
        root_cluster,
        fsinfo_sector,
        mirroring,
        active_fat: if mirroring { 0 } else { active_fat },
        media,
        volume_id,
        bpb_label,
    })
}

/// Validates the FSInfo signatures and returns `(free_count, next_free)`.
pub(crate) fn parse_fsinfo(sector: &[u8; SECTOR_SIZE]) -> Result<(u32, u32)> {
    if le32(sector, 0) != FSINFO_LEAD_SIG
        || le32(sector, 484) != FSINFO_STRUCT_SIG
        || le32(sector, 508) != FSINFO_TRAIL_SIG
    {
        return Err(Error::Corrupt("invalid FSInfo signature"));
    }
    Ok((le32(sector, 488), le32(sector, 492)))
}
