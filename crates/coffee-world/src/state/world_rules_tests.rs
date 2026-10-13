use super::*;
use terminus_core::Permissions;

fn pair() -> (WorldState, SessionId, SessionId, PlayerId, PlayerId) {
    let mut world = WorldState::for_tests();
    let ada_id = PlayerId::new();
    let bob_id = PlayerId::new();
    world.insert_operator(starter_operator(ada_id, uuid::Uuid::new_v4(), "ada", "ada"));
    world.insert_operator(starter_operator(bob_id, uuid::Uuid::new_v4(), "bob", "bob"));
    let ada = SessionIdentity::new(ada_id, "ada", Permissions::player());
    let bob = SessionIdentity::new(bob_id, "bob", Permissions::player());
    let ada_session = ada.session_id;
    let bob_session = bob.session_id;
    world.connect_player(ada).unwrap();
    world.connect_player(bob).unwrap();
    (world, ada_session, bob_session, ada_id, bob_id)
}

fn feed_text(world: &WorldState, session: SessionId) -> String {
    world
        .view(session)
        .unwrap()
        .feed
        .iter()
        .map(|line| format!("{}:{}", line.speaker.clone().unwrap_or_default(), line.text))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn movement_chat_and_presence() {
    let (mut world, ada, bob, _, _) = pair();
    let here = world.view(ada).unwrap();
    assert_eq!(here.location_name, "The Coffee Shop");
    assert!(here.present.iter().any(|p| p.name == "bob"));

    world.apply(ada, "say hello bob").unwrap();
    assert!(feed_text(&world, bob).contains("hello bob"));

    world.apply(bob, "go alley").unwrap();
    assert_eq!(world.view(bob).unwrap().location_name, "Neon Alley");
    assert!(!world.view(ada).unwrap().present.iter().any(|p| p.name == "bob"));
    assert!(feed_text(&world, ada).contains("heads toward"));

    world.apply(ada, "say secret").unwrap();
    assert!(!feed_text(&world, bob).contains("secret"));

    world.apply(ada, "chat the wire is up").unwrap();
    assert!(feed_text(&world, bob).contains("the wire is up"));
}

#[test]
fn archive_starts_sealed() {
    let (mut world, ada, _, ada_id, _) = pair();
    world.operators.get_mut(&ada_id).unwrap().location_id = "old-relay".into();
    let err = world.apply(ada, "go archive").unwrap_err();
    assert!(err.to_string().contains("sealed"));
    assert!(world.view(ada).unwrap().exits.iter().all(|exit| exit.keyword != "archive"));
    world.apply(ada, "inspect relay").unwrap();
    assert!(world
        .operator(ada_id)
        .unwrap()
        .items
        .iter()
        .any(|item| item.keyword == "cipher-plate"));
    world.apply(ada, "go archive").unwrap();
    assert_eq!(world.view(ada).unwrap().location_name, "The Black Archive");
}

#[test]
fn bare_exit_word_and_quit() {
    let (mut world, ada, _, _, _) = pair();
    world.apply(ada, "alley").unwrap();
    assert_eq!(world.location_key(ada).unwrap(), "neon-alley");
    let outcome = world.apply(ada, "quit").unwrap();
    assert!(outcome.outcome.quit);
    assert!(world.apply(ada, "frobnicate").is_err());
}

#[test]
fn spectator_cannot_speak_or_move() {
    let mut world = WorldState::for_tests();
    let id = PlayerId::new();
    world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "cy", "cy"));
    let identity = SessionIdentity::new(id, "cy", Permissions::spectator());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    assert!(world.apply(session, "look").is_ok());
    assert!(world.apply(session, "say hi").is_err());
    assert!(world.apply(session, "go alley").is_err());
}

#[test]
fn reconnect_keeps_location_and_replaces_session() {
    let (mut world, ada, _, ada_id, _) = pair();
    world.apply(ada, "go up").unwrap();
    let again = SessionIdentity::new(ada_id, "ada", Permissions::player());
    let new_session = again.session_id;
    let (_transition, replaced) = world.connect_player(again).unwrap();
    assert_eq!(replaced, Some(ada));
    assert!(world.view(ada).is_err());
    assert_eq!(world.location_key(new_session).unwrap(), "roof-array");
}

#[test]
fn every_command_has_a_handler() {
    let world = WorldState::new();
    for spec in world.registry().specs() {
        assert!(find_command(&world.commands, spec.name).is_some(), "{}", spec.name);
    }
}

#[test]
fn command_rollback_round_trip() {
    let (mut world, ada, _, _, _) = pair();
    world.set_overlay(ada, "held note");
    let snap = world.command_rollback_snapshot();
    world.overlays.clear();
    world.restore_command_rollback(snap);
    assert_eq!(world.overlays.get(&ada).map(String::as_str), Some("held note"));
}

