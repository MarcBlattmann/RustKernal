//! Simple Disk-Backed Filesystem
//! 
//! This module implements a simple filesystem that stores data on a real disk.
//! It uses a FAT-like structure with:
//! - Superblock (sector 0): Filesystem metadata
//! - File Allocation Table (sectors 1-8): Block allocation bitmap
//! - Directory entries (sectors 9-16): File/directory metadata
//! - Data blocks (sector 17+): Actual file content

use spin::Mutex;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::format;
use crate::drivers::ata::{self, SECTOR_SIZE};

/// Filesystem constants
const SUPERBLOCK_SECTOR: u32 = 100;      // Start filesystem at sector 100 (avoid boot sectors)
const FAT_START_SECTOR: u32 = 101;       // File Allocation Table start
const FAT_SECTORS: u32 = 8;              // 8 sectors for FAT = 4096 blocks
const DIR_START_SECTOR: u32 = 109;       // Directory entries start
const DIR_SECTORS: u32 = 8;              // 8 sectors for directory = 128 entries
const DATA_START_SECTOR: u32 = 117;      // Data blocks start
const MAX_FILENAME: usize = 32;          // Maximum filename length
const BLOCKS_PER_FILE: usize = 64;       // Maximum blocks per file

/// Magic number to identify our filesystem
const FS_MAGIC: u32 = 0x5253_4653;       // "RSFS" in hex

/// Global filesystem instance
pub static FILESYSTEM: Mutex<SimpleFS> = Mutex::new(SimpleFS::new());

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
    pub name: [u8; MAX_FILENAME],  // 32 bytes: filename
    pub file_type: u8,             // 1 byte: file type
    pub _reserved: [u8; 3],        // 3 bytes: reserved
    pub size: u32,                 // 4 bytes: file size in bytes
    pub first_block: u32,          // 4 bytes: first data block
    pub block_count: u32,          // 4 bytes: number of blocks
    pub created: u32,              // 4 bytes: creation timestamp
    pub modified: u32,             // 4 bytes: modification timestamp
    pub _padding: [u8; 8],         // 8 bytes: padding to 64 bytes
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
            if b == 0 {
                break;
            }
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

    pub fn is_empty(&self) -> bool {
        self.file_type == FileType::Empty as u8
    }

    pub fn is_file(&self) -> bool {
        self.file_type == FileType::File as u8
    }

    pub fn is_directory(&self) -> bool {
        self.file_type == FileType::Directory as u8
    }
}

