//! Creating a FAT32 volume (`mkfs`).
//!
//! Layout: 32 reserved sectors (boot sector at 0, FSInfo at 1, backups at 6 and 7), two FATs,
//! then the data area with the root directory at cluster 2. The boot sector is written last so
//! an interrupted format never leaves a mountable half-initialized volume.

use crate::boot_sector::{
    FSINFO_LEAD_SIG, FSINFO_STRUCT_SIG, FSINFO_TRAIL_SIG, MAX_CLUSTERS, MIN_CLUSTERS, put16, put32,
};
use crate::dir::{ATTR_VOLUME_ID, ENTRY_SIZE, short_entry};
use crate::fat::{CLEAN_SHUTDOWN, EOC, NO_HARD_ERROR};
use crate::name::label_bytes;
use crate::time;
use crate::{BlockDevice, Error, FormatOptions, Result, SECTOR_SIZE};
use alloc::vec;

pub(crate) const RESERVED_SECTORS: u32 = 32;
pub(crate) const NUM_FATS: u32 = 2;
pub(crate) const MEDIA_FIXED_DISK: u8 = 0xF8;
const FSINFO_SECTOR: u32 = 1;
const BACKUP_BOOT_SECTOR: u32 = 6;
const ROOT_CLUSTER: u32 = 2;
/// Sectors per request when zeroing the FATs.
const ZERO_CHUNK_SECTORS: u32 = 128;

/// Cluster size (in sectors) Microsoft's format tool uses for a FAT32 volume of `sectors`.
fn default_sectors_per_cluster(sectors: u32) -> u32 {
    match sectors {
        0..=532_480 => 1,              // up to 260 MiB: 512 bytes
        532_481..=16_777_216 => 8,     // up to 8 GiB: 4 KiB
        16_777_217..=33_554_432 => 16, // up to 16 GiB: 8 KiB
        33_554_433..=67_108_864 => 32, // up to 32 GiB: 16 KiB
        _ => 64,                       // larger: 32 KiB
    }
}

/// FAT size in sectors (fatgen103 formula; slightly over-estimates, never under-estimates).
fn fat_size(total: u32, sectors_per_cluster: u32) -> u32 {
    let tmp1 = total as u64 - RESERVED_SECTORS as u64;
    let tmp2 = (256 * sectors_per_cluster as u64 + NUM_FATS as u64) / 2;
    tmp1.div_ceil(tmp2) as u32
}

/// `(sectors_per_cluster, fat_size, cluster_count)` for a volume of `total` sectors.
pub(crate) fn layout(total: u32, requested: Option<u8>) -> Result<(u32, u32, u32)> {
    let mut sectors_per_cluster = match requested {
        Some(n) if n == 0 || !n.is_power_of_two() || n > 128 => {
            return Err(Error::Unsupported("sectors per cluster must be a power of two up to 128"));
        }
        Some(n) => n as u32,
        None => default_sectors_per_cluster(total),
    };
    loop {
        if total <= RESERVED_SECTORS + NUM_FATS {
            return Err(Error::Unsupported("volume too small for FAT32"));
        }
        let mut fat = fat_size(total, sectors_per_cluster);
        let clusters = loop {
            let overhead = RESERVED_SECTORS as u64 + NUM_FATS as u64 * fat as u64;
            if overhead >= total as u64 {
                break 0;
            }
            let clusters = ((total as u64 - overhead) / sectors_per_cluster as u64) as u32;
            if fat as u64 * (SECTOR_SIZE as u64 / 4) >= clusters as u64 + 2 {
                break clusters;
            }
            fat += 1;
        };
        if clusters < MIN_CLUSTERS {
            if requested.is_none() && sectors_per_cluster > 1 {
                sectors_per_cluster /= 2;
                continue;
            }
            return Err(Error::Unsupported("volume too small for FAT32 (needs at least 65525 clusters)"));
        }
        if clusters > MAX_CLUSTERS {
            if requested.is_none() && sectors_per_cluster < 128 {
                sectors_per_cluster *= 2;
                continue;
            }
            return Err(Error::Unsupported("too many clusters for FAT32"));
        }
        return Ok((sectors_per_cluster, fat, clusters));
    }
}