#[test]
fn disconnect_does_not_remove_the_other_player() {
    let (mut world, ada, bob, _, _) = pair();
    world.detach(ada).unwrap();
    assert!(world.view(bob).is_ok());
    assert!(feed_text(&world, bob).contains("disconnects"));
}

#[test]
fn outskirts_can_be_walked_marked_and_left() {
    let (mut world, ada, bob, ada_id, _) = pair();
    world.apply(ada, "go up").unwrap();
    let roof = world.view(ada).unwrap();
    assert!(roof.exits.iter().any(|exit| exit.keyword == "outskirts"));
    world.apply(ada, "outskirts").unwrap();
    let arrived = world.view(ada).unwrap();
    assert_eq!(arrived.location_name, "The Squalor");
    assert!(arrived.description.contains("Roof Array"));
    assert!(arrived.exits.iter().any(|exit| exit.keyword == "city"));
    assert!(world.location_key(ada).unwrap().starts_with("w:undernet:1:0:0:1:"));
    assert_eq!(world.geography.regions.len(), 1);
    let identity = world.geography.regions.values().next().unwrap().identity;
    assert_eq!(identity.generation_version, 1);
    assert_eq!(identity.origin_x, 0);
    assert_eq!(identity.origin_y, 0);

    world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "trail", ada_id).unwrap();
    world.apply(ada, "north").unwrap();
    let moved = world.location_key(ada).unwrap();
    assert!(moved.ends_with(":8:9"), "{moved}");
    world.apply(ada, "mark camp").unwrap();
    let marked = world.view(ada).unwrap();
    assert!(marked.objects.iter().any(|object| object.keyword == "camp"));
    world.apply(ada, "map").unwrap();
    let mapped = world.view(ada).unwrap().overlay.unwrap_or_default();
    assert!(mapped.contains("birth (0, 0)"));
    assert!(mapped.contains('@'));

    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    assert!(!world.view(ada).unwrap().present.iter().any(|person| person.name == "bob"));
    world.apply(bob, "north").unwrap();
    assert!(world.view(ada).unwrap().present.iter().any(|person| person.name == "bob"));
    world.apply(ada, "say quiet").unwrap();
    assert!(feed_text(&world, bob).contains("quiet"));

    world.apply(ada, "south").unwrap();
    world.apply(ada, "city").unwrap();
    assert_eq!(world.view(ada).unwrap().location_name, "Roof Array");
    assert_eq!(world.geography.structures.len(), 1);
    assert_eq!(world.geography.structures[0].owner_id, Some(ada_id));
}

#[test]
fn gate_description_matches_movement_and_the_return_path() {
    let (mut world, ada, _, _, _) = pair();
    let region = crate::geography::generate_region(crate::geography::gate_identity()).unwrap();
    assert_eq!(crate::geography::fingerprint(&region), 0x8812_F1DB_241C_39A1);
    assert_eq!(region.tile(8, 8).unwrap().terrain, Terrain::Water);

    world.apply(ada, "go up").unwrap();
    world.apply(ada, "outskirts").unwrap();
    let arrived = world.view(ada).unwrap();
    let exits: Vec<&str> = arrived.exits.iter().map(|exit| exit.keyword.as_str()).collect();
    assert_eq!(exits, ["west", "city"]);
    assert!(arrived.description.contains("Open: west"), "{}", arrived.description);
    assert!(
        arrived.description.contains("Floodwater blocks the way north, east, and south"),
        "{}",
        arrived.description
    );
    assert!(arrived.objects.iter().any(|object| object.keyword == "ground"));
    assert!(arrived.objects.iter().all(|object| object.keyword != "water"));
    for dir in ["north", "east", "south"] {
        let err = world.apply(ada, dir).unwrap_err().to_string();
        assert!(err.contains("Floodwater blocks"), "{dir}: {err}");
        assert_eq!(world.location_key(ada).unwrap(), gate_position_key());
    }
    world.apply(ada, "west").unwrap();
    assert_eq!(world.location_key(ada).unwrap(), crate::geography::survey_position().key());
    world.apply(ada, "east").unwrap();
    world.apply(ada, "city").unwrap();
    assert_eq!(world.view(ada).unwrap().location_name, "Roof Array");
    world.apply(ada, "outskirts").unwrap();
    assert_eq!(world.view(ada).unwrap().location_name, "The Squalor");
}

fn gate_position_key() -> String {
    crate::geography::gate_position().key()
}

