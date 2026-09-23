use super::bitmap::Bitmap;
use super::font::{glyph, GLYPH_HEIGHT, GLYPH_WIDTH};
use super::rect::Rect;

/// Number of fractional bits used by the `*_subpixel` functions (1/16 pixel precision).
pub const SUBPIXEL_BITS: i32 = 4;
const SUBPIXEL_ONE: i64 = 1 << SUBPIXEL_BITS;

/// Fills a slice of pixels with one value using `rep stosd`, which is much faster than a
/// per-pixel loop on a target without SIMD.
#[inline]
pub fn fill_pixels(pixels: &mut [u32], color: u32) {
    unsafe {
        core::arch::asm!(
            "rep stosd",
            inout("rcx") pixels.len() => _,
            inout("rdi") pixels.as_mut_ptr() => _,
            in("eax") color,
            options(nostack, preserves_flags)
        );
    }
}

/// Mixes `source` over `destination` (both 0xAARRGGBB) with an opacity of 0..=255.
#[inline]
pub fn blend(destination: u32, source: u32, alpha: u32) -> u32 {
    let alpha = alpha + (alpha >> 7); // 0..=256 so the division below is a shift
    let inverse = 256 - alpha;
    let red_blue = ((source & 0x00FF_00FF) * alpha + (destination & 0x00FF_00FF) * inverse) >> 8;
    let green = ((source & 0x0000_FF00) * alpha + (destination & 0x0000_FF00) * inverse) >> 8;
    0xFF00_0000 | (red_blue & 0x00FF_00FF) | (green & 0x0000_FF00)
}

/// Linear interpolation between two colors, `amount` in 0..=255.
pub fn mix_colors(from: u32, to: u32, amount: u32) -> u32 {
    blend(from, to, amount.min(255))
}

#[allow(dead_code)] // A drawing API; not every primitive has a caller yet.
/// Drawing operations shared by the screen (`Display`) and off-screen surfaces (`Bitmap`).
/// Colors are 0xAARRGGBB. `fill_*` calls ignore alpha; `blend_*` calls and bitmap blits honour it.
pub trait Canvas {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    /// Row-major pixels, `width()` pixels per row.
    fn pixels(&self) -> &[u32];
    fn pixels_mut(&mut self) -> &mut [u32];
    /// Called with every area that was drawn to.
    fn mark_dirty(&mut self, _area: Rect) {}

    fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width() as i32, self.height() as i32)
    }

    fn clear(&mut self, color: u32) {
        fill_pixels(self.pixels_mut(), color);
        let bounds = self.bounds();
        self.mark_dirty(bounds);
    }

    fn put_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.width() && (y as usize) < self.height() {
            let width = self.width();
            self.pixels_mut()[y as usize * width + x as usize] = color;
            self.mark_dirty(Rect::new(x, y, 1, 1));
        }
    }

    fn blend_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.width() && (y as usize) < self.height() {
            let width = self.width();
            let pixel = &mut self.pixels_mut()[y as usize * width + x as usize];
            *pixel = blend(*pixel, color, color >> 24);
            self.mark_dirty(Rect::new(x, y, 1, 1));
        }
    }

    fn fill_rect(&mut self, rect: Rect, color: u32) {
        let Some(area) = rect.intersect(self.bounds()) else { return };
        let width = self.width();
        let pixels = self.pixels_mut();
        for y in area.y..area.bottom() {
            let start = y as usize * width + area.x as usize;
            fill_pixels(&mut pixels[start..start + area.width as usize], color);
        }
        self.mark_dirty(area);
    }

    fn blend_rect(&mut self, rect: Rect, color: u32) {
        let alpha = color >> 24;
        if alpha == 0xFF {
            return self.fill_rect(rect, color);
        }
        let Some(area) = rect.intersect(self.bounds()) else { return };
        let width = self.width();
        let pixels = self.pixels_mut();
        for y in area.y..area.bottom() {
            let start = y as usize * width + area.x as usize;
            for pixel in &mut pixels[start..start + area.width as usize] {
                *pixel = blend(*pixel, color, alpha);
            }
        }
        self.mark_dirty(area);
    }

    fn draw_rect_outline(&mut self, rect: Rect, thickness: i32, color: u32) {
        self.fill_rect(Rect::new(rect.x, rect.y, rect.width, thickness), color);
        self.fill_rect(Rect::new(rect.x, rect.bottom() - thickness, rect.width, thickness), color);
        self.fill_rect(Rect::new(rect.x, rect.y, thickness, rect.height), color);
        self.fill_rect(Rect::new(rect.right() - thickness, rect.y, thickness, rect.height), color);
    }

    /// Top-to-bottom gradient between two colors.
    fn fill_vertical_gradient(&mut self, rect: Rect, top: u32, bottom: u32) {
        let Some(area) = rect.intersect(self.bounds()) else { return };
        let width = self.width();
        let span = (rect.height - 1).max(1);
        let pixels = self.pixels_mut();
        for y in area.y..area.bottom() {
            let amount = ((y - rect.y) * 255 / span) as u32;
            let start = y as usize * width + area.x as usize;
            fill_pixels(&mut pixels[start..start + area.width as usize], mix_colors(top, bottom, amount));
        }
        self.mark_dirty(area);
    }

    /// Anti-aliased filled circle. Alpha of `color` sets the opacity.
    fn fill_circle(&mut self, center_x: i32, center_y: i32, radius: i32, color: u32) {
        let one = SUBPIXEL_ONE as i32;
        self.fill_circle_subpixel(center_x * one, center_y * one, radius * one, color);
    }

    /// Anti-aliased filled circle with coordinates in 1/16 pixel units, for smooth motion.
    fn fill_circle_subpixel(&mut self, center_x: i32, center_y: i32, radius: i32, color: u32) {
        if radius <= 0 {
            return;
        }
        let bounding_box = Rect::new(
            (center_x - radius) >> SUBPIXEL_BITS,
            (center_y - radius) >> SUBPIXEL_BITS,
            ((radius * 2) >> SUBPIXEL_BITS) + 2,
            ((radius * 2) >> SUBPIXEL_BITS) + 2,
        );
        let Some(area) = bounding_box.intersect(self.bounds()) else { return };

        let color_alpha = color >> 24;
        let radius = radius as i64;
        let inner_squared = (radius - SUBPIXEL_ONE).max(0).pow(2);
        let outer_squared = (radius + SUBPIXEL_ONE).pow(2);
        let width = self.width();
        let pixels = self.pixels_mut();

        for y in area.y..area.bottom() {
            let delta_y = (y as i64 * SUBPIXEL_ONE + SUBPIXEL_ONE / 2) - center_y as i64;
            let row = y as usize * width;
            for x in area.x..area.right() {
                let delta_x = (x as i64 * SUBPIXEL_ONE + SUBPIXEL_ONE / 2) - center_x as i64;
                let distance_squared = delta_x * delta_x + delta_y * delta_y;
                if distance_squared >= outer_squared {
                    continue;
                }

                // Coverage is how far the pixel centre sits inside the edge, clamped to one pixel.
                let coverage = if distance_squared <= inner_squared {
                    256
                } else {
                    let edge_distance = (distance_squared - radius * radius) / (2 * radius);
                    (128 - edge_distance * 256 / SUBPIXEL_ONE).clamp(0, 256)
                };
                if coverage == 0 {
                    continue;
                }

                let pixel = &mut pixels[row + x as usize];
                *pixel = blend(*pixel, color, color_alpha * coverage as u32 / 256);
            }
        }
        self.mark_dirty(area);
    }

    fn draw_line(&mut self, from_x: i32, from_y: i32, to_x: i32, to_y: i32, color: u32) {
        let delta_x = (to_x - from_x).abs();
        let delta_y = -(to_y - from_y).abs();
        let step_x = if from_x < to_x { 1 } else { -1 };
        let step_y = if from_y < to_y { 1 } else { -1 };
        let mut error = delta_x + delta_y;
        let (mut x, mut y) = (from_x, from_y);
        loop {
            self.blend_pixel(x, y, color);
            if x == to_x && y == to_y {
                break;
            }
            let doubled = error * 2;
            if doubled >= delta_y {
                error += delta_y;
                x += step_x;
            }
            if doubled <= delta_x {
                error += delta_x;
                y += step_y;
            }
        }
    }

    /// Copies a bitmap as-is (no transparency), the fastest way to draw images and backgrounds.
    fn blit(&mut self, source: &Bitmap, x: i32, y: i32) {
        self.blit_region(source, source.bounds(), x, y);
    }

    /// Copies `source_area` of a bitmap to (`x`, `y`) without transparency.
    fn blit_region(&mut self, source: &Bitmap, source_area: Rect, x: i32, y: i32) {
        let Some(source_area) = source_area.intersect(source.bounds()) else { return };
        let target = Rect::new(x, y, source_area.width, source_area.height);
        let Some(area) = target.intersect(self.bounds()) else { return };

        let source_x = (source_area.x + area.x - x) as usize;
        let source_y = source_area.y + area.y - y;
        let width = self.width();
        let pixels = self.pixels_mut();
        for row in 0..area.height {
            let from = (source_y + row) as usize * source.width + source_x;
            let to = (area.y + row) as usize * width + area.x as usize;
            pixels[to..to + area.width as usize].copy_from_slice(&source.pixels[from..from + area.width as usize]);
        }
        self.mark_dirty(area);
    }

    /// Draws a bitmap honouring its per-pixel alpha (0 = transparent, 255 = opaque).
    fn blit_blended(&mut self, source: &Bitmap, x: i32, y: i32) {
        let target = Rect::new(x, y, source.width as i32, source.height as i32);
        let Some(area) = target.intersect(self.bounds()) else { return };
        let width = self.width();
        let pixels = self.pixels_mut();
        for row in area.y..area.bottom() {
            for column in area.x..area.right() {
                let source_pixel = source.pixels[(row - y) as usize * source.width + (column - x) as usize];
                let alpha = source_pixel >> 24;
                if alpha != 0 {
                    let pixel = &mut pixels[row as usize * width + column as usize];
                    *pixel = blend(*pixel, source_pixel, alpha);
                }
            }
        }
        self.mark_dirty(area);
    }

    /// Draws text with the built-in 8x8 font, each font pixel becoming a `scale` x `scale` block.
    /// Returns the width of the drawn text in pixels.
    fn draw_text(&mut self, x: i32, y: i32, text: &str, color: u32, scale: i32) -> i32 {
        let mut cursor_x = x;
        for character in text.chars() {
            self.draw_char(cursor_x, y, character, color, scale);
            cursor_x += GLYPH_WIDTH as i32 * scale;
        }
        cursor_x - x
    }

    fn draw_char(&mut self, x: i32, y: i32, character: char, color: u32, scale: i32) {
        let rows = glyph(character);
        for (row, bits) in rows.iter().enumerate().take(GLYPH_HEIGHT) {
            for column in 0..GLYPH_WIDTH {
                if (bits >> (7 - column)) & 1 == 1 {
                    let block = Rect::new(x + column as i32 * scale, y + row as i32 * scale, scale, scale);
                    self.blend_rect(block, color);
                }
            }
        }
    }

    /// Moves the whole image up by `distance` rows and fills the uncovered rows with `fill`.
    fn scroll_up(&mut self, distance: usize, fill: u32) {
        let width = self.width();
        let height = self.height();
        let distance = distance.min(height);
        let pixels = self.pixels_mut();
        pixels.copy_within(distance * width.., 0);
        fill_pixels(&mut pixels[(height - distance) * width..], fill);
        let bounds = self.bounds();
        self.mark_dirty(bounds);
    }
}
