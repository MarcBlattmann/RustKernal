//! ATA/IDE Disk Driver
//! 
//! This module provides low-level access to ATA/IDE hard drives using
//! PIO (Programmed I/O) mode. It supports the primary ATA controller
//! and automatically detects available drives.

use x86_64::instructions::port::Port;
use spin::Mutex;
use alloc::string::String;
use alloc::vec::Vec;

/// ATA I/O ports for primary controller
const ATA_PRIMARY_DATA: u16 = 0x1F0;
const ATA_PRIMARY_ERROR: u16 = 0x1F1;
const ATA_PRIMARY_SECTOR_COUNT: u16 = 0x1F2;
const ATA_PRIMARY_LBA_LOW: u16 = 0x1F3;
const ATA_PRIMARY_LBA_MID: u16 = 0x1F4;
const ATA_PRIMARY_LBA_HIGH: u16 = 0x1F5;
const ATA_PRIMARY_DRIVE_HEAD: u16 = 0x1F6;
const ATA_PRIMARY_STATUS: u16 = 0x1F7;
const ATA_PRIMARY_COMMAND: u16 = 0x1F7;

/// ATA I/O ports for secondary controller
const ATA_SECONDARY_DATA: u16 = 0x170;
const ATA_SECONDARY_ERROR: u16 = 0x171;
const ATA_SECONDARY_SECTOR_COUNT: u16 = 0x172;
const ATA_SECONDARY_LBA_LOW: u16 = 0x173;
const ATA_SECONDARY_LBA_MID: u16 = 0x174;
const ATA_SECONDARY_LBA_HIGH: u16 = 0x175;
const ATA_SECONDARY_DRIVE_HEAD: u16 = 0x176;
const ATA_SECONDARY_STATUS: u16 = 0x177;
const ATA_SECONDARY_COMMAND: u16 = 0x177;

/// ATA Status Register bits
const ATA_STATUS_BSY: u8 = 0x80;  // Busy
const ATA_STATUS_DRDY: u8 = 0x40; // Drive ready
const ATA_STATUS_DRQ: u8 = 0x08;  // Data request ready
const ATA_STATUS_ERR: u8 = 0x01;  // Error

/// ATA Commands
const ATA_CMD_READ_PIO: u8 = 0x20;
const ATA_CMD_WRITE_PIO: u8 = 0x30;
const ATA_CMD_IDENTIFY: u8 = 0xEC;
const ATA_CMD_FLUSH: u8 = 0xE7;

/// Sector size in bytes
pub const SECTOR_SIZE: usize = 512;

/// Global ATA controller instance
pub static ATA_CONTROLLER: Mutex<AtaController> = Mutex::new(AtaController::new());

/// Drive location
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DriveLocation {
    PrimaryMaster,
    PrimarySlave,
    SecondaryMaster,
    SecondarySlave,
}

impl DriveLocation {
    fn get_ports(&self) -> (u16, u16, u16, u16, u16, u16, u16, u16, u16) {
        match self {
            DriveLocation::PrimaryMaster | DriveLocation::PrimarySlave => (
                ATA_PRIMARY_DATA,
                ATA_PRIMARY_ERROR,
                ATA_PRIMARY_SECTOR_COUNT,
                ATA_PRIMARY_LBA_LOW,
                ATA_PRIMARY_LBA_MID,
                ATA_PRIMARY_LBA_HIGH,
                ATA_PRIMARY_DRIVE_HEAD,
                ATA_PRIMARY_STATUS,
                ATA_PRIMARY_COMMAND,
            ),
            DriveLocation::SecondaryMaster | DriveLocation::SecondarySlave => (
                ATA_SECONDARY_DATA,
                ATA_SECONDARY_ERROR,
                ATA_SECONDARY_SECTOR_COUNT,
                ATA_SECONDARY_LBA_LOW,
                ATA_SECONDARY_LBA_MID,
                ATA_SECONDARY_LBA_HIGH,
                ATA_SECONDARY_DRIVE_HEAD,
                ATA_SECONDARY_STATUS,
                ATA_SECONDARY_COMMAND,
            ),
        }
    }

    fn is_slave(&self) -> bool {
        matches!(self, DriveLocation::PrimarySlave | DriveLocation::SecondarySlave)
    }

