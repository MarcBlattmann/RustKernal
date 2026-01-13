//! Drive Manager - handles multiple disk drives as mount points
//!
//! Drives appear as folders at the root level:
//! /disk0, /disk1, etc.

use spin::Mutex;
use alloc::string::String;
use alloc::vec::Vec;
use super::ata::{ATA_CONTROLLER, DriveLocation, SECTOR_SIZE};

/// Filesystem constants for each drive
const SUPERBLOCK_SECTOR: u32 = 100;
const FAT_START_SECTOR: u32 = 101;
const FAT_SECTORS: u32 = 8;
const DIR_START_SECTOR: u32 = 109;
const DIR_SECTORS: u32 = 8;
const DATA_START_SECTOR: u32 = 117;
const MAX_FILENAME: usize = 32;
const BLOCKS_PER_FILE: usize = 64;
const FS_MAGIC: u32 = 0x5253_4653;

/// Global drive manager
pub static DRIVE_MANAGER: Mutex<DriveManager> = Mutex::new(DriveManager::new());

/// Drive information
#[derive(Clone)]
pub struct MountedDrive {
    pub name: String,           // e.g., "disk0"
    pub location: DriveLocation,
    pub size_mb: u32,
    pub initialized: bool,
    // Cached filesystem data
    superblock: Superblock,
    fat: [u8; FAT_SECTORS as usize * SECTOR_SIZE],
    entries: [DirEntry; 128],
}

/// File entry flags
#[derive(Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum FileType {
    Empty = 0,
    File = 1,
    Directory = 2,
}

/// Directory entry structure (64 bytes each)
#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct DirEntry {
    pub name: [u8; MAX_FILENAME],
    pub file_type: u8,
    pub _reserved: [u8; 3],
    pub size: u32,
    pub first_block: u32,
    pub block_count: u32,
    pub created: u32,
    pub modified: u32,
    pub _padding: [u8; 8],
}

impl DirEntry {
    pub const fn empty() -> Self {
        DirEntry {
            name: [0; MAX_FILENAME],
            file_type: FileType::Empty as u8,
            _reserved: [0; 3],
            size: 0,
            first_block: 0,
            block_count: 0,
            created: 0,
            modified: 0,
            _padding: [0; 8],
        }
    }

    pub fn get_name(&self) -> String {
        let mut name = String::new();
        for &b in &self.name {
            if b == 0 { break; }
            name.push(b as char);
        }
        name
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = [0; MAX_FILENAME];
        for (i, b) in name.bytes().take(MAX_FILENAME - 1).enumerate() {
            self.name[i] = b;
        }
    }

    pub fn is_empty(&self) -> bool { self.file_type == FileType::Empty as u8 }
    pub fn is_file(&self) -> bool { self.file_type == FileType::File as u8 }
    pub fn is_directory(&self) -> bool { self.file_type == FileType::Directory as u8 }
}

/// Superblock structure
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct Superblock {
    magic: u32,
    version: u32,
    total_blocks: u32,
    free_blocks: u32,
    total_entries: u32,
    used_entries: u32,
    block_size: u32,
    _reserved: [u8; 484],
}

impl Superblock {
    const fn new() -> Self {
        Superblock {
            magic: FS_MAGIC,
            version: 1,
            total_blocks: 4096,
            free_blocks: 4096,
            total_entries: 128,
            used_entries: 0,
            block_size: SECTOR_SIZE as u32,
            _reserved: [0; 484],
        }
    }
}

impl MountedDrive {
    fn new(name: String, location: DriveLocation, sectors: u32) -> Self {
        MountedDrive {
            name,
            location,
            size_mb: (sectors as u64 * SECTOR_SIZE as u64 / 1024 / 1024) as u32,
            initialized: false,
            superblock: Superblock::new(),
            fat: [0; FAT_SECTORS as usize * SECTOR_SIZE],
            entries: [DirEntry::empty(); 128],
        }
    }