/// Superblock structure (512 bytes)
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct Superblock {
    magic: u32,                    // Filesystem magic number
    version: u32,                  // Filesystem version
    total_blocks: u32,             // Total data blocks
    free_blocks: u32,              // Free data blocks
    total_entries: u32,            // Total directory entries
    used_entries: u32,             // Used directory entries
    block_size: u32,               // Block size in bytes
    _reserved: [u8; 484],          // Reserved
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

/// Simple Filesystem structure
pub struct SimpleFS {
    initialized: bool,
    use_disk: bool,
    // In-memory cache
    superblock: Superblock,
    fat: [u8; FAT_SECTORS as usize * SECTOR_SIZE],
    entries: [DirEntry; 128],
}

impl SimpleFS {
    pub const fn new() -> Self {
        SimpleFS {
            initialized: false,
            use_disk: false,
            superblock: Superblock::new(),
            fat: [0; FAT_SECTORS as usize * SECTOR_SIZE],
            entries: [DirEntry::empty(); 128],
        }
    }

    /// Initialize the filesystem
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Try to read superblock from disk
        match ata::AtaDevice::read_sector(SUPERBLOCK_SECTOR) {
            Ok(data) => {
                // Check if filesystem exists on disk
                let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                
                if magic == FS_MAGIC {
                    // Load existing filesystem from disk
                    self.load_from_disk()?;
                    self.use_disk = true;
                } else {
                    // Format new filesystem
                    self.format()?;
                    self.use_disk = true;
                }
            }
            Err(_) => {
                // No disk available, use in-memory filesystem
                self.use_disk = false;
            }
        }
        
        self.initialized = true;
        Ok(())
    }

    /// Format the filesystem (creates new empty filesystem)
    pub fn format(&mut self) -> Result<(), &'static str> {
        // Initialize superblock
        self.superblock = Superblock::new();
        
        // Clear FAT
        self.fat = [0; FAT_SECTORS as usize * SECTOR_SIZE];
        
        // Clear directory entries
        self.entries = [DirEntry::empty(); 128];
        
        // Save to disk if available
        if self.use_disk {
            self.save_to_disk()?;
        }
        
        Ok(())
    }

    /// Load filesystem from disk
    fn load_from_disk(&mut self) -> Result<(), &'static str> {
        // Load superblock
        let sb_data = ata::AtaDevice::read_sector(SUPERBLOCK_SECTOR)?;
        unsafe {
            let sb_ptr = sb_data.as_ptr() as *const Superblock;
            self.superblock = *sb_ptr;
        }
        
        // Load FAT
        for i in 0..FAT_SECTORS {
            let sector_data = ata::AtaDevice::read_sector(FAT_START_SECTOR + i)?;
            let offset = (i as usize) * SECTOR_SIZE;
            self.fat[offset..offset + SECTOR_SIZE].copy_from_slice(&sector_data);
        }
        
        // Load directory entries
        for i in 0..DIR_SECTORS {
            let sector_data = ata::AtaDevice::read_sector(DIR_START_SECTOR + i)?;
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
        if !self.use_disk {
            return Ok(());
        }
        
        // Save superblock
        let mut sb_sector = [0u8; SECTOR_SIZE];
        unsafe {
            let sb_ptr = &self.superblock as *const Superblock as *const u8;
            core::ptr::copy_nonoverlapping(sb_ptr, sb_sector.as_mut_ptr(), core::mem::size_of::<Superblock>());
        }
        ata::AtaDevice::write_sector(SUPERBLOCK_SECTOR, &sb_sector)?;
        
        // Save FAT
        for i in 0..FAT_SECTORS {
            let offset = (i as usize) * SECTOR_SIZE;
            let mut sector_data = [0u8; SECTOR_SIZE];
            sector_data.copy_from_slice(&self.fat[offset..offset + SECTOR_SIZE]);
            ata::AtaDevice::write_sector(FAT_START_SECTOR + i, &sector_data)?;
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
            ata::AtaDevice::write_sector(DIR_START_SECTOR + i, &sector_data)?;
        }
        
        Ok(())
    }

    /// Allocate a data block
    fn allocate_block(&mut self) -> Option<u32> {
        for i in 0..self.fat.len() * 8 {
            let byte_idx = i / 8;
            let bit_idx = i % 8;
            if (self.fat[byte_idx] & (1 << bit_idx)) == 0 {
                // Mark block as used
                self.fat[byte_idx] |= 1 << bit_idx;
                self.superblock.free_blocks = self.superblock.free_blocks.saturating_sub(1);
                return Some(i as u32);
            }
        }
        None
    }

    /// Free a data block
    fn free_block(&mut self, block: u32) {
        let byte_idx = (block / 8) as usize;
        let bit_idx = (block % 8) as usize;
        if byte_idx < self.fat.len() {
            self.fat[byte_idx] &= !(1 << bit_idx);
            self.superblock.free_blocks += 1;
        }
    }

    /// Find a free directory entry
    fn find_free_entry(&self) -> Option<usize> {
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.is_empty() {
                return Some(i);
            }
        }
        None
    }

    /// Find entry by name
    fn find_entry(&self, name: &str) -> Option<usize> {
        for (i, entry) in self.entries.iter().enumerate() {
            if !entry.is_empty() && entry.get_name() == name {
                return Some(i);
            }
        }
        None
    }

    /// Create a new file
    pub fn create_file(&mut self, name: String) -> bool {
        // Check if file already exists
        if self.find_entry(&name).is_some() {
            return false;
        }
        
        // Find free directory entry
        let entry_idx = match self.find_free_entry() {
            Some(idx) => idx,
            None => return false,
        };
        
        // Create entry
        self.entries[entry_idx] = DirEntry::empty();
        self.entries[entry_idx].set_name(&name);
        self.entries[entry_idx].file_type = FileType::File as u8;
        self.entries[entry_idx].size = 0;
        self.entries[entry_idx].first_block = 0;
        self.entries[entry_idx].block_count = 0;
        
        self.superblock.used_entries += 1;
        
        // Save to disk
        let _ = self.save_to_disk();
        
        true
    }

    /// Create a new directory
    pub fn create_directory(&mut self, name: String) -> bool {
        // Check if directory already exists
        if self.find_entry(&name).is_some() {
            return false;
        }
        
        // Find free directory entry
        let entry_idx = match self.find_free_entry() {
            Some(idx) => idx,
            None => return false,
        };
        
        // Create entry
        self.entries[entry_idx] = DirEntry::empty();
        self.entries[entry_idx].set_name(&name);
        self.entries[entry_idx].file_type = FileType::Directory as u8;
        
        self.superblock.used_entries += 1;
        
        // Save to disk
        let _ = self.save_to_disk();
        
        true
    }

    /// Write data to a file
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
        
        // Calculate blocks needed
        let blocks_needed = (data.len() + SECTOR_SIZE - 1) / SECTOR_SIZE;
        if blocks_needed > BLOCKS_PER_FILE {
            return false;
        }
        
        // Allocate new blocks
        let first_block = if blocks_needed > 0 {
            match self.allocate_block() {
                Some(b) => b,
                None => return false,
            }
        } else {
            0
        };
        
        // Allocate consecutive blocks
        for i in 1..blocks_needed {
            let _ = self.allocate_block();
        }
        
        // Write data to blocks
        if self.use_disk {
            for i in 0..blocks_needed {
                let mut sector_data = [0u8; SECTOR_SIZE];
                let start = i * SECTOR_SIZE;
                let end = core::cmp::min(start + SECTOR_SIZE, data.len());
                sector_data[..end - start].copy_from_slice(&data[start..end]);
                
                let sector = DATA_START_SECTOR + first_block + i as u32;
                if ata::AtaDevice::write_sector(sector, &sector_data).is_err() {
                    return false;
                }
            }
        }
        
        // Update entry
        self.entries[entry_idx].size = data.len() as u32;
        self.entries[entry_idx].first_block = first_block;
        self.entries[entry_idx].block_count = blocks_needed as u32;
        
        // Save metadata
        let _ = self.save_to_disk();
        
        true
    }

    /// Read data from a file
    pub fn read_file(&self, name: &str) -> Option<Vec<u8>> {
        let entry_idx = self.find_entry(name)?;
        let entry = &self.entries[entry_idx];
        
        if !entry.is_file() {
            return None;
        }
        
        if entry.size == 0 {
            return Some(Vec::new());
        }
        
        let mut data = Vec::with_capacity(entry.size as usize);
        
        if self.use_disk {
            for i in 0..entry.block_count {
                let sector = DATA_START_SECTOR + entry.first_block + i;
                match ata::AtaDevice::read_sector(sector) {
                    Ok(sector_data) => {
                        let remaining = entry.size as usize - data.len();
                        let to_read = core::cmp::min(remaining, SECTOR_SIZE);
                        data.extend_from_slice(&sector_data[..to_read]);
                    }
                    Err(_) => return None,
                }
            }
        }
        
        Some(data)
    }

    /// Delete a file or directory
    pub fn delete_file(&mut self, name: &str) -> bool {
        let entry_idx = match self.find_entry(name) {
            Some(idx) => idx,
            None => return false,
        };
        
        // Free blocks
        let first_block = self.entries[entry_idx].first_block;
        let block_count = self.entries[entry_idx].block_count;
        for i in 0..block_count {
            self.free_block(first_block + i);
        }
        
        // Clear entry
        self.entries[entry_idx] = DirEntry::empty();
        self.superblock.used_entries = self.superblock.used_entries.saturating_sub(1);
        
        // Save to disk
        let _ = self.save_to_disk();
        
        true
    }

    /// List all files and directories
    pub fn list_files(&self) -> Vec<(String, bool)> {
        let mut files = Vec::new();
        
        for entry in &self.entries {
            if !entry.is_empty() {
                files.push((entry.get_name(), entry.is_directory()));
            }
        }
        
        files
    }
    
    /// List files and directories in a specific directory path
    /// Files are stored with full paths like "/folder/file.txt"
    /// This returns just the names of items directly in the given directory
    pub fn list_directory(&self, path: &str) -> Vec<(String, bool)> {
        let mut items = Vec::new();
        let prefix = if path == "/" { 
            String::from("/") 
        } else { 
            format!("{}/", path.trim_end_matches('/'))
        };
        
        for entry in &self.entries {
            if entry.is_empty() {
                continue;
            }
            
            let name = entry.get_name();
            
            // Check if this file is directly in the given directory
            if path == "/" {
                // For root, we want files that start with "/" but have no other "/"
                if name.starts_with('/') {
                    let rest = &name[1..]; // Remove leading "/"
                    if !rest.contains('/') {
                        items.push((rest.to_string(), entry.is_directory()));
                    }
                } else if !name.contains('/') {
                    // Files without any path separator are at root
                    items.push((name, entry.is_directory()));
                }
            } else {
                // For subdirectories, check if file starts with "path/" and has no more "/"
                if name.starts_with(&prefix) {
                    let rest = &name[prefix.len()..];
                    if !rest.contains('/') && !rest.is_empty() {
                        items.push((rest.to_string(), entry.is_directory()));
                    }
                }
            }
        }
        
        items
    }

    /// Get file info
    pub fn get_file_info(&self, name: &str) -> Option<(u32, bool)> {
        let entry_idx = self.find_entry(name)?;
        let entry = &self.entries[entry_idx];
        Some((entry.size, entry.is_directory()))
    }

    /// Get filesystem statistics
    pub fn get_stats(&self) -> (u32, u32, u32, u32) {
        (
            self.superblock.total_blocks,
            self.superblock.free_blocks,
            self.superblock.total_entries,
            self.superblock.used_entries,
        )
    }

    /// Check if using disk storage
    pub fn is_using_disk(&self) -> bool {
        self.use_disk
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// Initialize the filesystem (call this from kernel init)
pub fn init() -> Result<(), &'static str> {
    let mut fs = FILESYSTEM.lock();
    fs.init()
}