pub(crate) fn format<D: BlockDevice>(device: &mut D, options: &FormatOptions) -> Result<()> {
    let label = if options.label.is_empty() { *b"NO NAME    " } else { label_bytes(&options.label)? };
    let total = device.sector_count().min(u32::MAX as u64) as u32;
    let (sectors_per_cluster, fat_sectors, clusters) = layout(total, options.sectors_per_cluster)?;
    let cluster_bytes = sectors_per_cluster as usize * SECTOR_SIZE;

    // Invalidate any previous boot sector first, then clear the reserved area.
    let zeros = vec![0u8; ZERO_CHUNK_SECTORS as usize * SECTOR_SIZE];
    device.write_sectors(0, &zeros[..RESERVED_SECTORS as usize * SECTOR_SIZE])?;

    // FATs: zero, then the first sector with the reserved entries and the root directory.
    let mut first_fat_sector = [0u8; SECTOR_SIZE];
    put32(&mut first_fat_sector, 0, 0x0FFF_FF00 | MEDIA_FIXED_DISK as u32);
    put32(&mut first_fat_sector, 4, EOC | CLEAN_SHUTDOWN | NO_HARD_ERROR);
    put32(&mut first_fat_sector, 8, EOC);
    for copy in 0..NUM_FATS {
        let start = RESERVED_SECTORS as u64 + copy as u64 * fat_sectors as u64;
        let mut done = 0u32;
        while done < fat_sectors {
            let count = ZERO_CHUNK_SECTORS.min(fat_sectors - done);
            device.write_sectors(start + done as u64, &zeros[..count as usize * SECTOR_SIZE])?;
            done += count;
        }
        device.write_sectors(start, &first_fat_sector)?;
    }

    // Root directory: one zeroed cluster holding the volume label entry.
    let first_data_sector = RESERVED_SECTORS as u64 + NUM_FATS as u64 * fat_sectors as u64;
    let mut root = vec![0u8; cluster_bytes];
    if !options.label.is_empty() {
        let now = time::encode(time::default_now());
        root[..ENTRY_SIZE].copy_from_slice(&short_entry(label, ATTR_VOLUME_ID, 0, 0, now, now));
    }
    let root_lba = first_data_sector + (ROOT_CLUSTER as u64 - 2) * sectors_per_cluster as u64;
    device.write_sectors(root_lba, &root)?;

    // FSInfo (+ backup).
    let mut fsinfo = [0u8; SECTOR_SIZE];
    put32(&mut fsinfo, 0, FSINFO_LEAD_SIG);
    put32(&mut fsinfo, 484, FSINFO_STRUCT_SIG);
    put32(&mut fsinfo, 488, clusters - 1);
    put32(&mut fsinfo, 492, ROOT_CLUSTER + 1);
    put32(&mut fsinfo, 508, FSINFO_TRAIL_SIG);
    device.write_sectors(FSINFO_SECTOR as u64, &fsinfo)?;
    device.write_sectors((BACKUP_BOOT_SECTOR + FSINFO_SECTOR) as u64, &fsinfo)?;

    // Boot sector (+ backup), last.
    let boot = boot_sector(total, sectors_per_cluster, fat_sectors, options.volume_id, &label);
    device.write_sectors(BACKUP_BOOT_SECTOR as u64, &boot)?;
    device.flush()?;
    device.write_sectors(0, &boot)?;
    device.flush()?;
    Ok(())
}

