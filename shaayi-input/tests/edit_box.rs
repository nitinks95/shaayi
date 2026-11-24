use shaayi_input::edit_box::{EditBox, ASPECT};

#[test]
fn height_follows_aspect() {
    let b = EditBox { anchor: (500.0, 300.0), width: 320.0 };
    assert!((b.height() - 320.0 / ASPECT).abs() < 1e-9);
}

#[test]
fn map_corners_hit_box_corners() {
    let b = EditBox { anchor: (500.0, 300.0), width: 320.0 };
    let (x0, y0) = (b.anchor.0 - 160.0, b.anchor.1 - b.height() / 2.0);
    assert_eq!(b.map(0.0, 0.0), (x0, y0));
    assert_eq!(b.map(1.0, 1.0), (x0 + 320.0, y0 + b.height()));
}

#[test]
fn clamp_keeps_box_on_canvas() {
    let mut b = EditBox { anchor: (5.0, 5.0), width: 320.0 };
    b.clamp_to(1000.0, 700.0);
    assert!(b.anchor.0 - 160.0 >= 0.0 && b.anchor.1 - b.height() / 2.0 >= 0.0);
}

#[test]
fn clamp_after_window_shrinks() {
    let mut b = EditBox { anchor: (900.0, 600.0), width: 320.0 };
    b.clamp_to(400.0, 300.0);
    assert!(b.anchor.0 + 160.0 <= 400.0 && b.anchor.1 + b.height() / 2.0 <= 300.0);
}

#[test]
fn contains_inside_edge_outside() {
    let b = EditBox { anchor: (500.0, 300.0), width: 320.0 };
    let (x0, y0) = (340.0, 300.0 - b.height() / 2.0);
    assert!(b.contains((500.0, 300.0)));
    assert!(b.contains((x0, y0)));
    assert!(b.contains((x0 + 320.0, y0 + b.height())));
    assert!(!b.contains((x0 - 0.1, 300.0)));
    assert!(!b.contains((500.0, y0 + b.height() + 0.1)));
}

#[test]
fn oversized_box_shrinks_to_canvas_height() {
    let mut b = EditBox { anchor: (200.0, 50.0), width: 320.0 };
    b.clamp_to(300.0, 100.0);
    assert!(b.width <= 100.0 * ASPECT + 1e-9);
    assert!(b.anchor.0 - b.width / 2.0 >= -1e-9 && b.anchor.0 + b.width / 2.0 <= 300.0 + 1e-9);
    assert!(b.anchor.1 - b.height() / 2.0 >= -1e-9 && b.anchor.1 + b.height() / 2.0 <= 100.0 + 1e-9);
}

#[test]
fn clamp_right_bottom_keeps_width() {
    let mut b = EditBox { anchor: (990.0, 690.0), width: 320.0 };
    b.clamp_to(1000.0, 700.0);
    assert_eq!(b.width, 320.0);
    assert_eq!(b.anchor.0, 1000.0 - 160.0);
    assert_eq!(b.anchor.1, 700.0 - b.height() / 2.0);
}

#[test]
fn degenerate_canvas_does_not_panic() {
    let mut b = EditBox { anchor: (10.0, 10.0), width: 320.0 };
    b.clamp_to(f64::NAN, 300.0);
    b.clamp_to(300.0, f64::INFINITY);
    assert_eq!(b, EditBox { anchor: (10.0, 10.0), width: 320.0 });
    b.clamp_to(0.0, 0.0);
    assert_eq!(b.anchor, (0.0, 0.0));
    assert_eq!(b.width, 320.0);
    b.clamp_to(-5.0, 100.0);
}
