use bootloader_api::info::PixelFormat;

pub struct Display {
    pub width: usize,
    pub height: usize,
    pub framebuffer: &'static mut [u8],
    pub color_format: PixelFormat,
}

impl Display {
    pub fn new(
        width: usize,
        height: usize,
        framebuffer: &'static mut [u8],
        color_format: PixelFormat,
    ) -> Self {
        Self {
            width,
            height,
            framebuffer,
            color_format,
        }
    }
}