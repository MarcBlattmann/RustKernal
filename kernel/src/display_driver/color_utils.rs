use bootloader_api::info::PixelFormat;
use crate::tools::digit_count;

pub fn color_to_pixel_bytes(hex: u32, format: PixelFormat) {
    let mut alpha = 255;
    let mut red = 0;
    let mut green = 0;
    let mut blue = 0;

    if (digit_count(hex as u64) == 6) {
        red = ((hex >> 16) & 0xFF) as u8;
        green = ((hex >> 8) & 0xFF) as u8;
        blue = (hex & 0xFF) as u8;
    } else if (digit_count(hex as u64) == 8) {
        alpha = ((hex >> 24) & 0xFF) as u8;
        red = ((hex >> 16) & 0xFF) as u8;
        green = ((hex >> 8) & 0xFF) as u8;
        blue = (hex & 0xFF) as u8;
    } else {
        return;
    }
}