    fn drive_select_byte(&self) -> u8 {
        if self.is_slave() { 0xF0 } else { 0xE0 }
    }

    pub fn name(&self) -> &'static str {
        match self {
            DriveLocation::PrimaryMaster => "Primary Master",
            DriveLocation::PrimarySlave => "Primary Slave",
            DriveLocation::SecondaryMaster => "Secondary Master",
            DriveLocation::SecondarySlave => "Secondary Slave",
        }
    }
}

/// Detected drive info
#[derive(Clone, Copy)]
pub struct DriveInfo {
    pub location: DriveLocation,
    pub sectors: u32,  // Total sectors (0 if unknown)
    pub present: bool,
}

/// ATA Controller - manages all ATA drives
pub struct AtaController {
    drives: [Option<DriveInfo>; 4],
    active_drive: Option<usize>,
}

impl AtaController {
    pub const fn new() -> Self {
        AtaController {
            drives: [None; 4],
            active_drive: None,
        }
    }

    /// Probe all possible drive locations and detect present drives
    pub fn probe_drives(&mut self) {
        let locations = [
            DriveLocation::PrimaryMaster,
            DriveLocation::PrimarySlave,
            DriveLocation::SecondaryMaster,
            DriveLocation::SecondarySlave,
        ];

        for (i, &location) in locations.iter().enumerate() {
            if let Some(info) = self.identify_drive(location) {
                self.drives[i] = Some(info);
                // Set the first found drive as active (prefer non-boot drives)
                if self.active_drive.is_none() || location.is_slave() {
                    self.active_drive = Some(i);
                }
            }
        }
    }

    /// Identify a specific drive
    fn identify_drive(&self, location: DriveLocation) -> Option<DriveInfo> {
        let (data_port, _error_port, sector_count_port, lba_low_port, 
             lba_mid_port, lba_high_port, drive_head_port, status_port, command_port) 
            = location.get_ports();

        unsafe {
            let mut data = Port::<u16>::new(data_port);
            let mut sector_count = Port::<u8>::new(sector_count_port);
            let mut lba_low = Port::<u8>::new(lba_low_port);
            let mut lba_mid = Port::<u8>::new(lba_mid_port);
            let mut lba_high = Port::<u8>::new(lba_high_port);
            let mut drive_head = Port::<u8>::new(drive_head_port);
            let mut status = Port::<u8>::new(status_port);
            let mut command = Port::<u8>::new(command_port);

            // Select drive
            drive_head.write(location.drive_select_byte());
            
            // Small delay - read status port 4 times (400ns delay)
            for _ in 0..4 {
                status.read();
            }

            // Zero out ports
            sector_count.write(0);
            lba_low.write(0);
            lba_mid.write(0);
            lba_high.write(0);

            // Send IDENTIFY command
            command.write(ATA_CMD_IDENTIFY);

            // Check if drive exists
            let stat = status.read();
            if stat == 0 || stat == 0xFF {
                return None;
            }

            // Wait for BSY to clear
            let mut timeout = 100000;
            while (status.read() & ATA_STATUS_BSY) != 0 {
                timeout -= 1;
                if timeout == 0 {
                    return None;
                }
            }

            // Check for ATAPI (we only support ATA)
            if lba_mid.read() != 0 || lba_high.read() != 0 {
                return None; // ATAPI device, skip
            }

            // Wait for DRQ
            timeout = 100000;
            loop {
                let s = status.read();
                if (s & ATA_STATUS_ERR) != 0 {
                    return None;
                }
                if (s & ATA_STATUS_DRQ) != 0 {
                    break;
                }
                timeout -= 1;
                if timeout == 0 {
                    return None;
                }
            }

            // Read identify data
            let mut identify_data = [0u16; 256];
            for word in &mut identify_data {
                *word = data.read();
            }

            // Extract total sectors (LBA28)
            let sectors = ((identify_data[61] as u32) << 16) | (identify_data[60] as u32);

            Some(DriveInfo {
                location,
                sectors,
                present: true,
            })
        }
    }

    /// Get the active drive for filesystem operations
    pub fn get_active_drive(&self) -> Option<&DriveInfo> {
        self.active_drive.and_then(|i| self.drives[i].as_ref())
    }

    /// Set active drive by index
    pub fn set_active_drive(&mut self, index: usize) -> bool {
        if index < 4 && self.drives[index].is_some() {
            self.active_drive = Some(index);
            true
        } else {
            false
        }
    }

