use bootloader_api::info::PixelFormat;

pub fn color_to_bytes(hex: u32, format: PixelFormat) -> Option<[u8; 4]> {
    let alpha = ((hex >> 24) & 0xFF) as u8;
    let red = ((hex >> 16) & 0xFF) as u8;
    let green = ((hex >> 8) & 0xFF) as u8;
    let blue = (hex & 0xFF) as u8;
    
    let alpha = if alpha == 0 && (red | green | blue) != 0 { 0xFF } else { alpha };

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
