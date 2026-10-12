//! Subway vertical-slice coverage.

#[cfg(test)]
mod tests {
    use terminus_core::{Game, Permissions, PlayerId, SessionIdentity};
    use uuid::Uuid;

    use crate::geography::WildernessPos;
    use crate::state::{starter_operator, WorldState};
    use crate::subway::{
        self, ash_entrance_pos, ward_entrance_pos, ASH_STATION, SUBWAY_SURFACE_SCALE, WARD_STATION,
    };

    fn session(world: &mut WorldState, name: &str) -> (terminus_core::SessionId, PlayerId) {
        let id = PlayerId::new();
        let op = starter_operator(id, Uuid::new_v4(), name, name);
        world.insert_operator(op);
        let identity = SessionIdentity::new(id, name, Permissions::player());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        (session, id)
    }

    #[test]
    fn subway_hop_covers_eight_surface_tiles_across_regions() {
        let mut world = WorldState::for_tests();
        let (ada, ada_id) = session(&mut world, "ada");

        world.apply(ada, "go up").unwrap();
        world.apply(ada, "outskirts").unwrap();
        for _ in 0..6 {
            world.apply(ada, "west").unwrap();
        }
        assert_eq!(world.location_key(ada).unwrap(), ash_entrance_pos().key());
        let surface = world.view(ada).unwrap();
        assert!(surface.status.iter().any(|line| line == "layer: surface"));
        assert!(surface.exits.iter().any(|exit| exit.keyword == "metro"));

        world.apply(ada, "inspect stair").unwrap();
        let notice = world.view(ada).unwrap().finding.unwrap_or_default();
        assert!(notice.contains("Ash Plaza"), "{notice}");
        assert!(notice.contains("go metro"), "{notice}");

        world.apply(ada, "metro").unwrap();
        let station = world.view(ada).unwrap();
        assert_eq!(station.location_name, "Ash Plaza Station");
        assert!(station.status.iter().any(|line| line == "layer: underground"));
        assert!(station.exits.iter().any(|exit| exit.keyword == "west"));
        assert!(station.exits.iter().any(|exit| exit.keyword == "surface"));

        world.apply(ada, "west").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "Ward West Station");
        assert_eq!(world.location_key(ada).unwrap(), WARD_STATION);

        world.apply(ada, "surface").unwrap();
        assert_eq!(world.location_key(ada).unwrap(), ward_entrance_pos().key());
        let emerged = world.view(ada).unwrap();
        assert!(emerged.status.iter().any(|line| line == "layer: surface"));
        let ash = WildernessPos::parse(&ash_entrance_pos().key()).unwrap();
        let ward = WildernessPos::parse(&world.location_key(ada).unwrap()).unwrap();
        assert_ne!(ash.identity.origin_x, ward.identity.origin_x);
        assert_eq!(subway::entrance_surface_distance(), SUBWAY_SURFACE_SCALE);

        world.apply(ada, "metro").unwrap();
        world.apply(ada, "east").unwrap();
        assert_eq!(world.location_key(ada).unwrap(), ASH_STATION);
        world.apply(ada, "surface").unwrap();
        assert_eq!(world.location_key(ada).unwrap(), ash_entrance_pos().key());

        world.apply(ada, "metro").unwrap();
        world.apply(ada, "waypoint add ash-station").unwrap();
        world.apply(ada, "waypoint show ash-station").unwrap();
        let wp = world.view(ada).unwrap().overlay.unwrap_or_default();
        assert!(wp.contains("Ash Plaza") || wp.contains(ASH_STATION), "{wp}");
        assert!(!world.operator(ada_id).unwrap().flags.contains("cipher-plate"));
        assert!(world.view(ada).unwrap().exits.iter().all(|exit| exit.keyword != "archive"));

        assert!(crate::seed_exits()
            .iter()
            .any(|exit| exit.from == ASH_STATION && exit.to == WARD_STATION));
        assert!(crate::static_location(ASH_STATION).is_some());
    }

    #[test]
    fn squalor_alias_enters_the_gate_and_subway_stays_deterministic() {
        let mut world = WorldState::for_tests();
        let (ada, _) = session(&mut world, "ada");
        world.apply(ada, "go up").unwrap();
        world.apply(ada, "squalor").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "The Squalor");
        world.apply(ada, "city").unwrap();
        assert_eq!(world.view(ada).unwrap().location_name, "Roof Array");

        // Ordinary station hop is the same destination twice.
        world.apply(ada, "outskirts").unwrap();
        for _ in 0..6 {
            world.apply(ada, "west").unwrap();
        }
        world.apply(ada, "metro").unwrap();
        world.apply(ada, "west").unwrap();
        let first = world.location_key(ada).unwrap();
        world.apply(ada, "east").unwrap();
        world.apply(ada, "west").unwrap();
        assert_eq!(world.location_key(ada).unwrap(), first);
        assert_eq!(first, WARD_STATION);
    }
}