    /// List all detected drives
    pub fn list_drives(&self) -> Vec<DriveInfo> {
        self.drives.iter().filter_map(|d| *d).collect()
    }

    /// Check if any drive is available
    pub fn has_drive(&self) -> bool {
        self.active_drive.is_some()
    }

    /// Read a sector from the active drive
    pub fn read_sector(&self, lba: u32) -> Result<[u8; SECTOR_SIZE], &'static str> {
        let drive = self.get_active_drive().ok_or("No active drive")?;
        self.read_sector_from(drive.location, lba)
    }

    /// Read a sector from a specific drive
    pub fn read_sector_from(&self, location: DriveLocation, lba: u32) -> Result<[u8; SECTOR_SIZE], &'static str> {
        let (data_port, _error_port, sector_count_port, lba_low_port, 
             lba_mid_port, lba_high_port, drive_head_port, status_port, command_port) 
            = location.get_ports();

        unsafe {
            let mut data = Port::<u16>::new(data_port);
            let mut sector_count = Port::<u8>::new(sector_count_port);
            let mut lba_low = Port::<u8>::new(lba_low_port);
            let mut lba_mid = Port::<u8>::new(lba_mid_port);
            let mut lba_high = Port::<u8>::new(lba_high_port);
            let mut drive_head = Port::<u8>::new(drive_head_port);
            let mut status = Port::<u8>::new(status_port);
            let mut command = Port::<u8>::new(command_port);

            // Wait for not busy
            let mut timeout = 100000;
            while (status.read() & ATA_STATUS_BSY) != 0 {
                timeout -= 1;
                if timeout == 0 {
                    return Err("Drive busy timeout");
                }
            }

            // Select drive and set LBA
            let drive_byte = location.drive_select_byte() | ((lba >> 24) & 0x0F) as u8;
            drive_head.write(drive_byte);
            
            // Small delay
            for _ in 0..4 { status.read(); }

            sector_count.write(1);
            lba_low.write((lba & 0xFF) as u8);
            lba_mid.write(((lba >> 8) & 0xFF) as u8);
            lba_high.write(((lba >> 16) & 0xFF) as u8);

            // Send read command
            command.write(ATA_CMD_READ_PIO);

            // Wait for DRQ
            timeout = 100000;
            loop {
                let s = status.read();
                if (s & ATA_STATUS_ERR) != 0 {
                    return Err("Read error");
                }
                if (s & ATA_STATUS_DRQ) != 0 {
                    break;
                }
                timeout -= 1;
                if timeout == 0 {
                    return Err("DRQ timeout");
                }
            }

            // Read data
            let mut buffer = [0u8; SECTOR_SIZE];
            for i in (0..SECTOR_SIZE).step_by(2) {
                let word = data.read();
                buffer[i] = (word & 0xFF) as u8;
                buffer[i + 1] = ((word >> 8) & 0xFF) as u8;
            }

            Ok(buffer)
        }
    }

    /// Write a sector to the active drive
    pub fn write_sector(&self, lba: u32, buffer: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        let drive = self.get_active_drive().ok_or("No active drive")?;
        self.write_sector_to(drive.location, lba, buffer)
    }

    /// Write a sector to a specific drive
    pub fn write_sector_to(&self, location: DriveLocation, lba: u32, buffer: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        let (data_port, _error_port, sector_count_port, lba_low_port, 
             lba_mid_port, lba_high_port, drive_head_port, status_port, command_port) 
            = location.get_ports();

        unsafe {
            let mut data = Port::<u16>::new(data_port);
            let mut sector_count = Port::<u8>::new(sector_count_port);
            let mut lba_low = Port::<u8>::new(lba_low_port);
            let mut lba_mid = Port::<u8>::new(lba_mid_port);
            let mut lba_high = Port::<u8>::new(lba_high_port);
            let mut drive_head = Port::<u8>::new(drive_head_port);
            let mut status = Port::<u8>::new(status_port);
            let mut command = Port::<u8>::new(command_port);

            // Wait for not busy
            let mut timeout = 100000;
            while (status.read() & ATA_STATUS_BSY) != 0 {
                timeout -= 1;
                if timeout == 0 {
                    return Err("Drive busy timeout");
                }
            }

            // Select drive and set LBA
            let drive_byte = location.drive_select_byte() | ((lba >> 24) & 0x0F) as u8;
            drive_head.write(drive_byte);
            
            // Small delay
            for _ in 0..4 { status.read(); }

            sector_count.write(1);
            lba_low.write((lba & 0xFF) as u8);
            lba_mid.write(((lba >> 8) & 0xFF) as u8);
            lba_high.write(((lba >> 16) & 0xFF) as u8);

            // Send write command
            command.write(ATA_CMD_WRITE_PIO);

            // Wait for DRQ
            timeout = 100000;
            loop {
                let s = status.read();
                if (s & ATA_STATUS_ERR) != 0 {
                    return Err("Write error");
                }
                if (s & ATA_STATUS_DRQ) != 0 {
                    break;
                }
                timeout -= 1;
                if timeout == 0 {
                    return Err("DRQ timeout");
                }
            }

            // Write data
            for i in (0..SECTOR_SIZE).step_by(2) {
                let word = (buffer[i] as u16) | ((buffer[i + 1] as u16) << 8);
                data.write(word);
            }

            // Flush cache
            command.write(ATA_CMD_FLUSH);

            // Wait for completion
            timeout = 100000;
            while (status.read() & ATA_STATUS_BSY) != 0 {
                timeout -= 1;
                if timeout == 0 {
                    return Err("Flush timeout");
                }
            }

            Ok(())
        }
    }
}

