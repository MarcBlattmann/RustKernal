use alloc::vec;
use alloc::vec::Vec;
use super::canvas::Canvas;

/// An off-screen image in 0xAARRGGBB pixels. It supports every `Canvas` drawing operation, so
/// scenes can be prepared once and then blitted to the display each frame.
pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

impl Bitmap {
    pub fn new(width: usize, height: usize, color: u32) -> Self {
        Self { width, height, pixels: vec![color; width * height] }
    }
}

impl Canvas for Bitmap {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }
}
