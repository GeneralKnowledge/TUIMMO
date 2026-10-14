//! Remote-world acceptance: generation and presentation ~1000 tiles from the Hub.
//!
//! These tests never walk intervening tiles. A far birth cell is addressed by its
//! stable identity alone.

use super::density::{
    density_band, DensityBand, UrbanContext, REMOTE_ACCEPTANCE_ORIGIN_X, REMOTE_ACCEPTANCE_ORIGIN_Y,
};
use super::generate::{
    fingerprint, generate_region, sample_baseline, EPOCH1_WORLD_SEED, GENERATION_V1,
};
use super::identity::{origins_for_absolute, RegionIdentity, WildernessPos, WorldId, REGION_SIZE};
use super::travel::{describe_tile, effective_tile};
use super::GeographyState;

fn remote_identity() -> RegionIdentity {
    RegionIdentity {
        world: WorldId::Undernet,
        epoch_id: 1,
        origin_x: REMOTE_ACCEPTANCE_ORIGIN_X,
        origin_y: REMOTE_ACCEPTANCE_ORIGIN_Y,
        generation_version: GENERATION_V1,
    }
}

fn first_walkable(identity: RegionIdentity) -> WildernessPos {
    let generated = generate_region(identity).unwrap();
    for local_y in 0..16u8 {
        for local_x in 0..16u8 {
            let pos = WildernessPos { identity, local_x, local_y };
            let baseline = generated.tile(local_x, local_y).unwrap();
            if effective_tile(pos, baseline, &GeographyState::default()).traversable() {
                return pos;
            }
        }
    }
    panic!("remote region has no walkable tile");
}

#[test]
fn remote_region_is_about_one_thousand_tiles_from_the_hub() {
    let abs_x = i64::from(REMOTE_ACCEPTANCE_ORIGIN_X) * i64::from(REGION_SIZE);
    assert_eq!(abs_x, 992);
    assert!((900..=1100).contains(&abs_x));
    let (ox, oy, lx, ly) = origins_for_absolute(abs_x, 0).unwrap();
    assert_eq!((ox, oy, lx, ly), (REMOTE_ACCEPTANCE_ORIGIN_X, 0, 0, 0));
}

#[test]
fn remote_region_generates_without_intervening_cells() {
    let remote = generate_region(remote_identity()).unwrap();
    let again = generate_region(remote_identity()).unwrap();
    assert_eq!(fingerprint(&remote), fingerprint(&again));
    // Origin fingerprint stays the Hub lock; remote must differ.
    let origin = generate_region(super::identity::gate_identity()).unwrap();
    assert_eq!(fingerprint(&origin), 0x8812_F1DB_241C_39A1);
    assert_ne!(fingerprint(&remote), fingerprint(&origin));
}

#[test]
fn generation_order_does_not_change_remote_identity() {
    let after_neighbors = {
        let _a = generate_region(RegionIdentity {
            origin_x: REMOTE_ACCEPTANCE_ORIGIN_X - 1,
            ..remote_identity()
        })
        .unwrap();
        let _b = generate_region(RegionIdentity {
            origin_x: REMOTE_ACCEPTANCE_ORIGIN_X + 1,
            ..remote_identity()
        })
        .unwrap();
        let _c = generate_region(super::identity::gate_identity()).unwrap();
        fingerprint(&generate_region(remote_identity()).unwrap())
    };
    let alone = fingerprint(&generate_region(remote_identity()).unwrap());
    assert_eq!(alone, after_neighbors);
}

#[test]
fn remote_region_has_coherent_urban_content_not_an_empty_fallback() {
    let identity = remote_identity();
    let generated = generate_region(identity).unwrap();
    let geo = GeographyState::default();
    let mut walkable = 0u32;
    let mut landmarks = 0u32;
    let mut textured = 0u32;
    let mut districts = std::collections::BTreeSet::new();
    let mut blocks = std::collections::BTreeSet::new();
    for local_y in 0..16u8 {
        for local_x in 0..16u8 {
            let pos = WildernessPos { identity, local_x, local_y };
            let baseline = generated.tile(local_x, local_y).unwrap();
            let tile = effective_tile(pos, baseline, &geo);
            let ctx = UrbanContext::at(pos);
            districts.insert(ctx.district.short());
            blocks.insert(ctx.block.name());
            assert_eq!(ctx.band, DensityBand::Deep);
            assert_eq!(density_band(pos.absolute().0, pos.absolute().1), DensityBand::Deep);
            if tile.traversable() {
                walkable += 1;
                if tile.landmark.is_some() {
                    landmarks += 1;
                }
                if ctx.trace.is_some() {
                    textured += 1;
                }
            }
        }
    }
    assert!(walkable >= 64, "walkable={walkable}");
    assert!(landmarks >= 1, "landmarks={landmarks}");
    assert!(textured >= 1, "textured={textured}");
    assert!(!districts.is_empty());
    assert!(!blocks.is_empty());
    // Authored Hub clinic story must not appear a thousand tiles away.
    assert!(UrbanContext::at(first_walkable(identity)).story.is_none());
}

#[test]
fn remote_boundary_matches_absolute_field_with_neighbour() {
    let west = generate_region(remote_identity()).unwrap();
    let east = generate_region(RegionIdentity {
        origin_x: REMOTE_ACCEPTANCE_ORIGIN_X + 1,
        ..remote_identity()
    })
    .unwrap();
    let west_edge_abs = i64::from(REMOTE_ACCEPTANCE_ORIGIN_X) * i64::from(REGION_SIZE) + 15;
    let east_edge_abs = i64::from(REMOTE_ACCEPTANCE_ORIGIN_X + 1) * i64::from(REGION_SIZE);
    assert_eq!(east_edge_abs, west_edge_abs + 1);
    for y in 0..16u8 {
        let abs_y = i64::from(y);
        assert_eq!(
            west.tile(15, y).unwrap(),
            sample_baseline(EPOCH1_WORLD_SEED, west_edge_abs, abs_y),
            "west edge y={y}"
        );
        assert_eq!(
            east.tile(0, y).unwrap(),
            sample_baseline(EPOCH1_WORLD_SEED, east_edge_abs, abs_y),
            "east edge y={y}"
        );
        // District identity is continuous across the absolute seam.
        assert_eq!(
            UrbanContext::at(WildernessPos {
                identity: remote_identity(),
                local_x: 15,
                local_y: y,
            })
            .district,
            UrbanContext::at(WildernessPos {
                identity: RegionIdentity {
                    origin_x: REMOTE_ACCEPTANCE_ORIGIN_X + 1,
                    ..remote_identity()
                },
                local_x: 0,
                local_y: y,
            })
            .district
        );
    }
}

#[test]
fn remote_look_carries_district_familiarity() {
    let pos = first_walkable(remote_identity());
    let generated = generate_region(pos.identity).unwrap();
    let scene = describe_tile(
        pos,
        &generated,
        &GeographyState::default(),
        &[],
        &[],
        &Default::default(),
    );
    assert!(scene.title.contains('—'), "{}", scene.title);
    assert!(
        scene.title.contains("residential")
            || scene.title.contains("commercial")
            || scene.title.contains("civic"),
        "{}",
        scene.title
    );
    assert!(scene.map.contains("deep Squalor"), "{}", scene.map);
    assert!(!scene.description.is_empty());
}
