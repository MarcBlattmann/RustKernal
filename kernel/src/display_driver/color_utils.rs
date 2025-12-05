use bootloader_api::info::PixelFormat;

pub fn color_to_bytes(hex: u32, format: PixelFormat) -> Option<[u8; 4]> {

    let red = ((hex >> 16) & 0xFF) as u8;
    let green = ((hex >> 8) & 0xFF) as u8;
    let blue = (hex & 0xFF) as u8;

    match format {
        PixelFormat::Rgb => Some([red, green, blue, 0]),
        PixelFormat::Bgr => Some([blue, green, red, 0]),
        PixelFormat::U8 => {
            let gray = ((red as u16 + green as u16 + blue as u16) / 3) as u8;
            return Some([gray, 0, 0, 0]);
        },
        PixelFormat::Unknown { red_position, green_position, blue_position } => {
            let r = ((hex >> red_position) & 0xFF) as u8;
            let g = ((hex >> green_position) & 0xFF) as u8;
            let b = ((hex >> blue_position) & 0xFF) as u8;
            return Some([r, g, b, 0]);
        },
        _ => Some([red, green, blue, 0]),
    }
}