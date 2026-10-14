//! Gameplay-path integration tests for clue discovery (commands only, no injected state).

use terminus_core::{Permissions, PlayerId, SessionIdentity};
use uuid::Uuid;

use crate::geography::{frontier_epoch, gate_identity};
use crate::model::{Item, Operator, PersistOp, STARTER_LOCATION};
use crate::state::WorldState;
use crate::store::Store;
use crate::threads::{
    generate_for_region_with_params, load_default_threads, look_object_label, CoverageParams,
};

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

use std::collections::BTreeSet;

fn connect(world: &mut WorldState, player_id: PlayerId, name: &str) -> terminus_core::SessionId {
    let identity = SessionIdentity::new(player_id, name, Permissions::player());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    session
}

#[test]
fn scenario_a_first_discovery_starter_walk_via_commands() {
    let seed = frontier_epoch().world_seed;
    let templates = crate::threads::load_default_templates();
    let region = generate_for_region_with_params(
        seed,
        gate_identity(),
        &templates,
        CoverageParams::default(),
    )
    .unwrap();
    let starter_clue = region
        .clues
        .iter()
        .find(|(_, c)| c.tags.iter().any(|t| t == "starter"))
        .expect("starter clue");
    let site = region.sites.get(starter_clue.0).expect("starter site");
    let pos = match site {
        crate::threads::ClueSite::Wild(p) => *p,
        _ => panic!("starter must be wild"),
    };

    let mut world = WorldState::for_tests();
    let (player, op) = ada_operator();
    world.insert_operator(op);
    let session = connect(&mut world, player, "Ada");
    world.operators.get_mut(&player).unwrap().location_id = pos.key();
    world.threads.ensure_procedural(pos.identity).unwrap();

    world.apply(session, "look").unwrap();
    let look_objects = world.thread_look_objects(&pos.key(), Some(pos));
    assert!(
        look_objects.iter().any(|(_, label)| label.contains("counted mark")),
        "look objects: {look_objects:?}"
    );
    assert_eq!(look_object_label(starter_clue.1), "counted mark");

    world.apply(session, "inspect graffiti").unwrap();
    assert!(world.threads.has_seen(player, starter_clue.0));
    let finding = world.findings.get(&session).cloned().unwrap_or_default();
    assert!(!finding.is_empty());
}

