//! Integration tests for thread clues, placement, inspect, and persistence.

use std::collections::BTreeSet;

use terminus_core::{Permissions, PlayerId, SessionIdentity};
use uuid::Uuid;

use crate::geography::frontier_epoch;
use crate::model::{Item, Operator, PersistOp, STARTER_LOCATION};
use crate::state::WorldState;
use crate::store::Store;
use crate::subway::ASH_STATION;
use crate::threads::{best_reveal_text, load_default_threads, placement_hash, USED_SUBWAY_FLAG};

fn ada_operator() -> (PlayerId, Operator) {
    let id = PlayerId::new();
    let account_id = Uuid::new_v4();
    let op = Operator {
        id,
        account_id,
        username: "ada".into(),
        name: "Ada".into(),
        location_id: STARTER_LOCATION.into(),
        bio: String::new(),
        status: String::new(),
        flags: BTreeSet::new(),
        items: vec![Item::chip()],
    };
    (id, op)
}

fn connect(world: &mut WorldState, player_id: PlayerId, name: &str) -> terminus_core::SessionId {
    let identity = SessionIdentity::new(player_id, name, Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    session
}

#[test]
fn inspect_shows_surface_until_knowledge_then_best_reveal() {
    let mut world = WorldState::for_tests();
    let catalog = load_default_threads();
    let sites = crate::threads::build_placement_map(&catalog, frontier_epoch().world_seed);
    let pos = match sites.get("warm-chalk-doorframe").unwrap() {
        crate::threads::ClueSite::Wild(p) => *p,
        _ => panic!("expected wild site"),
    };
    let (player, op) = ada_operator();
    world.insert_operator(op);
    let session = connect(&mut world, player, "Ada");
    world.operators.get_mut(&player).unwrap().location_id = pos.key();
    world.apply(session, "inspect graffiti").unwrap();
    let first = world.findings.get(&session).cloned().unwrap_or_default();
    assert!(first.contains("chalk circle"));
    assert!(!first.contains("still tends"));

    world.threads.seen.push(crate::threads::ClueSeen {
        operator_id: player,
        clue_id: "warm-stairwell-cup".into(),
        first_seen_at: 1,
    });
    world.apply(session, "inspect graffiti").unwrap();
    let second = world.findings.get(&session).cloned().unwrap_or_default();
    assert!(second.contains("still tends"));
}

#[tokio::test]
async fn clue_seen_survives_store_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("threads.db");
    let store = Store::open(&path).await.unwrap();
    store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();

    let (player, op) = ada_operator();
    store.create_account(op.account_id, "ada", "hash", &op, 10).await.unwrap();

    store
        .commit(&[PersistOp::ClueSeen {
            operator_id: player,
            clue_id: "subway-stair-clue".into(),
            first_seen_at: 99,
        }])
        .await
        .unwrap();

    let loaded = store.load().await.unwrap();
    assert_eq!(loaded.clue_seen.len(), 1);
    assert_eq!(loaded.clue_seen[0].clue_id, "subway-stair-clue");

    let mut again = WorldState::for_tests();
    again.install_loaded_world(loaded);
    assert!(again.threads.has_seen(player, "subway-stair-clue"));
}

#[test]
fn placement_differs_by_world_seed() {
    let catalog = load_default_threads();
    let a = crate::threads::build_placement_map(&catalog, 1);
    let b = crate::threads::build_placement_map(&catalog, 2);
    assert_ne!(a, b);
}

#[test]
fn hash_placement_is_stable_for_region() {
    let seed = frontier_epoch().world_seed;
    let one = placement_hash(seed, "the-tally", "tally-shutter-graffiti", 0, 1);
    let two = placement_hash(seed, "the-tally", "tally-shutter-graffiti", 0, 1);
    assert_eq!(one, two);
}

#[test]
fn off_schedule_reveal_requires_subway_use() {
    let mut world = WorldState::for_tests();
    let (player, op) = ada_operator();
    world.insert_operator(op);
    let session = connect(&mut world, player, "Ada");
    world.operators.get_mut(&player).unwrap().location_id = ASH_STATION.into();

    world.apply(session, "inspect timetable").unwrap();
    let before = world.findings.get(&session).cloned().unwrap_or_default();
    assert!(before.contains("Meridian Cut"));
    assert!(!before.contains("departure time"));

    world.operators.get_mut(&player).unwrap().flags.insert(USED_SUBWAY_FLAG.into());
    world.apply(session, "inspect timetable").unwrap();
    let after = world.findings.get(&session).cloned().unwrap_or_default();
    assert!(after.contains("departure time"));
}

#[test]
fn two_maps_reveal_requires_corroborated_theories() {
    let mut world = WorldState::for_tests();
    let catalog = load_default_threads();
    world.threads.catalog = catalog;
    let player = PlayerId::new();
    let clue = world.threads.catalog.clue("map-hand-third").unwrap().clone();
    let ctx = world.knowledge_context(player);
    assert!(best_reveal_text(&clue.reveals, &ctx).is_none());

    world.theories.entries.push(crate::theories::TheoryEntry {
        id: 1,
        topic: "two-maps".into(),
        author_id: PlayerId::new(),
        kind: crate::theories::TheoryKind::Theory,
        body: "Older layout.".into(),
        created_at: 1,
    });
    world.theories.entries.push(crate::theories::TheoryEntry {
        id: 2,
        topic: "two-maps".into(),
        author_id: PlayerId::new(),
        kind: crate::theories::TheoryKind::Theory,
        body: "They redraw after floods.".into(),
        created_at: 2,
    });
    let ctx = world.knowledge_context(player);
    let text = best_reveal_text(&clue.reveals, &ctx).unwrap();
    assert!(text.contains("different period"));
}
