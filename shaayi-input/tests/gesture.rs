use shaayi_input::{ContactEvent, GestureMachine, InkOp, Phase, Source};

fn ev(id: u64, phase: Phase, x: f64, y: f64, t: f64) -> ContactEvent {
    ContactEvent { source: Source::Trackpad, id, phase, x, y, at_ms: t }
}

#[test]
fn single_finger_move_emits_segment() {
    let mut g = GestureMachine::new();
    assert!(g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0)).is_empty());
    let ops = g.handle(ev(1, Phase::Move, 0.3, 0.2, 10.0));
    assert!(matches!(ops.as_slice(), [InkOp::Segment { .. }]));
}

#[test]
fn tap_emits_dot() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.5, 0.5, 0.0));
    let ops = g.handle(ev(1, Phase::Up, 0.5, 0.5, 80.0));
    assert!(matches!(ops.as_slice(), [InkOp::Dot { .. }]));
}

#[test]
fn second_finger_within_150ms_rolls_back_session() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.handle(ev(1, Phase::Move, 0.3, 0.2, 40.0));
    let ops = g.handle(ev(2, Phase::Down, 0.6, 0.6, 100.0));
    assert!(matches!(ops.as_slice(), [InkOp::Rollback { segments: 1 }]));
}

#[test]
fn second_finger_after_150ms_keeps_ink_and_suppresses_drawing() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.handle(ev(1, Phase::Move, 0.3, 0.2, 40.0));
    let ops = g.handle(ev(2, Phase::Down, 0.6, 0.6, 200.0));
    assert!(ops.is_empty());
    assert!(g.handle(ev(1, Phase::Move, 0.4, 0.2, 220.0)).is_empty());
}

#[test]
fn lift_mid_stroke_ends_cleanly_without_joining_next_stroke() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.handle(ev(1, Phase::Move, 0.3, 0.2, 10.0));
    g.handle(ev(1, Phase::Cancel, 0.3, 0.2, 12.0));
    let ops = g.handle(ev(3, Phase::Down, 0.9, 0.9, 500.0));
    assert!(ops.is_empty(), "a new Down must not connect to the old stroke");
}

#[test]
fn reset_ends_session_and_clears_state() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.2, 0.2, 0.0));
    g.reset();
    assert!(g.handle(ev(1, Phase::Move, 0.3, 0.2, 10.0)).is_empty());
}

#[test]
fn unchanged_move_emits_nothing_and_keeps_tap() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.5, 0.5, 0.0));
    // Windows reports a Move per HID report while a finger rests.
    assert!(g.handle(ev(1, Phase::Move, 0.5, 0.5, 8.0)).is_empty());
    assert!(g.handle(ev(1, Phase::Move, 0.5002, 0.5001, 16.0)).is_empty());
    let ops = g.handle(ev(1, Phase::Up, 0.5, 0.5, 24.0));
    assert!(matches!(ops.as_slice(), [InkOp::Dot { .. }]));
}

#[test]
fn tap_down_up_same_point_emits_dot() {
    let mut g = GestureMachine::new();
    assert!(g.handle(ev(1, Phase::Down, 0.3, 0.7, 0.0)).is_empty());
    let ops = g.handle(ev(1, Phase::Up, 0.3, 0.7, 50.0));
    assert_eq!(ops, vec![InkOp::Dot { at: (0.3, 0.7) }]);
}

#[test]
fn sub_slop_moves_accumulate_into_a_segment() {
    let mut g = GestureMachine::new();
    g.handle(ev(1, Phase::Down, 0.5, 0.5, 0.0));
    // Sub-slop wobble from the Down point draws nothing.
    assert!(g.handle(ev(1, Phase::Move, 0.502, 0.5, 8.0)).is_empty());
    // Once the contact leaves the slop radius, one segment from the Down point is drawn.
    let ops = g.handle(ev(1, Phase::Move, 0.504, 0.5, 16.0));
    assert_eq!(ops, vec![InkOp::Segment { from: (0.5, 0.5), to: (0.504, 0.5) }]);
}
