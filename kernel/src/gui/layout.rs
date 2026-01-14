//! Layout System - Flexbox-like automatic positioning
//!
//! Example:
//! ```xml
//! <app title="My App" width="300" height="200">
//!     <vbox gap="10" padding="20">
//!         <label>Title</label>
//!         <hbox gap="5">
//!             <button grow="1">OK</button>
//!             <button grow="1">Cancel</button>
//!         </hbox>
//!     </vbox>
//! </app>
//! ```

use alloc::string::String;
use alloc::vec::Vec;

/// Layout direction
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    Horizontal, // hbox - left to right
    Vertical,   // vbox - top to bottom
}

/// Alignment within container
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

/// A layout node that can contain children
#[derive(Clone, Debug)]
pub struct LayoutNode {
    pub kind: NodeKind,
    pub children: Vec<LayoutNode>,
    
    // Sizing hints
    pub width: SizeHint,
    pub height: SizeHint,
    pub grow: f32,      // Flex grow factor (0 = don't grow)
    pub shrink: f32,    // Flex shrink factor
    
    // Computed position (filled in by layout pass)
    pub computed_x: i32,
    pub computed_y: i32,
    pub computed_width: usize,
    pub computed_height: usize,
}

/// Size hint for layout
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SizeHint {
    Auto,           // Size to content
    Fixed(usize),   // Fixed pixel size
    Percent(f32),   // Percentage of parent
    Fill,           // Fill remaining space
}

/// What kind of node is this
#[derive(Clone, Debug)]
pub enum NodeKind {
    /// Container with direction
    Container {
        direction: Direction,
        gap: usize,
        padding: usize,
        align: Align,
        cross_align: Align,
    },
    /// Label element
    Label { text: String },
    /// Button element  
    Button { text: String },
    /// TextBox element
    TextBox,
    /// Panel/spacer
    Panel,
    /// Spacer - takes remaining space
    Spacer,
}

impl LayoutNode {
    /// Create a vertical box container
    pub fn vbox() -> Self {
        Self {
            kind: NodeKind::Container {
                direction: Direction::Vertical,
                gap: 0,
                padding: 0,
                align: Align::Stretch,
                cross_align: Align::Start,
            },
            children: Vec::new(),
            width: SizeHint::Fill,
            height: SizeHint::Fill,
            grow: 1.0,
            shrink: 1.0,
            computed_x: 0,
            computed_y: 0,
            computed_width: 0,
            computed_height: 0,
        }
    }
    
    /// Create a horizontal box container
    pub fn hbox() -> Self {
        Self {
            kind: NodeKind::Container {
                direction: Direction::Horizontal,
                gap: 0,
                padding: 0,
                align: Align::Stretch,
                cross_align: Align::Start,
            },
            children: Vec::new(),
            width: SizeHint::Fill,
            height: SizeHint::Auto,
            grow: 0.0,
            shrink: 1.0,
            computed_x: 0,
            computed_y: 0,
            computed_width: 0,
            computed_height: 0,
        }
    }
    
    /// Create a label
    pub fn label(text: &str) -> Self {
        Self {
            kind: NodeKind::Label { text: String::from(text) },
            children: Vec::new(),
            width: SizeHint::Auto,
            height: SizeHint::Auto,
            grow: 0.0,
            shrink: 0.0,
            computed_x: 0,
            computed_y: 0,
            computed_width: 0,
            computed_height: 0,
        }
    }
    
    /// Create a button
    pub fn button(text: &str) -> Self {
        Self {
            kind: NodeKind::Button { text: String::from(text) },
            children: Vec::new(),
            width: SizeHint::Auto,
            height: SizeHint::Fixed(30),
            grow: 0.0,
            shrink: 0.0,
            computed_x: 0,
            computed_y: 0,
            computed_width: 0,
            computed_height: 0,
        }
    }
    
    /// Create a textbox
    pub fn textbox() -> Self {
        Self {
            kind: NodeKind::TextBox,
            children: Vec::new(),
            width: SizeHint::Fill,
            height: SizeHint::Fixed(28),
            grow: 0.0,
            shrink: 0.0,
            computed_x: 0,
            computed_y: 0,
            computed_width: 0,
            computed_height: 0,
        }
    }
    
    /// Create a spacer that fills available space
    pub fn spacer() -> Self {
        Self {
            kind: NodeKind::Spacer,
            children: Vec::new(),
            width: SizeHint::Auto,
            height: SizeHint::Auto,
            grow: 1.0,
            shrink: 1.0,
            computed_x: 0,
            computed_y: 0,
            computed_width: 0,
            computed_height: 0,
        }
    }
    
    // Builder methods
    