    /// Initialize filesystem on this drive
    pub fn init(&mut self) -> Result<(), &'static str> {
        let controller = ATA_CONTROLLER.lock();
        
        match controller.read_sector_from(self.location, SUPERBLOCK_SECTOR) {
            Ok(data) => {
                let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                drop(controller);
                
                if magic == FS_MAGIC {
                    self.load_from_disk()?;
                } else {
                    self.format()?;
                }
            }
            Err(_) => {
                drop(controller);
                self.format()?;
            }
        }
        
        self.initialized = true;
        Ok(())
    }

    /// Format this drive
    pub fn format(&mut self) -> Result<(), &'static str> {
        self.superblock = Superblock::new();
        self.fat = [0; FAT_SECTORS as usize * SECTOR_SIZE];
        self.entries = [DirEntry::empty(); 128];
        self.save_to_disk()
    }

    /// Load filesystem from disk
    fn load_from_disk(&mut self) -> Result<(), &'static str> {
        let controller = ATA_CONTROLLER.lock();
        
        // Load superblock
        let sb_data = controller.read_sector_from(self.location, SUPERBLOCK_SECTOR)?;
        unsafe {
            let sb_ptr = sb_data.as_ptr() as *const Superblock;
            self.superblock = *sb_ptr;
        }
        
        // Load FAT
        for i in 0..FAT_SECTORS {
            let sector_data = controller.read_sector_from(self.location, FAT_START_SECTOR + i)?;
            let offset = (i as usize) * SECTOR_SIZE;
            self.fat[offset..offset + SECTOR_SIZE].copy_from_slice(&sector_data);
        }
        
        // Load directory entries
        for i in 0..DIR_SECTORS {
            let sector_data = controller.read_sector_from(self.location, DIR_START_SECTOR + i)?;
            let entries_per_sector = SECTOR_SIZE / 64;
            for j in 0..entries_per_sector {
                let entry_idx = (i as usize) * entries_per_sector + j;
                if entry_idx < 128 {
                    let offset = j * 64;
                    unsafe {
                        let entry_ptr = sector_data[offset..].as_ptr() as *const DirEntry;
                        self.entries[entry_idx] = *entry_ptr;
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Save filesystem to disk
    fn save_to_disk(&self) -> Result<(), &'static str> {
        let controller = ATA_CONTROLLER.lock();
        
        // Save superblock
        let mut sb_sector = [0u8; SECTOR_SIZE];
        unsafe {
            let sb_ptr = &self.superblock as *const Superblock as *const u8;
            core::ptr::copy_nonoverlapping(sb_ptr, sb_sector.as_mut_ptr(), core::mem::size_of::<Superblock>());
        }
        controller.write_sector_to(self.location, SUPERBLOCK_SECTOR, &sb_sector)?;
        
        // Save FAT
        for i in 0..FAT_SECTORS {
            let offset = (i as usize) * SECTOR_SIZE;
            let mut sector_data = [0u8; SECTOR_SIZE];
            sector_data.copy_from_slice(&self.fat[offset..offset + SECTOR_SIZE]);
            controller.write_sector_to(self.location, FAT_START_SECTOR + i, &sector_data)?;
        }
        
        // Save directory entries
        for i in 0..DIR_SECTORS {
            let mut sector_data = [0u8; SECTOR_SIZE];
            let entries_per_sector = SECTOR_SIZE / 64;
            for j in 0..entries_per_sector {
                let entry_idx = (i as usize) * entries_per_sector + j;
                if entry_idx < 128 {
                    let offset = j * 64;
                    unsafe {
                        let entry_ptr = &self.entries[entry_idx] as *const DirEntry as *const u8;
                        core::ptr::copy_nonoverlapping(entry_ptr, sector_data[offset..].as_mut_ptr(), 64);
                    }
                }
            }
            controller.write_sector_to(self.location, DIR_START_SECTOR + i, &sector_data)?;
        }
        
        Ok(())
    }

    fn allocate_block(&mut self) -> Option<u32> {
        for i in 0..self.fat.len() * 8 {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            if (self.fat[byte_idx] & (1 << bit_idx)) == 0 {
                self.fat[byte_idx] |= 1 << bit_idx;
                self.superblock.free_blocks = self.superblock.free_blocks.saturating_sub(1);
                return Some(i as u32);
            }
        }
        None
    }

    fn free_block(&mut self, block: u32) {
        let byte_idx = (block / 8) as usize;
        let bit_idx = (block % 8) as usize;
        if byte_idx < self.fat.len() {
            self.fat[byte_idx] &= !(1 << bit_idx);
            self.superblock.free_blocks += 1;
        }
    }

    fn find_free_entry(&self) -> Option<usize> {
        self.entries.iter().position(|e| e.is_empty())
    }

    fn find_entry(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| !e.is_empty() && e.get_name() == name)
    }

    /// Create a file
    pub fn create_file(&mut self, name: &str) -> bool {
        if self.find_entry(name).is_some() { return false; }
        
        let entry_idx = match self.find_free_entry() {
            Some(idx) => idx,
            None => return false,
        };
        
        self.entries[entry_idx] = DirEntry::empty();
        self.entries[entry_idx].set_name(name);
        self.entries[entry_idx].file_type = FileType::File as u8;
        self.superblock.used_entries += 1;
        
        let _ = self.save_to_disk();
        true
    }

    /// Create a directory
    pub fn create_directory(&mut self, name: &str) -> bool {
        if self.find_entry(name).is_some() { return false; }
        
        let entry_idx = match self.find_free_entry() {
            Some(idx) => idx,
            None => return false,
        };
        
        self.entries[entry_idx] = DirEntry::empty();
        self.entries[entry_idx].set_name(name);
        self.entries[entry_idx].file_type = FileType::Directory as u8;
        self.superblock.used_entries += 1;
        
        let _ = self.save_to_disk();
        true
    }

    /// Write to file
    pub fn write_file(&mut self, name: &str, data: &[u8]) -> bool {
        let entry_idx = match self.find_entry(name) {
            Some(idx) => idx,
            None => return false,
        };
        
        // Free existing blocks
        let old_first_block = self.entries[entry_idx].first_block;
        let old_block_count = self.entries[entry_idx].block_count;
        for i in 0..old_block_count {
            self.free_block(old_first_block + i);
        }
        
        let blocks_needed = (data.len() + SECTOR_SIZE - 1) / SECTOR_SIZE;
        if blocks_needed > BLOCKS_PER_FILE { return false; }
        
        let first_block = if blocks_needed > 0 {
            match self.allocate_block() {
                Some(b) => b,
                None => return false,
            }
        } else { 0 };
        
        for _ in 1..blocks_needed {
            let _ = self.allocate_block();
        }
        
        // Write data
        let controller = ATA_CONTROLLER.lock();
        for i in 0..blocks_needed {
            let mut sector_data = [0u8; SECTOR_SIZE];
            let start = i * SECTOR_SIZE;
            let end = core::cmp::min(start + SECTOR_SIZE, data.len());
            sector_data[..end - start].copy_from_slice(&data[start..end]);
            
            let sector = DATA_START_SECTOR + first_block + i as u32;
            if controller.write_sector_to(self.location, sector, &sector_data).is_err() {
                return false;
            }
        }
        drop(controller);
        
        self.entries[entry_idx].size = data.len() as u32;
        self.entries[entry_idx].first_block = first_block;
        self.entries[entry_idx].block_count = blocks_needed as u32;
        
        let _ = self.save_to_disk();
        true
    }

    /// Read file
    pub fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        let entry_idx = self.find_entry(name)?;
        let entry = &self.entries[entry_idx];
        
        if !entry.is_file() { return None; }
        if entry.size == 0 { return Some(Vec::new()); }
        
        let mut data = Vec::with_capacity(entry.size as usize);
        let controller = ATA_CONTROLLER.lock();
        
        for i in 0..entry.block_count {
            let sector = DATA_START_SECTOR + entry.first_block + i;
            match controller.read_sector_from(self.location, sector) {
                Ok(sector_data) => {
                    let remaining = entry.size as usize - data.len();
                    let to_read = core::cmp::min(remaining, SECTOR_SIZE);
                    data.extend_from_slice(&sector_data[..to_read]);
                }
                Err(_) => return None,
            }
        }
        
        Some(data)
    }

    /// Delete file
    pub fn delete_file(&mut self, name: &str) -> bool {
        let entry_idx = match self.find_entry(name) {
            Some(idx) => idx,
            None => return false,
        };
        
        let first_block = self.entries[entry_idx].first_block;
        let block_count = self.entries[entry_idx].block_count;
        for i in 0..block_count {
            self.free_block(first_block + i);
        }
        
        self.entries[entry_idx] = DirEntry::empty();
        self.superblock.used_entries = self.superblock.used_entries.saturating_sub(1);
        
        let _ = self.save_to_disk();
        true
    }

    /// List files
    pub fn list_files(&self) -> Vec<(String, bool)> {
        self.entries
            .iter()
            .filter(|e| !e.is_empty())
            .map(|e| (e.get_name(), e.is_directory()))
            .collect()
    }

    /// Get file info
    pub fn get_file_info(&self, name: &str) -> Option<(u32, bool)> {
        let entry_idx = self.find_entry(name)?;
        let entry = &self.entries[entry_idx];
        Some((entry.size, entry.is_directory()))
    }

    /// Get stats
    pub fn get_stats(&self) -> (u32, u32, u32, u32) {
        (
            self.superblock.total_blocks,
            self.superblock.free_blocks,
            self.superblock.total_entries,
            self.superblock.used_entries,
        )
    }
}