// Legacy compatibility layer for existing code
pub struct AtaDevice;

impl AtaDevice {
    pub fn read_sector(lba: u32) -> Result<[u8; SECTOR_SIZE], &'static str> {
        ATA_CONTROLLER.lock().read_sector(lba)
    }

    pub fn write_sector(lba: u32, data: &[u8; SECTOR_SIZE]) -> Result<(), &'static str> {
        ATA_CONTROLLER.lock().write_sector(lba, data)
    }
}

/// Read multiple sectors
pub fn read_sectors(start_lba: u32, count: u32) -> Result<Vec<u8>, &'static str> {
    let controller = ATA_CONTROLLER.lock();
    let mut result = Vec::with_capacity((count as usize) * SECTOR_SIZE);
    
    for i in 0..count {
        let sector = controller.read_sector(start_lba + i)?;
        result.extend_from_slice(&sector);
    }
    
    Ok(result)
}

/// Write multiple sectors
pub fn write_sectors(start_lba: u32, data: &[u8]) -> Result<(), &'static str> {
    let controller = ATA_CONTROLLER.lock();
    let sector_count = (data.len() + SECTOR_SIZE - 1) / SECTOR_SIZE;
    
    for i in 0..sector_count {
        let mut sector_data = [0u8; SECTOR_SIZE];
        let start = i * SECTOR_SIZE;
        let end = core::cmp::min(start + SECTOR_SIZE, data.len());
        sector_data[..end - start].copy_from_slice(&data[start..end]);
        
        controller.write_sector(start_lba + i as u32, &sector_data)?;
    }
    
    Ok(())
}

/// Initialize the ATA driver - probes for all drives
pub fn init() -> Result<(), &'static str> {
    let mut controller = ATA_CONTROLLER.lock();
    controller.probe_drives();
    
    if controller.has_drive() {
        Ok(())
    } else {
        Err("No ATA drives detected")
    }
}

/// Get disk info as a formatted string
pub fn get_disk_info() -> String {
    let controller = ATA_CONTROLLER.lock();
    let drives = controller.list_drives();
    
    if drives.is_empty() {
        return String::from("No ATA drives detected");
    }
    
    let mut info = String::new();
    for drive in drives {
        let size_mb = (drive.sectors as u64 * SECTOR_SIZE as u64) / (1024 * 1024);
        info.push_str(&alloc::format!(
            "{}: {}MB\n",
            drive.location.name(),
            size_mb
        ));
    }
    
    if let Some(active) = controller.get_active_drive() {
        info.push_str(&alloc::format!("Active: {}", active.location.name()));
    }
    
    info
}

/// Get list of detected drives for display
pub fn list_detected_drives() -> Vec<(String, u32)> {
    let controller = ATA_CONTROLLER.lock();
    controller.list_drives()
        .iter()
        .map(|d| (String::from(d.location.name()), d.sectors))
        .collect()
}