#[test]
fn inspection_remains_beside_the_place() {
    let (mut world, ada, bob, _, _) = pair();
    world.apply(ada, "inspect counter").unwrap();
    let noticed = world.view(ada).unwrap();
    assert!(noticed.description.contains("Steam"));
    assert!(noticed.finding.as_deref().unwrap_or("").contains("scarred laminate"));
    world.apply(ada, "say still here").unwrap();
    let after_say = world.view(ada).unwrap();
    assert!(after_say.description.contains("Steam"));
    assert!(after_say.finding.as_deref().unwrap_or("").contains("scarred laminate"));
    assert!(world.view(bob).unwrap().finding.is_none());
    assert!(!feed_text(&world, bob).contains("scarred laminate"));
    world.apply(ada, "go alley").unwrap();
    let moved = world.view(ada).unwrap();
    assert!(moved.description.contains("service alley"));
    assert!(moved.finding.as_deref().unwrap_or("").contains("scarred laminate"));
    world.apply(ada, "look").unwrap();
    assert!(world.view(ada).unwrap().finding.as_deref().unwrap_or("").contains("laminate"));
    world.apply(ada, "journal").unwrap();
    let journal = world.view(ada).unwrap();
    assert!(journal.overlay.as_deref().unwrap_or("").contains("empty"));
    assert!(journal.description.contains("service alley"));
    assert_eq!(journal.location_name, "Neon Alley");
    assert!(journal.finding.as_deref().unwrap_or("").contains("laminate"));
}

#[test]
fn wilderness_rejects_invalid_actions() {
    let (mut world, ada, _, ada_id, _) = pair();
    let err = world.apply(ada, "mark camp").unwrap_err();
    assert!(err.to_string().contains("outskirts"));
    let err = world.apply(ada, "go north").unwrap_err();
    assert!(err.to_string().contains("can't go"));
    let err = world.apply(ada, "go w:undernet:1:5:5:1:0:0").unwrap_err();
    assert!(err.to_string().contains("can't go"));

    world.apply(ada, "go up").unwrap();
    world.apply(ada, "outskirts").unwrap();
    assert!(world.apply(ada, "mark n").is_err());
    assert!(world.apply(ada, "mark north").is_err());
    let before = world.geography_snapshot();
    world.apply(ada, "mark camp").unwrap();
    assert!(world.apply(ada, "mark camp").unwrap_err().to_string().contains("already"));
    assert_eq!(world.geography.structures[0].owner_id, Some(ada_id));
    let marked = world.geography_snapshot();
    world.restore_geography(before);
    assert!(world.geography.structures.is_empty());
    world.install_geography(marked);
    assert_eq!(world.geography.structures.len(), 1);
}

#[test]
fn water_blocks_movement_and_a_mod_can_open_it() {
    let (mut world, ada, _, ada_id, _) = pair();
    let region = crate::geography::generate_region(crate::geography::gate_identity()).unwrap();
    let mut water = None;
    let mut stand = None;
    for y in 0..16u8 {
        for x in 0..16u8 {
            if (x, y) == (crate::geography::GATE_LOCAL_X, crate::geography::GATE_LOCAL_Y) {
                continue;
            }
            let tile = region.tile(x, y).unwrap();
            if tile.terrain != Terrain::Water {
                continue;
            }
            for (dx, dy, dir) in
                [(1, 0, "west"), (-1, 0, "east"), (0, 1, "south"), (0, -1, "north")]
            {
                let nx = i32::from(x) + dx;
                let ny = i32::from(y) + dy;
                if !(0..16).contains(&nx) || !(0..16).contains(&ny) {
                    continue;
                }
                let Ok(nx) = u8::try_from(nx) else { continue };
                let Ok(ny) = u8::try_from(ny) else { continue };
                let neighbor = region.tile(nx, ny).unwrap();
                if neighbor.terrain.traversable() {
                    water = Some((x, y, dir));
                    stand = Some((nx, ny));
                }
            }
        }
    }
    let (wx, wy, back) = water.expect("region 0,0 has water beside walkable ground");
    let (sx, sy) = stand.unwrap();
    let pos =
        WildernessPos { identity: crate::geography::gate_identity(), local_x: sx, local_y: sy };
    world.operators.get_mut(&ada_id).unwrap().location_id = pos.key();
    let err = world.apply(ada, back).unwrap_err();
    assert!(err.to_string().contains("Floodwater") || err.to_string().contains("Water"), "{err}");

    world.ensure_region(pos.identity, Some(ada_id), sx, sy).unwrap();
    world.apply_terrain_mod(pos.identity, wx, wy, Terrain::Grass, "ford", ada_id).unwrap();
    world.apply(ada, back).unwrap();
    let arrived = WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap();
    assert_eq!((arrived.local_x, arrived.local_y), (wx, wy));
}

