// launches the kernel in QEMU

mod disk_image;

use std::path::Path;
use std::process::Command;

/// QEMU's `-drive` option list uses commas as separators; a literal comma is written twice.
fn qemu_drive(path: &Path, index: u8) -> String {
    let file = path.display().to_string().replace(',', ",,");
    format!("format=raw,file={},index={},media=disk", file, index)
}

fn main() {
    let bios_path = Path::new(env!("BIOS_PATH"));
    let disk_path = disk_image::disk_path();

    // The data disk must exist before QEMU starts; it is created (and formatted) only once.
    if let Err(error) = disk_image::ensure_data_disk(&disk_path) {
        eprintln!("error: cannot prepare the data disk: {}", error);
        eprintln!("(set {} to use a different image)", disk_image::DISK_PATH_VARIABLE);
        std::process::exit(1);
    }

    println!("Starting QEMU with BIOS image: {}", bios_path.display());

    let status = Command::new("qemu-system-x86_64")
        .args([
            // IDE primary master (hd0 in the kernel): the boot image.
            "-drive", &qemu_drive(bios_path, 0),
            // IDE primary slave (hd1): the persistent FAT32 data disk.
            "-drive", &qemu_drive(&disk_path, 1),
            "-m", "512M",
            // Standard VGA implements the Bochs display interface the kernel's display driver uses.
            // 32 MiB of video memory fits two 1920x1080 pages for page flipping.
            "-vga", "std",
            "-global", "VGA.vgamem_mb=32",
            // Kernel log output (display mode, fps) goes to this terminal.
            "-serial", "stdio",
        ])
        .status()
        .expect("Failed to start QEMU. Make sure qemu-system-x86_64 is installed and in your PATH.");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