#[test]
fn scenario_b_knowledge_gated_reveal_after_prior_discovery() {
    let mut world = WorldState::for_tests();
    let catalog = load_default_threads();
    let sites = crate::threads::build_placement_map(&catalog, frontier_epoch().world_seed);
    let pos = match sites.get("warm-chalk-doorframe").unwrap() {
        crate::threads::ClueSite::Wild(p) => *p,
        _ => panic!("wild"),
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

#[test]
fn scenario_c_follow_starter_second_clue_by_movement() {
    let seed = frontier_epoch().world_seed;
    let templates = crate::threads::load_default_templates();
    let region = generate_for_region_with_params(
        seed,
        gate_identity(),
        &templates,
        CoverageParams::default(),
    )
    .unwrap();
    let starter_ids: Vec<_> = region
        .clues
        .iter()
        .filter(|(_, c)| c.tags.iter().any(|t| t == "starter"))
        .map(|(id, _)| id.clone())
        .collect();
    assert!(starter_ids.len() >= 2, "starter thread should have multiple clues");
    let first_site = region.sites.get(&starter_ids[0]).unwrap();
    let second_site = region.sites.get(&starter_ids[1]).unwrap();
    let (first_pos, second_pos) = match (first_site, second_site) {
        (crate::threads::ClueSite::Wild(a), crate::threads::ClueSite::Wild(b)) => (*a, *b),
        _ => panic!("wild sites"),
    };
    assert_ne!(first_pos.key(), second_pos.key());

    let mut world = WorldState::for_tests();
    let (player, op) = ada_operator();
    world.insert_operator(op);
    let session = connect(&mut world, player, "Ada");
    world.operators.get_mut(&player).unwrap().location_id = first_pos.key();
    world.threads.ensure_procedural(first_pos.identity).unwrap();
    world.apply(session, "inspect graffiti").unwrap();
    assert!(world.threads.has_seen(player, &starter_ids[0]));

    world.operators.get_mut(&player).unwrap().location_id = second_pos.key();
    world.threads.ensure_procedural(second_pos.identity).unwrap();
    world.apply(session, "inspect graffiti").unwrap();
    assert!(world.threads.has_seen(player, &starter_ids[1]));
}

#[tokio::test]
async fn scenario_d_procedural_starter_clue_seen_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("starter_clue.db");
    let store = Store::open(&path).await.unwrap();
    store.seed(&crate::seed_locations(), &crate::seed_exits()).await.unwrap();

    let (player, op) = ada_operator();
    store.create_account(op.account_id, "ada", "hash", &op, 10).await.unwrap();

    let seed = frontier_epoch().world_seed;
    let clue_id = format!("{}:0", crate::threads::starter_thread_instance_id(seed));

    store
        .commit(&[PersistOp::ClueSeen {
            operator_id: player,
            clue_id: clue_id.clone(),
            first_seen_at: 42,
        }])
        .await
        .unwrap();

    let loaded = store.load().await.unwrap();
    let mut world = WorldState::for_tests();
    world.install_loaded_world(loaded);
    assert!(world.threads.has_seen(player, &clue_id));
}

#[test]
fn scenario_e_unresolved_thread_not_counted_as_final_without_last_clue() {
    let mut world = WorldState::for_tests();
    let catalog = load_default_threads();
    world.threads.catalog = catalog;
    let (player, op) = ada_operator();
    world.insert_operator(op);
    world.threads.seen.push(crate::threads::ClueSeen {
        operator_id: player,
        clue_id: "tally-shutter-graffiti".into(),
        first_seen_at: 1,
    });
    let reports = world.simulation_thread_reports(0);
    let tally = reports.iter().find(|r| r.thread_id == "the-tally").expect("tally report");
    assert_eq!(tally.operators_reaching_final_clue, 0);
}

#[test]
fn coverage_flavor_uses_wall_mark_thread_uses_counted_mark() {
    let seed = frontier_epoch().world_seed;
    let templates = crate::threads::load_default_templates();
    let region = generate_for_region_with_params(
        seed,
        gate_identity(),
        &templates,
        CoverageParams::default(),
    )
    .unwrap();
    let coverage = region.clues.iter().find(|(id, _)| id.starts_with("cover:")).map(|(_, c)| c);
    if let Some(clue) = coverage {
        assert_eq!(look_object_label(clue), "wall mark");
    }
    let starter =
        region.clues.values().find(|c| c.tags.iter().any(|t| t == "starter")).expect("starter");
    assert_eq!(look_object_label(starter), "counted mark");
}

#[test]
fn long_procession_first_spine_clue_is_inspectable_via_commands() {
    let seed = frontier_epoch().world_seed;
    let schedule = crate::threads::long_procession::build_spine_schedule(seed);
    let first = schedule.first().expect("spine");
    let pos =
        crate::threads::long_procession::snap_walkable(gate_identity(), first.abs_x, first.abs_y)
            .expect("first clue walkable");

    let mut world = WorldState::for_tests();
    let (player, op) = ada_operator();
    world.insert_operator(op);
    let session = connect(&mut world, player, "Ada");
    world.operators.get_mut(&player).unwrap().location_id = pos.key();
    world.threads.ensure_procedural(pos.identity).unwrap();
    world.apply(session, "inspect graffiti").unwrap();
    assert!(world.threads.has_seen(player, &first.clue_id));
    let text = world.findings.get(&session).cloned().unwrap_or_default();
    assert!(!text.is_empty(), "inspect should return surface text");
}
