use bootloader_api::info::PixelFormat;
use crate::tools::digit_count;

pub fn color_to_pixel_bytes(hex: u32, format: PixelFormat) -> [u8; 4] {
    let mut alpha = 255;
    let mut red = 0;
    let mut green = 0;
    let mut blue = 0;

    if digit_count(hex as u64) == 6  {
        red = ((hex >> 16) & 0xFF) as u8;
        green = ((hex >> 8) & 0xFF) as u8;
        blue = (hex & 0xFF) as u8;
    } else if digit_count(hex as u64) == 8 {
        alpha = ((hex >> 24) & 0xFF) as u8;
        red = ((hex >> 16) & 0xFF) as u8;
        green = ((hex >> 8) & 0xFF) as u8;
        blue = (hex & 0xFF) as u8;
    } else {
        return [red, green, blue, alpha];
    }

    match format {
        PixelFormat::Rgb => [red, green, blue, 0],
        PixelFormat::Bgr => [blue, green, red, 0],
        PixelFormat::U8 => {
            let gray = ((red as u16 + green as u16 + blue as u16) / 3) as u8;
            [gray, 0, 0, 0]
        },
        PixelFormat::Unknown { red_position, green_position, blue_position } => {
            let r = ((hex >> red_position) & 0xFF) as u8;
            let g = ((hex >> green_position) & 0xFF) as u8;
            let b = ((hex >> blue_position) & 0xFF) as u8;
            return [r, g, b, 0];
        },
        _ => [red, green, blue, 0],
    }
}