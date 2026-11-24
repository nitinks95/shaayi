//! Edit box: a fixed-aspect rectangle on the canvas that surface contact
//! coordinates (normalised 0-1) are mapped into.

use crate::event::InkOp;

pub const ASPECT: f64 = 1.6;

/// Box centred on `anchor`, `width` wide, in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditBox {
    pub anchor: (f64, f64),
    pub width: f64,
}

impl EditBox {
    pub fn height(&self) -> f64 {
        self.width / ASPECT
    }

    fn origin(&self) -> (f64, f64) {
        (self.anchor.0 - self.width / 2.0, self.anchor.1 - self.height() / 2.0)
    }

    /// Move the anchor so the whole box fits in the canvas; shrink width first
    /// if the box is larger than the canvas.
    pub fn clamp_to(&mut self, canvas_w: f64, canvas_h: f64) {
        if !canvas_w.is_finite() || !canvas_h.is_finite() {
            return;
        }
        if canvas_w <= 0.0 || canvas_h <= 0.0 {
            // Zero-size canvas: pin the anchor to the origin, leave width alone.
            self.anchor = (0.0, 0.0);
            return;
        }
        self.width = self.width.min(canvas_w).min(canvas_h * ASPECT);
        let (hw, hh) = (self.width / 2.0, self.height() / 2.0);
        // max/min chains (not f64::clamp) so this can never panic.
        self.anchor.0 = self.anchor.0.max(hw).min(canvas_w - hw);
        self.anchor.1 = self.anchor.1.max(hh).min(canvas_h - hh);
    }

    /// Linear map from normalised (0-1, y down) to canvas pixels.
    pub fn map(&self, x: f64, y: f64) -> (f64, f64) {
        let (x0, y0) = self.origin();
        (x0 + x * self.width, y0 + y * self.height())
    }

    /// False until a real box has been placed (the default box has width 0).
    pub fn is_set(&self) -> bool {
        self.width.is_finite() && self.width > 0.0 && self.anchor.0.is_finite() && self.anchor.1.is_finite()
    }

    /// Map an ink op into canvas pixels. Returns `None` while the box is not
    /// set, so no contact is ever drawn through the default zero-width box.
    pub fn map_op(&self, op: &InkOp) -> Option<InkOp> {
        if !self.is_set() {
            return None;
        }
        Some(match op {
            InkOp::Segment { from, to } => InkOp::Segment { from: self.map(from.0, from.1), to: self.map(to.0, to.1) },
            InkOp::Dot { at } => InkOp::Dot { at: self.map(at.0, at.1) },
            InkOp::Rollback { segments } => InkOp::Rollback { segments: *segments },
        })
    }

    pub fn contains(&self, p: (f64, f64)) -> bool {
        let (x0, y0) = self.origin();
        p.0 >= x0 && p.0 <= x0 + self.width && p.1 >= y0 && p.1 <= y0 + self.height()
    }
}
