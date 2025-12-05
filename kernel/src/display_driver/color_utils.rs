use bootloader_api::info::PixelFormat;

pub fn color_to_bytes(hex: u32, format: PixelFormat) -> Option<[u8; 4]> {
    let (alpha, red, green, blue) = if hex > 0x00FF_FFFF {
        (
            ((hex >> 24) & 0xFF) as u8,
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    } else {
        (
            0xFFu8,
            ((hex >> 16) & 0xFF) as u8,
            ((hex >> 8) & 0xFF) as u8,
            (hex & 0xFF) as u8,
        )
    };

    match format {
        PixelFormat::Rgb => Some([red, green, blue, alpha]),
        PixelFormat::Bgr => Some([blue, green, red, alpha]),
        PixelFormat::U8 => {
            let gray = ((red as u16 + green as u16 + blue as u16) / 3) as u8;
            Some([gray, 0, 0, alpha])
        },
        PixelFormat::Unknown { red_position, green_position, blue_position } => {
            let r = (hex >> red_position) as u8;
            let g = (hex >> green_position) as u8;
            let b = (hex >> blue_position) as u8;
            Some([r, g, b, alpha])
        },
        _ => Some([red, green, blue, alpha]),
    }
}