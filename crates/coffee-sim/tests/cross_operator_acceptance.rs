#![allow(clippy::unwrap_used)]

use coffee_sim::agent::SimAgent;
use coffee_sim::execute::execute_line;
use coffee_sim::feed_memory::absorb_command_outcome;
use coffee_sim::interaction::{player_index_map, InteractionTracker};
use coffee_sim::profile::Profile;
use coffee_world::geography::{permanent_traces_allowed, WildernessPos};
use coffee_world::WorldState;

/// Deterministic acceptance: sign → travel → inspect sign → theory response.
#[test]
fn scripted_sign_inspect_and_theory_response_counts_as_connected_sequence() {
    let mut world = WorldState::for_tests();
    let ada = SimAgent::new(1, 0, Profile::Wanderer);
    let mut bob = SimAgent::new(1, 1, Profile::Follower);
    world.insert_operator(ada.operator());
    world.insert_operator(bob.operator());
    world.connect_player(ada.identity.clone()).unwrap();
    world.connect_player(bob.identity.clone()).unwrap();

    let mut tracker = InteractionTracker::default();
    let slot = 1_000_000u64;

    for session in [ada.session_id, bob.session_id] {
        for line in ["go up", "outskirts"] {
            execute_line(&mut world, session, line);
        }
    }
    walk_to_fringe(&mut world, ada.session_id);
    execute_line(&mut world, ada.session_id, "sign Relay gap this way.");
    let ada_loc = world.location_key_for_sim(ada.session_id).unwrap();
    tracker.record_trace_created(0, &ada_loc, "sign", None, slot);

    walk_to_fringe(&mut world, bob.session_id);
    let (result, _) = execute_line(&mut world, bob.session_id, "inspect sign");
    assert!(result.is_success());
    absorb_command_outcome(&world, bob.session_id, "inspect sign", &mut bob.memory);
    tracker.note_encounters(1, &bob.memory, &ada_loc, slot + 1000);
    let (result, _) = execute_line(
        &mut world,
        bob.session_id,
        "theory post observation survey-mark Saw the sign pointing toward the relay.",
    );
    assert!(result.is_success());
    tracker.record_meaningful_action(
        1,
        &bob.display_name,
        "theory post observation survey-mark Saw the sign pointing toward the relay.",
        &ada_loc,
        slot + 2000,
    );

    let (metrics, transcript) =
        tracker.finalize(&world, &[ada.clone(), bob.clone()], &player_index_map(&[ada, bob]));
    assert_eq!(metrics.connected_sequences, 1);
    assert_eq!(metrics.follow_up_actions_after_encounter, 1);
    assert!(!transcript.lines.is_empty());
}

fn walk_to_fringe(world: &mut WorldState, session: terminus_core::SessionId) {
    for _ in 0..96 {
        let key = world.location_key_for_sim(session).expect("location");
        let pos = WildernessPos::parse(&key).expect("wilderness");
        if permanent_traces_allowed(pos) {
            return;
        }
        if execute_line(world, session, "west").0.is_success() {
            continue;
        }
        let _ = execute_line(world, session, "north");
    }
    panic!("could not reach a fringe tile for permanent traces");
}
