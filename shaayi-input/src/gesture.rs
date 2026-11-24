use crate::event::{ContactEvent, InkOp, Phase};
use std::collections::HashMap;

const ROLLBACK_WINDOW_MS: f64 = 150.0;
/// A Move closer than this (normalised units) to the last point is not movement.
/// Windows sends a report per frame while a finger rests; those must not draw.
pub const MOVE_SLOP: f64 = 0.0005;

struct Contact {
    last: (f64, f64),
    moved: bool,
}

#[derive(Default)]
pub struct GestureMachine {
    active: HashMap<u64, Contact>,
    session_start: f64,
    segments: usize,
    suppressed: bool,
}

impl GestureMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, e: ContactEvent) -> Vec<InkOp> {
        let p = (e.x, e.y);
        match e.phase {
            Phase::Down => {
                if self.active.is_empty() {
                    // New session.
                    self.session_start = e.at_ms;
                    self.segments = 0;
                    self.suppressed = false;
                    self.active.insert(e.id, Contact { last: p, moved: false });
                    return Vec::new();
                }
                if self.active.contains_key(&e.id) {
                    // Re-Down on an active id: treat as a new contact on that id.
                    self.active.insert(e.id, Contact { last: p, moved: false });
                    return Vec::new();
                }
                // A second (or further) contact.
                self.active.insert(e.id, Contact { last: p, moved: false });
                if self.suppressed {
                    return Vec::new();
                }
                self.suppressed = true;
                if e.at_ms - self.session_start <= ROLLBACK_WINDOW_MS {
                    let n = std::mem::take(&mut self.segments);
                    vec![InkOp::Rollback { segments: n }]
                } else {
                    Vec::new()
                }
            }
            Phase::Move => {
                let suppressed = self.suppressed;
                let Some(c) = self.active.get_mut(&e.id) else {
                    return Vec::new();
                };
                let from = c.last;
                if (p.0 - from.0).hypot(p.1 - from.1) <= MOVE_SLOP {
                    // Unchanged position: no segment, not counted as movement.
                    // `last` is kept so slow drags still accumulate past the slop.
                    return Vec::new();
                }
                c.last = p;
                c.moved = true;
                if suppressed {
                    return Vec::new();
                }
                self.segments += 1;
                vec![InkOp::Segment { from, to: p }]
            }
            Phase::Up => {
                let Some(c) = self.active.remove(&e.id) else {
                    return Vec::new();
                };
                let dot = !c.moved && !self.suppressed;
                self.close_if_idle();
                if dot {
                    vec![InkOp::Dot { at: p }]
                } else {
                    Vec::new()
                }
            }
            Phase::Cancel => {
                self.active.remove(&e.id);
                self.close_if_idle();
                Vec::new()
            }
        }
    }

    /// Ends the session (focus loss or edit-mode exit).
    pub fn reset(&mut self) -> Vec<InkOp> {
        self.active.clear();
        self.close_if_idle();
        Vec::new()
    }

    fn close_if_idle(&mut self) {
        if self.active.is_empty() {
            self.segments = 0;
            self.suppressed = false;
        }
    }
}
