use super::color_utils::color_to_bytes;
use super::bitmap::Bitmap;
use bootloader_api::info::PixelFormat;
use bootloader_api::BootInfo;
use alloc::vec::Vec;

pub fn init_screen(boot_info: &'static mut BootInfo) -> Screen {
    let framebuffer = boot_info.framebuffer.as_mut().expect("No framebuffer found");
    let info = framebuffer.info();
    let display = Screen::new(
        info.width,
        info.height,
        info.bytes_per_pixel,
        info.stride,
        framebuffer.buffer_mut(),
        info.pixel_format,
    );
    
    return display;
}

pub struct Screen {
    pub width: usize,
    pub height: usize,
    pub bytes_per_pixel: usize,
    pub stride: usize,
    pub framebuffer: &'static mut [u8],
    pub pixel_format: PixelFormat,
    // Double buffering
    back_buffer: Vec<u8>,
    use_back_buffer: bool,
}

impl Screen {
    pub fn new(
        width: usize,
        height: usize,
        bytes_per_pixel: usize,
        stride: usize,
        framebuffer: &'static mut [u8],
        pixel_format: PixelFormat,
    ) -> Self {
        Self {
            width,
            height,
            bytes_per_pixel,
            stride,
            framebuffer,
            pixel_format,
            back_buffer: Vec::new(),
            use_back_buffer: false,
        }
    }

    /// Get screen width
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get screen height
    pub fn height(&self) -> usize {
        self.height
    }

    /// Enable double buffering (call once before GUI mode)
    pub fn enable_double_buffer(&mut self) {
        if self.back_buffer.is_empty() {
            self.back_buffer = alloc::vec![0u8; self.framebuffer.len()];
        }
        self.use_back_buffer = true;
    }

    /// Disable double buffering (return to direct mode)
    pub fn disable_double_buffer(&mut self) {
        self.use_back_buffer = false;
    }

    /// Swap back buffer to front (copy to screen) - optimized with chunks
    pub fn swap_buffers(&mut self) {
        if self.use_back_buffer && !self.back_buffer.is_empty() {
            // Copy in large chunks for better performance
            let src = &self.back_buffer;
            let dst = &mut self.framebuffer;
            let len = src.len().min(dst.len());
            
            // Use chunks for faster copying
            unsafe {
                core::ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    dst.as_mut_ptr(),
                    len
                );
            }
        }
    }

    pub fn write_pixel(&mut self, x: usize, y: usize, color: u32) -> bool {        
        let color_bytes = color_to_bytes(color, self.pixel_format);

        if let Some(bytes) = color_bytes {
            if bytes[3] == 0 {
                return true;
            }
            return self.write_to_buffer(x, y, &bytes);
        }
        return false;
    }

    /// Read a pixel from framebuffer
    pub fn read_pixel(&self, x: usize, y: usize) -> u32 {
        let offset = (y * self.stride + x) * self.bytes_per_pixel;
        if offset + self.bytes_per_pixel <= self.framebuffer.len() {
            let b = self.framebuffer[offset];
            let g = self.framebuffer[offset + 1];
            let r = self.framebuffer[offset + 2];
            return 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
        }
        0
    }

    fn write_to_buffer(&mut self, x: usize, y: usize, bytes: &[u8]) -> bool {
        let offset = (y * self.stride + x) * self.bytes_per_pixel;
        
        if self.use_back_buffer && !self.back_buffer.is_empty() {
            if offset + self.bytes_per_pixel <= self.back_buffer.len() {
                self.back_buffer[offset..offset + self.bytes_per_pixel].copy_from_slice(&bytes[..self.bytes_per_pixel]);
                return true;
            }
        } else {
            if offset + self.bytes_per_pixel <= self.framebuffer.len() {
                self.framebuffer[offset..offset + self.bytes_per_pixel].copy_from_slice(&bytes[..self.bytes_per_pixel]);
                return true;
            }
        }
        false
    }

    /// Fast clear of back buffer using unsafe memset
    pub fn clear_buffer(&mut self, color: u32) {
        if self.use_back_buffer && !self.back_buffer.is_empty() {
            let bytes = color_to_bytes(color, self.pixel_format).unwrap_or([0, 0, 0, 255]);
            let bpp = self.bytes_per_pixel;
            
            // For black (most common), use fast memset
            if color == 0xFF000000 {
                unsafe {
                    core::ptr::write_bytes(self.back_buffer.as_mut_ptr(), 0, self.back_buffer.len());
                }
            } else {
                // Fill with color pattern
                for chunk in self.back_buffer.chunks_exact_mut(bpp) {
                    chunk.copy_from_slice(&bytes[..bpp]);
                }
            }
        }
    }

    pub fn clear_screen(&mut self, color: u32) {        
        for y in 0..self.height {
            for x in 0..self.width {
                self.write_pixel(x, y, color);
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
                    if let Some(&pixel) = bitmap.pixels.get(pixel_index) {
                        self.write_pixel(screen_x, screen_y, pixel);
                    }
                }
            }
        }
    }
}