#[test]
fn crossing_a_region_edge_keeps_identity_stable() {
    let (mut world, ada, _, ada_id, _) = pair();
    let identity = crate::geography::gate_identity();
    let edge = WildernessPos { identity, local_x: 15, local_y: 1 };
    world.operators.get_mut(&ada_id).unwrap().location_id = edge.key();
    world.ensure_region(identity, Some(ada_id), 15, 1).unwrap();
    let next = RegionIdentity { origin_x: 1, ..identity };
    world.ensure_region(next, Some(ada_id), 0, 1).unwrap();
    world.apply_terrain_mod(identity, 15, 1, Terrain::Grass, "edge", ada_id).unwrap();
    world.apply_terrain_mod(next, 0, 1, Terrain::Grass, "edge", ada_id).unwrap();
    world.apply(ada, "east").unwrap();
    let arrived = WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap();
    assert_eq!(arrived.identity.origin_x, 1);
    assert_eq!(arrived.local_x, 0);
    assert_eq!(arrived.identity.generation_version, 1);
    assert_eq!(world.geography.regions.len(), 2);

    let birth = world.geography.region(identity.coord_key()).unwrap().identity;
    let op = world.relocate_region(identity, 80, -4).unwrap();
    assert!(matches!(op, PersistOp::RelocateRegion { placement_x: 80, placement_y: -4, .. }));
    let stored = world.geography.region(identity.coord_key()).unwrap();
    assert_eq!(stored.identity, birth);
    assert_eq!((stored.placement_x, stored.placement_y), (80, -4));
    assert_eq!(stored.region_seed, derive_region_seed(identity).unwrap());
}

#[test]
fn links_reject_reserved_words_and_unknown_places() {
    let (mut world, _, _, ada_id, _) = pair();
    let err = world.link_places("coffee-shop", "north", "neon-alley", "North", ada_id).unwrap_err();
    assert!(err.to_string().contains("reserved"));
    let err = world.link_places("coffee-shop", "path", "nowhere", "Nowhere", ada_id).unwrap_err();
    assert!(err.to_string().contains("not in the world"));
    let err = world.link_places("coffee-shop", "path", "coffee-shop", "Loop", ada_id).unwrap_err();
    assert!(err.to_string().contains("itself"));
}

#[test]
fn unknown_generation_version_stays_addressable() {
    let (mut world, ada, _, ada_id, _) = pair();
    let mut identity = crate::geography::gate_identity();
    identity.origin_x = 3;
    identity.origin_y = -4;
    identity.generation_version = 9;
    let id = uuid::Uuid::new_v4();
    world.install_geography(GeographyState {
        regions: HashMap::from([(
            identity.coord_key(),
            MaterializedRegion {
                id,
                identity,
                region_seed: 99,
                placement_x: identity.origin_x,
                placement_y: identity.origin_y,
                discovered_at: 1,
            },
        )]),
        ..GeographyState::default()
    });
    let pos = WildernessPos { identity, local_x: 1, local_y: 1 };
    world.operators.get_mut(&ada_id).unwrap().location_id = pos.key();
    let view = world.view(ada).unwrap();
    assert!(view.description.contains("version 9"), "{}", view.description);
    assert_eq!(world.geography.regions.len(), 1);
    assert!(world.apply(ada, "north").is_err());
    assert_eq!(world.location_key(ada).unwrap(), pos.key());
}

#[test]
fn spectator_cannot_mark() {
    let mut world = WorldState::for_tests();
    let id = PlayerId::new();
    world.insert_operator(starter_operator(id, uuid::Uuid::new_v4(), "cy", "cy"));
    let identity = SessionIdentity::new(id, "cy", Permissions::spectator());
    let session = identity.session_id;
    world.connect_player(identity).unwrap();
    world.operators.get_mut(&id).unwrap().location_id = crate::geography::gate_position().key();
    assert!(world.apply(session, "mark camp").is_err());
    assert!(world.geography.structures.is_empty());
}

fn to_buildable_tile(world: &mut WorldState, session: SessionId, player: PlayerId) {
    world.apply(session, "go up").unwrap();
    world.apply(session, "outskirts").unwrap();
    let identity = crate::geography::gate_identity();
    world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "camp", player).unwrap();
    world.apply(session, "north").unwrap();
}

#[test]
fn whisper_is_private_and_offline_is_refused() {
    let (mut world, ada, bob, _, bob_id) = pair();
    world.apply(ada, "whisper bob secret-ping").unwrap();
    assert!(feed_text(&world, bob).contains("secret-ping"));
    assert!(feed_text(&world, ada).contains("You whisper to bob"));
    // A third party must not see private contents in their feed.
    world.apply(bob, "go alley").unwrap();
    let bob_feed = feed_text(&world, bob);
    assert!(bob_feed.contains("secret-ping"));
    world.detach(bob).unwrap();
    let err = world.apply(ada, "whisper bob later").unwrap_err();
    assert!(err.to_string().contains("offline"), "{err}");
    let _ = bob_id;
}

