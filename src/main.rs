// launches the kernel in QEMU

use std::process::Command;
use std::path::PathBuf;
use std::fs;

const DISK_SIZE_MB: u64 = 32;

fn main() {
    let bios_path = env!("BIOS_PATH");
    
    // Store disk.img in the PROJECT ROOT directory (not the temp build dir)
    // This ensures the disk persists across rebuilds and cargo clean
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let disk_path = project_root.join("disk.img");
    let disk_path_str = disk_path.to_string_lossy();
    
    if !disk_path.exists() {
        println!("Creating {}MB virtual disk at: {}", DISK_SIZE_MB, disk_path_str);
        let disk_size = DISK_SIZE_MB * 1024 * 1024; // Convert to bytes
        let zeros = vec![0u8; disk_size as usize];
        fs::write(&disk_path, zeros).expect("Failed to create disk image");
        println!("Disk image created successfully!");
    } else {
        println!("Using existing disk image: {}", disk_path_str);
    }
    
    println!("Starting QEMU with BIOS image: {}", bios_path);
    println!("Data disk: {}", disk_path_str);
    
    // Use explicit IDE configuration for reliable drive mapping:
    // - Boot drive on IDE bus 0, unit 0 (Primary Master)
    // - Data drive on IDE bus 0, unit 1 (Primary Slave)
    let status = Command::new("qemu-system-x86_64")
        .args([
            // Boot drive (Primary Master)
            "-drive", &format!("if=ide,bus=0,unit=0,format=raw,file={}", bios_path),
            // Persistent data drive (Primary Slave) 
            "-drive", &format!("if=ide,bus=0,unit=1,format=raw,file={}", disk_path_str),
        ])
        .status()
        .expect("Failed to start QEMU. Make sure qemu-system-x86_64 is installed and in your PATH.");
    
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}