    /// Set gap between children
    pub fn gap(mut self, gap: usize) -> Self {
        if let NodeKind::Container { gap: ref mut g, .. } = self.kind {
            *g = gap;
        }
        self
    }
    
    /// Set padding inside container
    pub fn padding(mut self, padding: usize) -> Self {
        if let NodeKind::Container { padding: ref mut p, .. } = self.kind {
            *p = padding;
        }
        self
    }
    
    /// Set alignment
    pub fn align(mut self, align: Align) -> Self {
        if let NodeKind::Container { align: ref mut a, .. } = self.kind {
            *a = align;
        }
        self
    }
    
    /// Set cross-axis alignment
    pub fn cross_align(mut self, align: Align) -> Self {
        if let NodeKind::Container { cross_align: ref mut a, .. } = self.kind {
            *a = align;
        }
        self
    }
    
    /// Set grow factor
    pub fn grow(mut self, factor: f32) -> Self {
        self.grow = factor;
        self
    }
    
    /// Set fixed width
    pub fn width(mut self, w: usize) -> Self {
        self.width = SizeHint::Fixed(w);
        self
    }
    
    /// Set fixed height
    pub fn height(mut self, h: usize) -> Self {
        self.height = SizeHint::Fixed(h);
        self
    }
    
    /// Fill available width
    pub fn fill_width(mut self) -> Self {
        self.width = SizeHint::Fill;
        self
    }
    
    /// Fill available height
    pub fn fill_height(mut self) -> Self {
        self.height = SizeHint::Fill;
        self
    }
    
    /// Add a child node
    pub fn child(mut self, node: LayoutNode) -> Self {
        self.children.push(node);
        self
    }
    
    /// Add multiple children
    pub fn children(mut self, nodes: Vec<LayoutNode>) -> Self {
        self.children.extend(nodes);
        self
    }
    
    /// Calculate the minimum content size
    pub fn min_content_size(&self) -> (usize, usize) {
        match &self.kind {
            NodeKind::Label { text } => {
                // Approximate: 8 pixels per char, 16 pixels height
                (text.len() * 8, 16)
            }
            NodeKind::Button { text } => {
                // Button with padding
                (text.len() * 8 + 16, 30)
            }
            NodeKind::TextBox => (100, 28),
            NodeKind::Panel => (0, 0),
            NodeKind::Spacer => (0, 0),
            NodeKind::Container { direction, gap, padding, .. } => {
                let mut total_w: usize = 0;
                let mut total_h: usize = 0;
                let mut max_w: usize = 0;
                let mut max_h: usize = 0;
                
                for (i, child) in self.children.iter().enumerate() {
                    let (cw, ch) = child.min_content_size();
                    max_w = max_w.max(cw);
                    max_h = max_h.max(ch);
                    
                    if *direction == Direction::Horizontal {
                        total_w += cw;
                        if i > 0 { total_w += *gap; }
                        total_h = total_h.max(ch);
                    } else {
                        total_h += ch;
                        if i > 0 { total_h += *gap; }
                        total_w = total_w.max(cw);
                    }
                }
                
                (total_w + padding * 2, total_h + padding * 2)
            }
        }
    }
    