#[test]
fn friendship_requires_accept_and_persists_in_memory() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    world.apply(ada, "friend add bob").unwrap();
    assert!(!world.social.are_friends(ada_id, bob_id));
    world.apply(bob, "friend accept ada").unwrap();
    assert!(world.social.are_friends(ada_id, bob_id));
    world.apply(ada, "friends").unwrap();
    let overlay = world.view(ada).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("bob"));
    assert!(overlay.contains("online"));
    world.apply(bob, "friend remove ada").unwrap();
    assert!(!world.social.are_friends(ada_id, bob_id));
}

#[test]
fn ignore_filters_chat_and_blocks_whisper() {
    let (mut world, ada, bob, _, _) = pair();
    world.apply(ada, "ignore bob").unwrap();
    world.apply(bob, "say ignored-local").unwrap();
    world.apply(bob, "chat ignored-global").unwrap();
    let ada_feed = feed_text(&world, ada);
    assert!(!ada_feed.contains("ignored-local"), "{ada_feed}");
    assert!(!ada_feed.contains("ignored-global"), "{ada_feed}");
    let err = world.apply(bob, "whisper ada hi").unwrap_err();
    assert!(err.to_string().contains("ignore"), "{err}");
    let err = world.apply(bob, "friend add ada").unwrap_err();
    assert!(err.to_string().contains("ignore"), "{err}");
}

#[test]
fn public_sign_discovered_by_another_operator_without_private_leaks() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    world.apply(ada, "go up").unwrap();
    world.apply(ada, "journal record").unwrap();
    world.apply(ada, "outskirts").unwrap();
    let identity = crate::geography::gate_identity();
    world
        .apply_terrain_mod(identity, 8, 9, crate::geography::Terrain::Grass, "camp", ada_id)
        .unwrap();
    world.apply(ada, "north").unwrap();
    world.apply(ada, "sign Relay gap worth a look").unwrap();
    world.apply(ada, "waypoint add camp-watch").unwrap();
    world.apply(ada, "waypoint note camp-watch Ada-only ridge tally").unwrap();
    let place = world.location_key(ada).unwrap();
    world.detach(ada).unwrap();

    to_buildable_tile(&mut world, bob, bob_id);
    assert_eq!(world.location_key(bob).unwrap(), place);
    let view = world.view(bob).unwrap();
    assert!(
        view.description.contains("Relay gap worth a look"),
        "expected sign prose in look, got: {}",
        view.description
    );
    assert!(view.description.contains("ada"));
    assert!(view.objects.iter().any(|obj| obj.keyword == "sign"));
    assert!(!view.description.contains("Ada-only ridge tally"));
    assert!(!view.description.contains("one step west of the gate"));

    world.apply(bob, "inspect sign").unwrap();
    let bob_feed = feed_text(&world, bob);
    assert!(bob_feed.contains("Relay gap worth a look"), "{bob_feed}");
    assert!(!bob_feed.contains("Ada-only ridge tally"), "{bob_feed}");

    let leaked = world.apply(bob, "journal read survey-scratch").unwrap_err();
    assert!(leaked.to_string().contains("No journal entry matches"), "{leaked}");
    world.apply(bob, "waypoint list").unwrap();
    let wps = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(!wps.contains("Ada-only ridge tally"), "{wps}");
    assert!(!wps.contains("camp-watch"), "{wps}");
}

#[test]
fn inspect_sign_requires_a_public_sign() {
    let (mut world, ada, _, ada_id, _) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    let err = world.apply(ada, "inspect sign").unwrap_err();
    assert!(err.to_string().contains("nothing called"), "{err}");
}

#[test]
fn cabin_build_enter_visit_and_private_access() {
    let (mut world, ada, bob, ada_id, _) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "build cabin Haven").unwrap();
    assert_eq!(world.geography.structures.iter().filter(|s| s.kind == "cabin").count(), 1);
    world.apply(ada, "enter Haven").unwrap();
    assert!(world.view(ada).unwrap().location_name.contains("Haven"));
    world.apply(ada, "leave").unwrap();
    assert!(!world.location_key(ada).unwrap().starts_with("i:"));

    // Bob can visit a public cabin.
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    world.apply(bob, "enter Haven").unwrap();
    assert!(world.view(bob).unwrap().location_name.contains("Haven"));
    world.apply(bob, "leave").unwrap();

    world.apply(ada, "home access private").unwrap();
    let err = world.apply(bob, "enter Haven").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");
    world.apply(ada, "home allow bob").unwrap();
    world.apply(bob, "enter Haven").unwrap();
    world.apply(bob, "leave").unwrap();

    world.apply(ada, "sign Welcome friends").unwrap();
    world.apply(ada, "sign read").unwrap();
    assert!(feed_text(&world, ada).contains("Welcome friends"));
    world.apply(ada, "trail Ridge").unwrap();
    assert!(world.geography.structures.iter().any(|s| s.kind == "trail"));
    // Marker can coexist with cabin/sign/trail on the same tile.
    world.apply(ada, "mark Camp").unwrap();
    assert!(
        world
            .geography
            .structures_at(
                world.geography.structures.iter().find(|s| s.kind == "cabin").unwrap().region_id,
                WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap().local_x,
                WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap().local_y,
            )
            .len()
            >= 4
    );
}

