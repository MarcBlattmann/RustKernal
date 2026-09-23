//! Driver for the Bochs Graphics Adapter ("BGA" / Bochs VBE extensions), the display device that
//! QEMU (`-vga std`), Bochs and VirtualBox emulate. It lets the kernel pick its own resolution and
//! flip between two pages of video memory, so a frame is only shown once it is completely drawn.

use crate::drivers::pci;
use crate::memory::paging;
use x86_64::instructions::port::Port;

const INDEX_PORT: u16 = 0x01CE;
const DATA_PORT: u16 = 0x01CF;

const REGISTER_ID: u16 = 0;
const REGISTER_X_RESOLUTION: u16 = 1;
const REGISTER_Y_RESOLUTION: u16 = 2;
const REGISTER_BITS_PER_PIXEL: u16 = 3;
const REGISTER_ENABLE: u16 = 4;
const REGISTER_VIRTUAL_WIDTH: u16 = 6;
const REGISTER_VIRTUAL_HEIGHT: u16 = 7;
const REGISTER_X_OFFSET: u16 = 8;
const REGISTER_Y_OFFSET: u16 = 9;

/// Version 0xB0C2 added 32-bit colour and the linear framebuffer.
const MINIMUM_VERSION: u16 = 0xB0C2;
const MAXIMUM_VERSION: u16 = 0xB0CF;

const ENABLE_DISPLAY: u16 = 0x01;
const ENABLE_LINEAR_FRAMEBUFFER: u16 = 0x40;

const BITS_PER_PIXEL: u16 = 32;
const BYTES_PER_PIXEL: usize = 4;

/// PCI ids of devices that implement the BGA interface: QEMU/Bochs standard VGA and VirtualBox.
const SUPPORTED_DEVICES: [(u16, u16); 2] = [(0x1234, 0x1111), (0x80EE, 0xBEEF)];

pub struct BgaDevice {
    /// Video memory, mapped into the kernel address space.
    video_memory: *mut u32,
    video_memory_size: usize,
}

pub struct BgaMode {
    pub width: usize,
    pub height: usize,
    /// How many full screens fit in video memory (2 or more allows page flipping).
    pub pages: usize,
}

fn write_register(register: u16, value: u16) {
    unsafe {
        Port::<u16>::new(INDEX_PORT).write(register);
        Port::<u16>::new(DATA_PORT).write(value);
    }
}

fn read_register(register: u16) -> u16 {
    unsafe {
        Port::<u16>::new(INDEX_PORT).write(register);
        Port::<u16>::new(DATA_PORT).read()
    }
}

impl BgaDevice {
    /// Finds the adapter and maps its video memory. Does not change the current mode.
    pub fn detect() -> Option<Self> {
        let version = read_register(REGISTER_ID);
        if !(MINIMUM_VERSION..=MAXIMUM_VERSION).contains(&version) {
            return None;
        }

        let device = SUPPORTED_DEVICES
            .iter()
            .find_map(|&(vendor_id, device_id)| pci::find_device(vendor_id, device_id))?;
        let (physical_address, size) = device.memory_bar(0)?;
        let video_memory = paging::map_device_memory(physical_address, size)? as *mut u32;

        crate::serial_println!(
            "bga: version {:#x}, {} MiB video memory at {:#x}",
            version,
            size / (1024 * 1024),
            physical_address
        );
        Some(Self { video_memory, video_memory_size: size })
    }

    pub fn video_memory_size(&self) -> usize {
        self.video_memory_size
    }

    /// Switches to `width` x `height` at 32 bits per pixel. Returns `None` if the adapter refused.
    pub fn set_mode(&mut self, width: usize, height: usize) -> Option<BgaMode> {
        let pages = self.video_memory_size / (width * height * BYTES_PER_PIXEL);
        if pages == 0 {
            return None;
        }

        write_register(REGISTER_ENABLE, 0);
        write_register(REGISTER_X_RESOLUTION, width as u16);
        write_register(REGISTER_Y_RESOLUTION, height as u16);
        write_register(REGISTER_BITS_PER_PIXEL, BITS_PER_PIXEL);
        write_register(REGISTER_ENABLE, ENABLE_DISPLAY | ENABLE_LINEAR_FRAMEBUFFER);

        // Enabling resets the virtual screen; widen it afterwards so all pages are addressable.
        write_register(REGISTER_VIRTUAL_WIDTH, width as u16);
        write_register(REGISTER_VIRTUAL_HEIGHT, (height * pages.min(2)) as u16);
        write_register(REGISTER_X_OFFSET, 0);
        write_register(REGISTER_Y_OFFSET, 0);

        let accepted = read_register(REGISTER_X_RESOLUTION) as usize == width
            && read_register(REGISTER_Y_RESOLUTION) as usize == height
            && read_register(REGISTER_BITS_PER_PIXEL) == BITS_PER_PIXEL;
        if !accepted {
            return None;
        }

        let virtual_height = read_register(REGISTER_VIRTUAL_HEIGHT) as usize;
        let pages = (virtual_height / height).clamp(1, 2);
        Some(BgaMode { width, height, pages })
    }

    /// Returns page `index` of the current mode as a slice of pixels.
    pub fn page(&mut self, index: usize, mode: &BgaMode) -> &mut [u32] {
        let page_pixels = mode.width * mode.height;
        unsafe { core::slice::from_raw_parts_mut(self.video_memory.add(index * page_pixels), page_pixels) }
    }

    /// Makes page `index` the one the monitor shows.
    pub fn show_page(&mut self, index: usize, mode: &BgaMode) {
        write_register(REGISTER_Y_OFFSET, (index * mode.height) as u16);
    }
}
