//! Platform-neutral coordinate helpers for native surfaces.

/// Flip a bottom-left-origin normalised point (macOS `NSTouch.normalizedPosition`)
/// to the shared top-left-origin convention used by `ContactEvent`.
pub fn to_top_left(x: f64, y: f64) -> (f64, f64) {
    (x, 1.0 - y)
}

#[cfg(test)]
mod tests {
    use super::to_top_left;

    #[test]
    fn normalised_touch_flips_to_top_left() {
        assert_eq!(to_top_left(0.25, 0.1), (0.25, 0.9));
    }
}
