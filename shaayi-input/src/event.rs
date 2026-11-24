#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Trackpad,
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Down,
    Move,
    Up,
    Cancel,
}

/// A single contact sample. `x`, `y` are normalised 0.0-1.0 across the
/// source surface, origin top-left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactEvent {
    pub source: Source,
    pub id: u64,
    pub phase: Phase,
    pub x: f64,
    pub y: f64,
    pub at_ms: f64,
}

/// Ink operations; coordinates are normalised to the edit box.
#[derive(Debug, Clone, PartialEq)]
pub enum InkOp {
    Segment { from: (f64, f64), to: (f64, f64) },
    Dot { at: (f64, f64) },
    Rollback { segments: usize },
}
