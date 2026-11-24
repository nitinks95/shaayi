/// Normalise a raw HID value to 0-1, origin top-left. The ASUS ELAN pad
/// already reports Y top-down; flipping it inverted vertical motion (see
/// PRODUCT.md, 2026-11-24 validation), so Y is passed through unflipped.
pub fn normalise(x: i32, y: i32, max_x: i32, max_y: i32) -> (f64, f64) {
    (x as f64 / max_x as f64, y as f64 / max_y as f64)
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn logical_range_normalises_to_unit_square() {
        assert_eq!(normalise(0, 0, 4095, 2499), (0.0, 0.0));
        assert_eq!(normalise(4095, 2499, 4095, 2499), (1.0, 1.0));
    }
}
