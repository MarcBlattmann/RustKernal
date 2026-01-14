//! Dirty Rectangle System - Only redraw what changed
//!
//! Instead of clearing the whole screen, we track which rectangles
//! need to be redrawn and only update those areas.

use alloc::vec::Vec;
use super::widgets::Rect;

/// Maximum dirty rectangles before we just do a full redraw
const MAX_DIRTY_RECTS: usize = 16;

/// Dirty rectangle manager
pub struct DirtyRegions {
    rects: Vec<Rect>,
    full_redraw: bool,
}

impl DirtyRegions {
    pub fn new() -> Self {
        Self {
            rects: Vec::with_capacity(MAX_DIRTY_RECTS),
            full_redraw: true, // Start with full redraw
        }
    }
    
    /// Mark entire screen dirty (full redraw)
    pub fn mark_full(&mut self) {
        self.full_redraw = true;
        self.rects.clear();
    }
    
    /// Mark a rectangle as dirty
    pub fn mark_rect(&mut self, rect: Rect) {
        if self.full_redraw {
            return; // Already doing full redraw
        }
        
        // If too many rects, switch to full redraw
        if self.rects.len() >= MAX_DIRTY_RECTS {
            self.mark_full();
            return;
        }
        
        // Add to dirty list (could merge overlapping rects for optimization)
        self.rects.push(rect);
    }
    
    /// Mark old and new position of a moved window
    pub fn mark_window_move(&mut self, old_bounds: Rect, new_bounds: Rect) {
        self.mark_rect(old_bounds);
        self.mark_rect(new_bounds);
    }
    
    /// Check if full redraw is needed
    pub fn needs_full_redraw(&self) -> bool {
        self.full_redraw
    }
    
    /// Get dirty rectangles
    pub fn get_rects(&self) -> &[Rect] {
        &self.rects
    }
    
    /// Clear dirty state after redraw
    pub fn clear(&mut self) {
        self.rects.clear();
        self.full_redraw = false;
    }
    
    /// Check if anything needs redrawing
    pub fn is_dirty(&self) -> bool {
        self.full_redraw || !self.rects.is_empty()
    }
}
