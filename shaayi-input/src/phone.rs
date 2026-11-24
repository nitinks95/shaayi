/// A phone link is dead once it has been silent longer than this.
/// The phone heartbeat is every 1.5 s, so this tolerates two missed pings.
pub const DEAD_AFTER_MS: f64 = 4000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    Alive,
    Dead,
}

pub fn link_state(last_seen_ms: f64, now_ms: f64) -> LinkState {
    if now_ms - last_seen_ms > DEAD_AFTER_MS {
        LinkState::Dead
    } else {
        LinkState::Alive
    }
}
