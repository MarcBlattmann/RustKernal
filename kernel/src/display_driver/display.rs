use bootloader_api::info::PixelFormat;

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
}