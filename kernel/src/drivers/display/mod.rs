//! Display driver.
//!
//! Everything is drawn into a back buffer in RAM through the `Canvas` trait. `Display::present`
//! then puts the finished frame on screen, copying only the areas that changed:
//!
//! * On QEMU / Bochs / VirtualBox the `bga` driver sets a high resolution and keeps two pages in
//!   video memory. The frame is copied into the hidden page and the pages are swapped, so a
//!   half-drawn frame is never visible (no tearing or flicker).
//! * Everywhere else the framebuffer set up by the bootloader is used, converting to its pixel
//!   format when needed.
//!
//! `FrameClock` paces a render loop to a steady frame rate such as 60 fps.

pub mod bga;
pub mod bitmap;
pub mod canvas;
pub mod font;
pub mod frame_clock;
pub mod rect;

pub use bitmap::Bitmap;
pub use canvas::Canvas;
pub use frame_clock::FrameClock;
pub use rect::Rect;

use alloc::vec;
use alloc::vec::Vec;
use bga::{BgaDevice, BgaMode};
use bootloader_api::info::{FrameBuffer, PixelFormat};
use rect::DirtyRegion;

/// Resolutions tried on the BGA adapter, best first. The first one with room for two pages wins.
const PREFERRED_MODES: [(usize, usize); 4] = [(1920, 1080), (1600, 900), (1280, 720), (1024, 768)];

pub struct Display {
    width: usize,
    height: usize,
    back_buffer: Vec<u32>,
    dirty: DirtyRegion,
    output: Output,
}

enum Output {
    PageFlipping {
        device: BgaDevice,
        mode: BgaMode,
        visible_page: usize,
        /// Per video memory page, the areas where it is behind the back buffer.
        stale: [DirtyRegion; 2],
    },
    Linear(LinearFramebuffer),
}

/// A framebuffer that is shown directly, in whatever pixel format the firmware chose.
struct LinearFramebuffer {
    base: *mut u8,
    byte_length: usize,
    stride: usize,
    bytes_per_pixel: usize,
    format: PixelFormat,
}

/// Picks the best available output. `framebuffer` is the bootloader's and is used as a fallback.
pub fn init(framebuffer: Option<FrameBuffer>) -> Display {
    if let Some(display) = init_page_flipping() {
        return display;
    }

    let framebuffer = framebuffer.expect("no display available");
    let info = framebuffer.info();
    let buffer = framebuffer.into_buffer();
    crate::serial_println!(
        "display: bootloader framebuffer {}x{}, {} bytes per pixel, {:?}",
        info.width,
        info.height,
        info.bytes_per_pixel,
        info.pixel_format
    );
    Display::new(
        info.width,
        info.height,
        Output::Linear(LinearFramebuffer {
            base: buffer.as_mut_ptr(),
            byte_length: buffer.len(),
            stride: info.stride,
            bytes_per_pixel: info.bytes_per_pixel,
            format: info.pixel_format,
        }),
    )
}

fn init_page_flipping() -> Option<Display> {
    let mut device = BgaDevice::detect()?;
    for (width, height) in PREFERRED_MODES {
        if device.video_memory_size() < width * height * 4 * 2 {
            continue;
        }
        let Some(mode) = device.set_mode(width, height) else { continue };
        if mode.pages < 2 {
            continue;
        }

        crate::serial_println!("display: bga {}x{}, page flipping", width, height);
        let full_screen = Rect::new(0, 0, width as i32, height as i32);
        let mut stale = [DirtyRegion::default(), DirtyRegion::default()];
        stale.iter_mut().for_each(|page| page.add(full_screen));
        return Some(Display::new(width, height, Output::PageFlipping { device, mode, visible_page: 0, stale }));
    }
    crate::serial_println!("display: bga found but no usable double-buffered mode");
    None
}

impl Display {
    fn new(width: usize, height: usize, output: Output) -> Self {
        let mut display = Self { width, height, back_buffer: vec![0xFF00_0000; width * height], dirty: DirtyRegion::default(), output };
        display.dirty.add(display.bounds());
        display
    }

    /// Short human readable description of the active output, e.g. for an on-screen overlay.
    pub fn output_name(&self) -> &'static str {
        match self.output {
            Output::PageFlipping { .. } => "BGA page flipping",
            Output::Linear(_) => "bootloader framebuffer",
        }
    }

    /// Shows everything drawn since the last call.
    pub fn present(&mut self) {
        match &mut self.output {
            Output::PageFlipping { device, mode, visible_page, stale } => {
                for page in stale.iter_mut() {
                    page.add_region(&self.dirty);
                }
                let hidden_page = 1 - *visible_page;
                if stale[hidden_page].is_empty() {
                    return;
                }

                let video_page = device.page(hidden_page, mode);
                for rect in stale[hidden_page].rects() {
                    copy_rect(&self.back_buffer, video_page, self.width, *rect);
                }
                stale[hidden_page].clear();
                device.show_page(hidden_page, mode);
                *visible_page = hidden_page;
            }
            Output::Linear(framebuffer) => {
                for rect in self.dirty.rects() {
                    framebuffer.write_rect(&self.back_buffer, self.width, *rect);
                }
            }
        }
        self.dirty.clear();
    }

    /// Returns a copy of the back buffer, e.g. to restore the screen after a full-screen app.
    pub fn snapshot(&self) -> Bitmap {
        Bitmap { width: self.width, height: self.height, pixels: self.back_buffer.clone() }
    }
}

impl Canvas for Display {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn pixels(&self) -> &[u32] {
        &self.back_buffer
    }

    fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.back_buffer
    }

    fn mark_dirty(&mut self, area: Rect) {
        self.dirty.add(area);
    }
}

fn copy_rect(source: &[u32], destination: &mut [u32], width: usize, rect: Rect) {
    for y in rect.y as usize..rect.bottom() as usize {
        let start = y * width + rect.x as usize;
        let end = start + rect.width as usize;
        destination[start..end].copy_from_slice(&source[start..end]);
    }
}

impl LinearFramebuffer {
    fn write_rect(&mut self, back_buffer: &[u32], width: usize, rect: Rect) {
        for y in rect.y as usize..rect.bottom() as usize {
            let source = &back_buffer[y * width + rect.x as usize..][..rect.width as usize];
            let offset = (y * self.stride + rect.x as usize) * self.bytes_per_pixel;
            if offset + source.len() * self.bytes_per_pixel > self.byte_length {
                return;
            }
            let row = unsafe { self.base.add(offset) };

            // 0xAARRGGBB stored little-endian is already B, G, R, A in memory.
            if self.bytes_per_pixel == 4 && self.format == PixelFormat::Bgr {
                unsafe { core::ptr::copy_nonoverlapping(source.as_ptr(), row as *mut u32, source.len()) };
                continue;
            }

            for (index, &color) in source.iter().enumerate() {
                let bytes = self.native_bytes(color);
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), row.add(index * self.bytes_per_pixel), self.bytes_per_pixel.min(4));
                }
            }
        }
    }

    fn native_bytes(&self, color: u32) -> [u8; 4] {
        let red = (color >> 16) as u8;
        let green = (color >> 8) as u8;
        let blue = color as u8;
        match self.format {
            PixelFormat::Rgb => [red, green, blue, 0],
            PixelFormat::U8 => [((red as u16 + green as u16 + blue as u16) / 3) as u8, 0, 0, 0],
            PixelFormat::Unknown { red_position, green_position, blue_position } => {
                let native = (red as u32) << red_position | (green as u32) << green_position | (blue as u32) << blue_position;
                native.to_le_bytes()
            }
            _ => [blue, green, red, 0],
        }
    }
}