fn boot_sector(
    total: u32,
    sectors_per_cluster: u32,
    fat_sectors: u32,
    volume_id: u32,
    label: &[u8; 11],
) -> [u8; SECTOR_SIZE] {
    let mut boot = [0u8; SECTOR_SIZE];
    boot[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
    boot[3..11].copy_from_slice(b"MSWIN4.1");
    put16(&mut boot, 11, SECTOR_SIZE as u16);
    boot[13] = sectors_per_cluster as u8;
    put16(&mut boot, 14, RESERVED_SECTORS as u16);
    boot[16] = NUM_FATS as u8;
    put16(&mut boot, 17, 0); // root entries (FAT12/16 only)
    put16(&mut boot, 19, 0); // total sectors 16
    boot[21] = MEDIA_FIXED_DISK;
    put16(&mut boot, 22, 0); // FAT size 16
    put16(&mut boot, 24, 63); // sectors per track
    put16(&mut boot, 26, 255); // heads
    put32(&mut boot, 28, 0); // hidden sectors (callers use PartitionDevice)
    put32(&mut boot, 32, total);
    put32(&mut boot, 36, fat_sectors);
    put16(&mut boot, 40, 0); // ext flags: mirroring on
    put16(&mut boot, 42, 0); // version 0.0
    put32(&mut boot, 44, ROOT_CLUSTER);
    put16(&mut boot, 48, FSINFO_SECTOR as u16);
    put16(&mut boot, 50, BACKUP_BOOT_SECTOR as u16);
    boot[64] = 0x80; // drive number
    boot[66] = 0x29; // extended boot signature
    put32(&mut boot, 67, volume_id);
    boot[71..82].copy_from_slice(label);
    boot[82..90].copy_from_slice(b"FAT32   ");
    // Boot code: print nothing, ask the BIOS to try the next device (int 0x18), then halt.
    boot[90..95].copy_from_slice(&[0xCD, 0x18, 0xF4, 0xEB, 0xFD]);
    boot[510] = 0x55;
    boot[511] = 0xAA;
    boot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_volume() {
        assert!(layout(60_000, None).is_err());
        // Smallest volumes need 512-byte clusters.
        let (spc, fat, clusters) = layout(70_000, None).unwrap();
        assert_eq!(spc, 1);
        assert!(clusters >= MIN_CLUSTERS);
        assert!(fat as u64 * 128 >= clusters as u64 + 2);
        assert!(RESERVED_SECTORS + 2 * fat + clusters * spc <= 70_000);
    }

    #[test]
    fn microsoft_table_with_fallback() {
        // 1 GiB: 4 KiB clusters.
        assert_eq!(layout(2_097_152, None).unwrap().0, 8);
        // 300 MiB: 4 KiB clusters, still enough of them.
        let (spc, _, clusters) = layout(614_400, None).unwrap();
        assert_eq!(spc, 8);
        assert!(clusters >= MIN_CLUSTERS);
        // 100 MiB: 512-byte clusters.
        assert_eq!(layout(204_800, None).unwrap().0, 1);
        // 64 GiB: 32 KiB clusters.
        assert_eq!(layout(134_217_728, None).unwrap().0, 64);
    }

    #[test]
    fn explicit_cluster_size() {
        assert!(layout(2_097_152, Some(3)).is_err());
        assert!(layout(2_097_152, Some(0)).is_err());
        // 1 GiB with 32 KiB clusters has only ~32K clusters: not FAT32.
        assert!(matches!(layout(2_097_152, Some(64)), Err(Error::Unsupported(_))));
        assert_eq!(layout(2_097_152, Some(1)).unwrap().0, 1);
    }

    #[test]
    fn fat_always_covers_clusters() {
        for total in (66_000u32..3_000_000).step_by(7_919) {
            if let Ok((spc, fat, clusters)) = layout(total, None) {
                assert!(fat as u64 * 128 >= clusters as u64 + 2, "total {}", total);
                assert!(RESERVED_SECTORS as u64 + 2 * fat as u64 + clusters as u64 * spc as u64 <= total as u64);
            }
        }
    }
}
