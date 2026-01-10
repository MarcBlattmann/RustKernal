use alloc::vec::Vec;

pub struct Bitmap {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}