    /// Perform layout calculation
    pub fn layout(&mut self, x: i32, y: i32, available_width: usize, available_height: usize) {
        self.computed_x = x;
        self.computed_y = y;
        
        // Determine our own size
        self.computed_width = match self.width {
            SizeHint::Fixed(w) => w,
            SizeHint::Auto => self.min_content_size().0,
            SizeHint::Fill => available_width,
            SizeHint::Percent(p) => ((available_width as f32) * p / 100.0) as usize,
        };
        
        self.computed_height = match self.height {
            SizeHint::Fixed(h) => h,
            SizeHint::Auto => self.min_content_size().1,
            SizeHint::Fill => available_height,
            SizeHint::Percent(p) => ((available_height as f32) * p / 100.0) as usize,
        };
        
        // Layout children if container
        if let NodeKind::Container { direction, gap, padding, align, cross_align } = self.kind {
            let content_x = x + padding as i32;
            let content_y = y + padding as i32;
            let content_w = self.computed_width.saturating_sub(padding * 2);
            let content_h = self.computed_height.saturating_sub(padding * 2);
            
            if self.children.is_empty() {
                return;
            }
            
            // Calculate total fixed size and grow factors
            let mut total_fixed: usize = 0;
            let mut total_grow: f32 = 0.0;
            let num_gaps = if self.children.len() > 1 { self.children.len() - 1 } else { 0 };
            let gap_space = num_gaps * gap;
            
            for child in &self.children {
                let (min_w, min_h) = child.min_content_size();
                if direction == Direction::Horizontal {
                    if child.grow == 0.0 {
                        total_fixed += match child.width {
                            SizeHint::Fixed(w) => w,
                            _ => min_w,
                        };
                    }
                } else {
                    if child.grow == 0.0 {
                        total_fixed += match child.height {
                            SizeHint::Fixed(h) => h,
                            _ => min_h,
                        };
                    }
                }
                total_grow += child.grow;
            }
            
            // Available space for growing items
            let main_size = if direction == Direction::Horizontal { content_w } else { content_h };
            let remaining = main_size.saturating_sub(total_fixed + gap_space);
            
            // Layout each child
            let mut pos = if direction == Direction::Horizontal { content_x } else { content_y };
            
            // Handle main axis alignment
            if total_grow == 0.0 {
                let total_children_size = total_fixed + gap_space;
                pos = match align {
                    Align::Start => pos,
                    Align::Center => pos + ((main_size.saturating_sub(total_children_size)) / 2) as i32,
                    Align::End => pos + (main_size.saturating_sub(total_children_size)) as i32,
                    Align::Stretch => pos,
                };
            }
            
            for child in &mut self.children {
                let (min_w, min_h) = child.min_content_size();
                
                let (child_w, child_h) = if direction == Direction::Horizontal {
                    let w = if child.grow > 0.0 {
                        ((remaining as f32) * child.grow / total_grow) as usize
                    } else {
                        match child.width {
                            SizeHint::Fixed(w) => w,
                            SizeHint::Fill => remaining,
                            _ => min_w,
                        }
                    };
                    
                    let h = match cross_align {
                        Align::Stretch => content_h,
                        _ => match child.height {
                            SizeHint::Fixed(h) => h,
                            SizeHint::Fill => content_h,
                            _ => min_h,
                        },
                    };
                    
                    (w, h)
                } else {
                    let h = if child.grow > 0.0 {
                        ((remaining as f32) * child.grow / total_grow) as usize
                    } else {
                        match child.height {
                            SizeHint::Fixed(h) => h,
                            SizeHint::Fill => remaining,
                            _ => min_h,
                        }
                    };
                    
                    let w = match cross_align {
                        Align::Stretch => content_w,
                        _ => match child.width {
                            SizeHint::Fixed(w) => w,
                            SizeHint::Fill => content_w,
                            _ => min_w,
                        },
                    };
                    
                    (w, h)
                };
                
                // Cross-axis position
                let (cx, cy) = if direction == Direction::Horizontal {
                    let y_offset = match cross_align {
                        Align::Start => 0,
                        Align::Center => (content_h.saturating_sub(child_h)) / 2,
                        Align::End => content_h.saturating_sub(child_h),
                        Align::Stretch => 0,
                    };
                    (pos, content_y + y_offset as i32)
                } else {
                    let x_offset = match cross_align {
                        Align::Start => 0,
                        Align::Center => (content_w.saturating_sub(child_w)) / 2,
                        Align::End => content_w.saturating_sub(child_w),
                        Align::Stretch => 0,
                    };
                    (content_x + x_offset as i32, pos)
                };
                
                child.layout(cx, cy, child_w, child_h);
                
                if direction == Direction::Horizontal {
                    pos += child_w as i32 + gap as i32;
                } else {
                    pos += child_h as i32 + gap as i32;
                }
            }
        }
    }
}

/// Convert a LayoutNode tree to flat Element list for rendering
pub fn flatten_layout(node: &LayoutNode) -> Vec<super::app::Element> {
    let mut result = Vec::new();
    
    flatten_recursive(node, &mut result);
    result
}

fn flatten_recursive(node: &LayoutNode, out: &mut Vec<super::app::Element>) {
    match &node.kind {
        NodeKind::Label { text } => {
            out.push(super::app::Element::Label {
                text: text.clone(),
                x: node.computed_x,
                y: node.computed_y,
            });
        }
        NodeKind::Button { text } => {
            out.push(super::app::Element::Button {
                text: text.clone(),
                x: node.computed_x,
                y: node.computed_y,
                width: node.computed_width,
                height: node.computed_height,
            });
        }
        NodeKind::TextBox => {
            out.push(super::app::Element::TextBox {
                x: node.computed_x,
                y: node.computed_y,
                width: node.computed_width,
                height: node.computed_height,
            });
        }
        NodeKind::Panel => {
            out.push(super::app::Element::Panel {
                x: node.computed_x,
                y: node.computed_y,
                width: node.computed_width,
                height: node.computed_height,
            });
        }
        NodeKind::Spacer => {
            // Spacers don't render
        }
        NodeKind::Container { .. } => {
            // Containers just layout their children
            for child in &node.children {
                flatten_recursive(child, out);
            }
        }
    }
}
