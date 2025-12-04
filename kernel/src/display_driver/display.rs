use bootloader_api::info::PixelFormat;
use bootloader_api::{BootInfo};

pub fn init_screen(boot_info: &'static mut BootInfo) -> Screen {
    let framebuffer = boot_info.framebuffer.as_mut().expect("No framebuffer found");
    let info = framebuffer.info();
    let display = Screen::new(
        info.width,
        info.height,
        framebuffer.buffer_mut(),
        info.pixel_format,
    );
    
    return display;
}

pub struct Screen {
    pub width: usize,
    pub height: usize,
    pub framebuffer: &'static mut [u8],
    pub pixel_format: PixelFormat,
}

impl Screen {
    pub fn new(
        width: usize,
        height: usize,
        framebuffer: &'static mut [u8],
        pixel_format: PixelFormat,
    ) -> Self {
        Self {
            width,
            height,
            framebuffer,
            pixel_format,
        }
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, hex_color: &str) {
        let hex = hex_color.trim_start_matches('#');
        let color = u32::from_str_radix(hex, 16).unwrap_or(0);
    }
}