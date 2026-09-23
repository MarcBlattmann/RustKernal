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

___

### Storage

`cargo run` attaches two IDE disks to QEMU:

| Kernel name | QEMU drive | Contents |
|-------------|------------|----------|
| `hd0` | primary master (`index=0`) | the boot image (`bios.img`), rebuilt on every build |
| `hd1` | primary slave (`index=1`) | the persistent data disk, `target/disk.img` |

The first time you run it, the runner creates `target/disk.img`: a 128 MiB raw image with an MBR,
one FAT32 partition (label `RUSTKERNAL`) and a `README.TXT`. After that the runner never modifies
the image, so everything saved on `hd1` from inside the OS survives restarts and rebuilds.
Delete `target/disk.img` to start over with an empty disk. Note that `cargo clean` deletes `target/`
and with it the disk, so keep a copy of anything important or store the image elsewhere by setting
`RUSTKERNAL_DISK` (PowerShell: `$env:RUSTKERNAL_DISK = "D:\disks\work.img"; cargo run`).
The image is a normal MBR/FAT32 disk, so the host can mount or inspect it too (while QEMU is not running).

The kernel talks to the disks through an ATA PIO driver (`kernel/src/drivers/ata.rs`) for the
legacy IDE channels: it detects up to four drives (`hd0`-`hd3`), skips CD/DVD drives, supports
28- and 48-bit LBA and times out instead of hanging on unresponsive hardware. The FAT32 code lives
in the `fat32` crate, which the kernel and the runner share.

___

### Files

At boot the kernel detects the drives, reads their partition tables and mounts the first FAT32
volume it finds on a drive that is not the boot drive (the one with an active MBR partition, `hd0`
under QEMU) at `/`. The shell's first lines say what happened, e.g.
`Storage: / mounted from hd1p1 (RUSTKERNAL, 125.0 MiB, 125.0 MiB free)`, or tell you to run
`format hd1` when the data disk has no file system. Nothing is ever formatted automatically.

The prompt shows the working directory (`rustkernel:/docs $`). Paths may be absolute or relative;
`.`, `..` and `~` (the root) work, names are case-insensitive as on any FAT volume, and long names
with spaces are fine when quoted: `cat "my notes.txt"`. Tab completes command names, file and
directory names (directories get a trailing `/`) and drive names after `mount` and `format`.

| Command | What it does |
|---------|--------------|
| `ls [-l] [-a] [path...]` | list a directory; `-l` adds size and modified time, `-a` shows hidden files |
| `cd [path \| -]`, `pwd` | change / print the working directory (`cd` alone goes to `/`, `cd -` back) |
| `tree [-a] [path]` | a directory and everything below it |
| `cat`, `head [-n N]`, `tail [-n N]`, `hexdump` | read files (non-printable bytes are shown as `.`) |
| `stat <path>` | size, created and modified time, attributes |
| `write <file> <text...>` | replace a file with one line of text |
| `edit <file>` | full-screen text editor: Ctrl+S saves, Ctrl+Q or Esc quits, Ctrl+F finds, Ctrl+G goes to a line, Ctrl+K / Ctrl+U cut and paste lines |
| `touch`, `mkdir [-p]`, `rmdir` | create files and directories, remove empty directories |
| `rm [-r] [-f]`, `mv`, `cp [-r]` | remove, move/rename and copy (into a directory if the target is one) |
| `df`, `lsblk` | free space of the mounted volume; drives, partitions, labels and mounts |
| `mount [drive]`, `umount`, `sync` | mount a drive or partition (`hd1`, `hd1p1`) on `/`, unmount it, flush it |
| `fsck` | read-only consistency check of the mounted volume |
| `format <drive> [label]` | erase a drive and create one FAT32 partition (asks you to type the drive name again) |

Any command's output can go into a file: `ls -l > listing.txt` replaces the file, `>>` appends
(`echo remember the milk >> todo.txt`). Errors still appear on the screen.

Data is safe to power off between commands: the shell writes all cached changes to the disk after
every command, and `reboot`/`shutdown` do the same first. Only a power loss in the middle of a
command can lose that command's changes; the next boot then says the volume was not cleanly
unmounted, recounts the free space, and `fsck` shows whether anything was lost (at worst
unreachable "lost" clusters). The boot drive is never mounted, formatted or written to.

Timestamps come from the hardware clock, which QEMU runs in UTC. To look at the data disk from the
host (read-only, with the `fat32` crate and with the independent `fatfs` crate), run
`RUSTKERNAL_IMAGE=target/disk.img cargo test -p fat32 --test disk_image inspect -- --ignored --nocapture`.
