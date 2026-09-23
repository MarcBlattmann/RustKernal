// launches the kernel in QEMU

use std::process::Command;

fn main() {
    let bios_path = env!("BIOS_PATH");

    println!("Starting QEMU with BIOS image: {}", bios_path);

    let status = Command::new("qemu-system-x86_64")
        .args([
            "-drive", &format!("format=raw,file={}", bios_path),
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