#[test]
fn cabin_demolish_evacuates_occupants() {
    let (mut world, ada, bob, ada_id, _) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "build cabin Nest").unwrap();
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    world.apply(bob, "enter Nest").unwrap();
    assert!(world.location_key(bob).unwrap().starts_with("i:"));
    world.apply(ada, "home demolish confirm").unwrap();
    assert!(!world.location_key(bob).unwrap().starts_with("i:"));
    assert!(!world.geography.structures.iter().any(|s| s.kind == "cabin"));
    assert!(world.geography.connections.is_empty());
}

#[test]
fn noticeboard_posts_and_owner_removal() {
    let (mut world, ada, bob, _, _) = pair();
    world.apply(ada, "board post Meet at Haven").unwrap();
    world.apply(bob, "board").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("Meet at Haven"));
    let id8 = world.social.notices[0].id.to_string()[..8].to_string();
    let err = world.apply(bob, &format!("board remove {id8}")).unwrap_err();
    assert!(err.to_string().contains("your own"), "{err}");
    world.apply(ada, &format!("board remove {id8}")).unwrap();
    assert!(world.social.notices.is_empty());
}

#[test]
fn profile_bio_is_public_without_account_secrets() {
    let (mut world, ada, bob, _, _) = pair();
    world.apply(ada, "profile bio Likes rain and relays").unwrap();
    world.apply(ada, "profile status exploring").unwrap();
    world.apply(bob, "inspect ada").unwrap();
    let feed = feed_text(&world, bob);
    assert!(feed.contains("Likes rain and relays"));
    assert!(feed.contains("exploring"));
    assert!(!feed.contains("password"));
    assert!(!feed.contains('@'));
}

#[test]
fn settlement_create_invite_accept_and_duplicate_name() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "settle create Haven").unwrap();
    assert!(world.settlements.settlement_of(ada_id).is_some());
    let err = world.apply(ada, "settle create Other").unwrap_err();
    assert!(err.to_string().contains("already belong"), "{err}");

    // Bob on same tile tries duplicate name.
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    let err = world.apply(bob, "settle create Haven").unwrap_err();
    assert!(err.to_string().contains("taken"), "{err}");

    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "settle accept Haven").unwrap();
    assert!(world
        .settlements
        .is_member(world.settlements.settlement_of(ada_id).unwrap().id, bob_id));
    world.apply(ada, "settle members").unwrap();
    let overlay = world.view(ada).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("bob"));
}

#[test]
fn settlement_invite_blocked_by_ignore_and_decline() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "settle create Ridge").unwrap();
    world.apply(bob, "ignore ada").unwrap();
    let err = world.apply(ada, "settle invite bob").unwrap_err();
    assert!(err.to_string().contains("blocked"), "{err}");
    world.apply(bob, "unignore ada").unwrap();
    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "settle decline Ridge").unwrap();
    assert!(world.settlements.membership_of(bob_id).is_none());
}

#[test]
fn settlement_kick_transfer_leave_and_disband_preserve_structures() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "build cabin Nest").unwrap();
    world.apply(ada, "settle create Nestland").unwrap();
    let settle_id = world.settlements.settlement_of(ada_id).unwrap().id;
    assert!(world
        .geography
        .structures
        .iter()
        .any(|s| s.kind == "cabin" && s.settlement_id == Some(settle_id)));

    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "settle accept Nestland").unwrap();

    world.apply(ada, "settle kick bob").unwrap();
    assert!(!world.settlements.is_member(settle_id, bob_id));
    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "settle accept Nestland").unwrap();
    world.apply(ada, "settle transfer bob").unwrap();
    assert!(world.settlements.is_owner(settle_id, bob_id));
    assert!(!world.settlements.is_owner(settle_id, ada_id));

    // Former owner can leave as member.
    world.apply(ada, "settle leave").unwrap();
    assert!(world.settlements.membership_of(ada_id).is_none());

    // Sole owner leave disbands; cabin remains.
    world.apply(bob, "settle leave").unwrap();
    assert!(world.settlements.settlements.is_empty());
    assert!(world.geography.structures.iter().any(|s| s.kind == "cabin" && s.name == "Nest"));
    assert!(world
        .geography
        .structures
        .iter()
        .filter(|s| s.kind == "cabin")
        .all(|s| s.settlement_id.is_none()));
}

