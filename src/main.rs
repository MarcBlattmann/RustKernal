// launches the kernel in QEMU

use std::process::Command;
use std::path::Path;
use std::fs;

const DISK_SIZE_MB: u64 = 32;

fn main() {
    let bios_path = env!("BIOS_PATH");
    
    let bios_dir = Path::new(bios_path).parent().unwrap_or(Path::new("."));
    let disk_path = bios_dir.join("disk.img");
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
    
    let status = Command::new("qemu-system-x86_64")
        .args([
            "-drive", &format!("format=raw,file={},index=0,media=disk", bios_path),
            "-drive", &format!("format=raw,file={},index=1,media=disk", disk_path_str),
        ])
        .status()
        .expect("Failed to start QEMU. Make sure qemu-system-x86_64 is installed and in your PATH.");
    
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}