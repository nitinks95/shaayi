use shaayi_input::phone::{link_state, LinkState};

#[test]
fn silent_for_four_seconds_is_dead() {
    assert_eq!(link_state(0.0, 4001.0), LinkState::Dead);
    assert_eq!(link_state(0.0, 3999.0), LinkState::Alive);
}