#[test]
fn regional_chat_shares_exterior_and_cabin_isolates_rooms() {
    let (mut world, ada, bob, ada_id, _) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "build cabin Lookout").unwrap();
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();

    world.apply(ada, "region ridge-call").unwrap();
    assert!(feed_text(&world, bob).contains("ridge-call"));
    world.apply(ada, "enter Lookout").unwrap();
    world.apply(ada, "region inside-call").unwrap();
    assert!(feed_text(&world, bob).contains("inside-call"));

    // Different Undernet rooms do not share region chat.
    world.apply(bob, "south").unwrap();
    let bob_loc = world.location_key(bob).unwrap();
    assert!(bob_loc.contains(":8:8") || bob_loc.ends_with(":8:8"), "bob at {bob_loc}");
    world.apply(bob, "city").unwrap();
    world.apply(bob, "go down").unwrap();
    world.apply(ada, "leave").unwrap();
    world.apply(ada, "south").unwrap();
    world.apply(ada, "city").unwrap();
    world.apply(ada, "go down").unwrap();
    world.apply(ada, "go alley").unwrap();
    world.apply(ada, "region alley-only").unwrap();
    let bob_feed = feed_text(&world, bob);
    assert!(!bob_feed.contains("alley-only"), "{bob_feed}");
    world.apply(bob, "region shop-only").unwrap();
    assert!(!feed_text(&world, ada).contains("shop-only"));
}

#[test]
fn regional_chat_respects_ignore_and_leaves_global() {
    let (mut world, ada, bob, _, _) = pair();
    world.apply(ada, "ignore bob").unwrap();
    world.apply(bob, "region ignored-region").unwrap();
    assert!(!feed_text(&world, ada).contains("ignored-region"));
    world.apply(bob, "chat still-global").unwrap();
    assert!(!feed_text(&world, ada).contains("still-global"));
    world.apply(ada, "unignore bob").unwrap();
    world.apply(bob, "chat heard-global").unwrap();
    assert!(feed_text(&world, ada).contains("heard-global"));
}

#[test]
fn settlement_discovery_and_board_permissions() {
    let (mut world, ada, bob, ada_id, _) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "settle create PublicHold").unwrap();
    world.apply(ada, "settle describe A windy ridge").unwrap();
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    world.apply(bob, "settlements").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("PublicHold"), "{overlay}");

    world.apply(ada, "settle access private").unwrap();
    world.apply(bob, "settlements").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(!overlay.contains("PublicHold"), "{overlay}");
    let err = world.apply(bob, "settle inspect PublicHold").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");

    world.apply(ada, "settle access public").unwrap();
    world.apply(ada, "settle board post Meet at dusk").unwrap();
    world.apply(bob, "settle board PublicHold").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("Meet at dusk"), "{overlay}");
    let err = world.apply(bob, "settle board PublicHold post outsider").unwrap_err();
    assert!(err.to_string().contains("members"), "{err}");
}

#[test]
fn settle_invites_inbox_and_offline_accept() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "settle create Haven").unwrap();
    world.apply(ada, "settle invite bob").unwrap();

    // Ada must not see Bob's inbox contents.
    world.apply(ada, "settle invites").unwrap();
    let ada_overlay = world.view(ada).unwrap().overlay.unwrap_or_default();
    assert!(ada_overlay.contains("none"), "{ada_overlay}");

    world.apply(bob, "settle invites").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("Haven"), "{overlay}");
    assert!(overlay.contains("settle accept Haven"), "{overlay}");
    assert!(overlay.contains("settle decline Haven"), "{overlay}");

    world.apply(bob, "settle accept Haven").unwrap();
    assert!(world
        .settlements
        .is_member(world.settlements.settlement_of(ada_id).unwrap().id, bob_id));
    world.apply(bob, "settle invites").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("none"), "{overlay}");
}

#[test]
fn expired_invite_cannot_join_and_reject_does_not_prune() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "settle create Haven").unwrap();
    world.apply(ada, "settle invite bob").unwrap();
    let invite_id = world.settlements.invites[0].id;
    // Force expiry without pruning.
    world.now_ms = world.settlements.invites[0].expires_at;

    // Rejected accept of a wrong name must not prune/expire in memory alone.
    let err = world.apply(bob, "settle accept NoSuchHold").unwrap_err();
    assert!(err.to_string().contains("invite"), "{err}");
    let invite = world.settlements.invites.iter().find(|i| i.id == invite_id).unwrap();
    assert_eq!(invite.status, crate::settlement::InviteStatus::Pending);

    // Accept of expired invite expires it durably (Ok path) without joining.
    world.apply(bob, "settle accept Haven").unwrap();
    assert!(world.settlements.membership_of(bob_id).is_none());
    let invite = world.settlements.invites.iter().find(|i| i.id == invite_id).unwrap();
    assert_eq!(invite.status, crate::settlement::InviteStatus::Expired);

    // Inbox stays empty for actionable invites.
    world.apply(bob, "settle invites").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("none"), "{overlay}");
    assert!(!overlay.contains("settle accept Haven"), "{overlay}");
    let _ = ada_id;
}

