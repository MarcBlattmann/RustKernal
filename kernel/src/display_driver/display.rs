use crate::display_driver::color_utils::color_to_bytes;
use crate::display_driver::bitmap::Bitmap;
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

    pub fn write_pixel(&mut self, x: usize, y: usize, hex_color: &str) -> bool {
        let hex = hex_color.trim_start_matches('#');
        let color = u32::from_str_radix(hex, 16).unwrap_or(0);
        let color_bytes  = color_to_bytes(color, self.pixel_format);

        if let Some(bytes) = color_bytes {
            let bytes_per_pixel = match self.pixel_format {
                PixelFormat::U8 => 1,
                _ => 3,
            };

            let offset = (y * self.width + x) * bytes_per_pixel;

            if offset + bytes_per_pixel <= self.framebuffer.len() {
                self.framebuffer[offset..offset+bytes_per_pixel].copy_from_slice(&bytes[..bytes_per_pixel]);
                return true;
            }
        }
        
        return false;
    }

    pub fn clear_screen(&mut self, hex_color: &str) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.write_pixel(x, y, hex_color);
            }
        }
    }

    pub fn draw_bitmap(&mut self, x: usize, y: usize, bitmap: &Bitmap) {
        for row in 0..bitmap.height {
            for col in 0..bitmap.width {
                let screen_x = x + col;
                let screen_y = y + row;
                if screen_x < self.width && screen_y < self.height {
                    let pixel_index = row * bitmap.width + col;
                    let color = bitmap.pixels[pixel_index];
                    if let Some(bytes) = color_to_bytes(color, self.pixel_format) {
                        let bytes_per_pixel = match self.pixel_format {
                            PixelFormat::U8 => 1,
                            _ => 3,
                        };
                        let offset = (screen_y * self.width + screen_x) * bytes_per_pixel;
                        if offset + bytes_per_pixel <= self.framebuffer.len() {
                            self.framebuffer[offset..offset+bytes_per_pixel]
                                .copy_from_slice(&bytes[..bytes_per_pixel]);
                        }
                    }
                }
            }
        }
    }
}