/// Drive Manager - manages multiple mounted drives
pub struct DriveManager {
    drives: Vec<MountedDrive>,
    initialized: bool,
}

impl DriveManager {
    pub const fn new() -> Self {
        DriveManager {
            drives: Vec::new(),
            initialized: false,
        }
    }

    /// Initialize - detect and mount all drives
    pub fn init(&mut self) -> Result<(), &'static str> {
        let controller = ATA_CONTROLLER.lock();
        let detected = controller.list_drives();
        drop(controller);
        
        for (i, drive_info) in detected.iter().enumerate() {
            let name = alloc::format!("disk{}", i);
            let mut mounted = MountedDrive::new(name, drive_info.location, drive_info.sectors);
            let _ = mounted.init();
            self.drives.push(mounted);
        }
        
        self.initialized = true;
        Ok(())
    }

    /// Get drive by name
    pub fn get_drive(&self, name: &str) -> Option<&MountedDrive> {
        self.drives.iter().find(|d| d.name == name)
    }

    /// Get mutable drive by name
    pub fn get_drive_mut(&mut self, name: &str) -> Option<&mut MountedDrive> {
        self.drives.iter_mut().find(|d| d.name == name)
    }

    /// List all drives
    pub fn list_drives(&self) -> Vec<(String, u32)> {
        self.drives
            .iter()
            .map(|d| (d.name.clone(), d.size_mb))
            .collect()
    }

    /// Check if we have any drives
    pub fn has_drives(&self) -> bool {
        !self.drives.is_empty()
    }

    /// Get first drive name
    pub fn default_drive(&self) -> Option<String> {
        self.drives.first().map(|d| d.name.clone())
    }
}

/// Initialize drive manager
pub fn init() -> Result<(), &'static str> {
    let mut manager = DRIVE_MANAGER.lock();
    manager.init()
}
