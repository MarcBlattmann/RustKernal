use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub fn right(&self) -> i32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub fn intersect(&self, other: Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let rect = Rect::new(x, y, self.right().min(other.right()) - x, self.bottom().min(other.bottom()) - y);
        if rect.is_empty() { None } else { Some(rect) }
    }

    pub fn union(&self, other: Rect) -> Rect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Rect::new(x, y, self.right().max(other.right()) - x, self.bottom().max(other.bottom()) - y)
    }

    fn area(&self) -> i64 {
        self.width as i64 * self.height as i64
    }
}

const MAX_DIRTY_RECTS: usize = 32;

/// The set of screen areas that changed since they were last copied to video memory.
#[derive(Clone, Default)]
pub struct DirtyRegion {
    rects: Vec<Rect>,
}

impl DirtyRegion {
    pub fn add(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }

        // Merge with an overlapping rect when that does not waste much area.
        for existing in self.rects.iter_mut() {
            let merged = existing.union(rect);
            if merged.area() <= existing.area() + rect.area() {
                *existing = merged;
                return;
            }
        }

        if self.rects.len() == MAX_DIRTY_RECTS {
            let bounds = self.rects.iter().fold(rect, |bounds, existing| bounds.union(*existing));
            self.rects.clear();
            self.rects.push(bounds);
        } else {
            self.rects.push(rect);
        }
    }

    pub fn add_region(&mut self, other: &DirtyRegion) {
        for rect in &other.rects {
            self.add(*rect);
        }
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }
}
