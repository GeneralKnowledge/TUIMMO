//! Tests for cross-region long-procession spine.

use crate::geography::{frontier_epoch, frontier_identity, origins_for_absolute, WorldId};
use crate::threads::long_procession::{
    breadcrumb_text, build_spine_schedule, first_clue_of_chapter,
    materialize_long_procession_for_region, region_pair_hash, shared_clues_for_region_pair,
    spine_id, CHAPTER_1_CLUES,
};
use crate::threads::procedural::ProceduralRegion;
use crate::threads::CoverageParams;

#[test]
fn spine_id_stable_per_seed() {
    let seed = frontier_epoch().world_seed;
    assert_eq!(spine_id(seed), spine_id(seed));
    assert_ne!(spine_id(seed), spine_id(seed.wrapping_add(1)));
}

#[test]
fn cross_region_generation_order_agrees() {
    let seed = frontier_epoch().world_seed;
    let a = frontier_identity(WorldId::Undernet, 2, 0);
    let b = frontier_identity(WorldId::Undernet, 3, 0);
    let _pair = region_pair_hash(seed, a, b);
    let (only_a, only_b) = shared_clues_for_region_pair(seed, a, b);
    let mut combined_ab = ProceduralRegion::default();
    let mut used = std::collections::HashSet::new();
    materialize_long_procession_for_region(
        seed,
        a,
        &mut combined_ab,
        &mut used,
        CoverageParams::default(),
    )
    .unwrap();
    used.clear();
    materialize_long_procession_for_region(
        seed,
        b,
        &mut combined_ab,
        &mut used,
        CoverageParams::default(),
    )
    .unwrap();

    for (id, clue, pos) in &only_a {
        let c = combined_ab.clues.get(id).expect("clue in combined");
        assert_eq!(c.surface_text, clue.surface_text);
        assert_eq!(combined_ab.sites.get(id).unwrap(), &crate::threads::ClueSite::Wild(*pos));
    }
    for (id, clue, pos) in &only_b {
        let c = combined_ab.clues.get(id).expect("clue in combined");
        assert_eq!(c.surface_text, clue.surface_text);
        assert_eq!(combined_ab.sites.get(id).unwrap(), &crate::threads::ClueSite::Wild(*pos));
    }
}

#[test]
fn breadcrumb_trail_crosses_four_regions() {
    let seed = frontier_epoch().world_seed;
    let schedule = build_spine_schedule(seed);
    let ch1_last = schedule.iter().find(|p| p.index + 1 == CHAPTER_1_CLUES).expect("ch1 last");
    let ch2_first = first_clue_of_chapter(&schedule, 2).expect("ch2 first");
    let text = breadcrumb_text(ch1_last.abs_x, ch1_last.abs_y, ch2_first.abs_x, ch2_first.abs_y);
    assert!(text.contains("region ("));
    let ch2_last = schedule
        .iter()
        .find(|p| p.chapter == 2 && p.index + 1 == CHAPTER_1_CLUES + 8)
        .expect("ch2 last");
    let ch3_first = first_clue_of_chapter(&schedule, 3).expect("ch3 first");
    let text2 = breadcrumb_text(ch2_last.abs_x, ch2_last.abs_y, ch3_first.abs_x, ch3_first.abs_y);
    assert!(text2.contains("region ("));

    let mut regions = std::collections::HashSet::new();
    for plan in &schedule {
        let Ok((ox, oy, _, _)) = origins_for_absolute(plan.abs_x, plan.abs_y) else {
            continue;
        };
        regions.insert((ox, oy));
    }
    assert!(regions.len() >= 4, "spine spans at least four birth cells");
}

#[test]
fn full_region_generation_is_deterministic() {
    let seed = frontier_epoch().world_seed;
    let templates = crate::threads::template_load::load_default_templates();
    let id = frontier_identity(WorldId::Undernet, 0, 0);
    let a = crate::threads::generate_for_region_with_params(
        seed,
        id,
        &templates,
        CoverageParams::default(),
    )
    .unwrap();
    let b = crate::threads::generate_for_region_with_params(
        seed,
        id,
        &templates,
        CoverageParams::default(),
    )
    .unwrap();
    assert_eq!(a.clues.len(), b.clues.len());
    for (key, clue) in &a.clues {
        assert_eq!(b.clues.get(key).map(|c| &c.surface_text), Some(&clue.surface_text));
    }
}
