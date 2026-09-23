# Rust kernal

To build you need :

>-> https://visualstudio.microsoft.com/visual-cpp-build-tools/
>
>-> rustup target add x86_64-unknown-none
>z
>-> rustup component add llvm-tools-preview

To run you need :

>-> quemu

___

### Run the kernal in qemu
cargo run

### Build the kernal
cargo build

___

### Graphics

The display driver (`kernel/src/drivers/display`) renders into a back buffer and presents at a steady 60 fps.
On QEMU (`-vga std`), Bochs and VirtualBox it switches the adapter to 1920x1080 and flips between two pages
of video memory for tear-free frames; elsewhere it falls back to the bootloader framebuffer.

Type `demo` in the shell for an animated test scene with an FPS / frame-time overlay (any key exits).
Kernel log output, including the chosen display mode and fps, is printed to the terminal running `cargo run`.
