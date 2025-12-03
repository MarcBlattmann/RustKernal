// launches the kernel in QEMU

use std::process::Command;

fn main() {
    let bios_path = env!("BIOS_PATH");
    
    println!("Starting QEMU with BIOS image: {}", bios_path);
    
    let status = Command::new("qemu-system-x86_64")
        .args([
            "-drive", &format!("format=raw,file={}", bios_path),
        ])
        .status()
        .expect("Failed to start QEMU. Make sure qemu-system-x86_64 is installed and in your PATH.");
    
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}