#[test]
fn private_cabin_stays_private_until_shared_then_leave_revokes() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "build cabin Nest").unwrap();
    world.apply(ada, "home access private").unwrap();
    world.apply(ada, "settle create Nestland").unwrap();
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "settle accept Nestland").unwrap();

    // Membership alone does not open a private cabin.
    let err = world.apply(bob, "enter Nest").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");
    let err = world.apply(bob, "go nest").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");

    // Non-owner cannot change sharing.
    let err = world.apply(bob, "home share shared").unwrap_err();
    assert!(err.to_string().contains("owner"), "{err}");

    world.apply(ada, "home share shared").unwrap();
    world.apply(bob, "enter Nest").unwrap();
    world.apply(bob, "leave").unwrap();

    world.apply(bob, "settle leave").unwrap();
    assert!(world.settlements.membership_of(bob_id).is_none());
    let err = world.apply(bob, "enter Nest").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");
    let _ = ada_id;
}

#[test]
fn communal_workshop_board_and_disband_fallback() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "settle create Forgehold").unwrap();
    world.apply(ada, "build workshop Anvil").unwrap();
    let workshop =
        world.geography.structures.iter().find(|s| s.kind == "workshop").cloned().unwrap();
    assert_eq!(workshop.share, "private");
    assert_eq!(workshop.settlement_id, world.settlements.settlement_of(ada_id).map(|s| s.id));

    world.apply(ada, "home share communal").unwrap();
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    let err = world.apply(bob, "enter Anvil").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");

    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "settle accept Forgehold").unwrap();
    world.apply(bob, "enter Anvil").unwrap();
    world.apply(bob, "workshop board post Fit new hinges").unwrap();
    world.apply(bob, "workshop board").unwrap();
    let overlay = world.view(bob).unwrap().overlay.unwrap_or_default();
    assert!(overlay.contains("Fit new hinges"), "{overlay}");
    world.apply(bob, "leave").unwrap();

    // Disband preserves the workshop and reverts share to private.
    world.apply(ada, "settle disband confirm").unwrap();
    assert!(world.settlements.settlements.is_empty());
    let workshop = world
        .geography
        .structures
        .iter()
        .find(|s| s.kind == "workshop" && s.name == "Anvil")
        .unwrap();
    assert!(workshop.settlement_id.is_none());
    assert_eq!(workshop.share, "private");
    let err = world.apply(bob, "enter Anvil").unwrap_err();
    assert!(err.to_string().contains("private"), "{err}");
    // Owner still enters.
    world.apply(ada, "enter Anvil").unwrap();
    let _ = bob_id;
}

#[test]
fn structure_access_precedence_owner_grant_share() {
    let (mut world, ada, bob, ada_id, bob_id) = pair();
    to_buildable_tile(&mut world, ada, ada_id);
    world.apply(ada, "build cabin Keep").unwrap();
    world.apply(ada, "home access private").unwrap();
    let cabin = world.geography.structures.iter().find(|s| s.kind == "cabin").cloned().unwrap();
    assert!(world.ensure_structure_access_for_move(ada_id, &cabin).is_ok());
    assert!(world.ensure_structure_access_for_move(bob_id, &cabin).is_err());

    world.apply(ada, "home allow bob").unwrap();
    let cabin = world.geography.structures.iter().find(|s| s.kind == "cabin").cloned().unwrap();
    assert!(world.ensure_structure_access_for_move(bob_id, &cabin).is_ok());
    world.apply(ada, "home deny bob").unwrap();

    world.apply(ada, "settle create Keepstead").unwrap();
    world.apply(ada, "settle invite bob").unwrap();
    world.apply(bob, "go up").unwrap();
    world.apply(bob, "outskirts").unwrap();
    world.apply(bob, "north").unwrap();
    world.apply(bob, "settle accept Keepstead").unwrap();
    world.apply(ada, "home share shared").unwrap();
    let cabin = world.geography.structures.iter().find(|s| s.kind == "cabin").cloned().unwrap();
    assert!(world.ensure_structure_access_for_move(bob_id, &cabin).is_ok());
    assert!(world.settlement_share_allows(bob_id, &cabin));
    assert!(world.settlement_share_allows(ada_id, &cabin));
}
