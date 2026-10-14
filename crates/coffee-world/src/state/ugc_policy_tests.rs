//! Command-path checks for near-hub UGC policy and listen/chalk.

use terminus_core::{Game, Permissions, SessionIdentity};

use crate::geography::{gate_position, permanent_traces_allowed, WildernessPos};
use crate::state::{starter_operator, visible_feed, WorldState};

#[test]
fn near_hub_rejects_sign_allows_chalk() {
    let mut world = WorldState::for_tests();
    let id = terminus_core::PlayerId::new();
    world.insert_operator(starter_operator(
        id,
        uuid::Uuid::new_v4(),
        "ada",
        "ada",
    ));
    let identity = SessionIdentity::new(id, "ada", Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    world.apply(session, "go up").unwrap();
    world.apply(session, "outskirts").unwrap();
    world.apply(session, "west").unwrap();
    let pos = WildernessPos::parse(&world.location_key(session).unwrap()).unwrap();
    assert!(!permanent_traces_allowed(pos));
    assert!(world.apply(session, "sign hello").is_err());
    world.apply(session, "chalk hello").unwrap();
    assert!(world.geography.structures.iter().any(|s| s.kind == "chalk"));
}

#[test]
fn listen_runs_in_wilderness_without_leaking_clues() {
    let mut world = WorldState::for_tests();
    let id = terminus_core::PlayerId::new();
    world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "ada", "ada"));
    let identity = SessionIdentity::new(id, "ada", Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    world.apply(session, "go up").unwrap();
    world.apply(session, "outskirts").unwrap();
    world.apply(session, "listen").unwrap();
    let feed = visible_feed(&world, session).unwrap();
    let text: String = feed.iter().map(|line| line.text.as_str()).collect();
    assert!(text.contains("near the Hub") || text.contains("Hub gate"));
    assert!(!text.to_lowercase().contains("clue"));
    assert!(!text.to_lowercase().contains("thread"));
}

#[test]
fn gate_still_blocks_chalk() {
    let mut world = WorldState::for_tests();
    let id = terminus_core::PlayerId::new();
    world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "ada", "ada"));
    let identity = SessionIdentity::new(id, "ada", Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    world.operators.get_mut(&id).unwrap().location_id = gate_position().key();
    assert!(world.apply(session, "chalk no").is_err());
}
