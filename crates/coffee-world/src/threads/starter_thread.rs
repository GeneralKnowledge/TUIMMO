//! Deterministic short thread near the Roof Array / Hub gate exit (first walk).

use std::collections::HashSet;

use crate::geography::{
    gate_position, generate_region, thread_placement_hash, RegionIdentity, WildernessPos,
    GATE_LOCAL_X, GATE_LOCAL_Y, REGION_SIZE,
};

use super::distance_tier::DistanceTier;
use super::procedural::ProceduralRegion;
use super::template_load::TemplateCatalog;
use super::{ClueDef, ClueKind, ClueSite, SiteRule, ThreadDef, WildSiteFilter};

const STARTER_MAX_CHEB_FROM_GATE: i64 = 20;
const STARTER_CLUE_MIN: u32 = 2;
const STARTER_CLUE_MAX: u32 = 4;

pub fn starter_thread_instance_id(world_seed: u64) -> String {
    let h = thread_placement_hash(world_seed, "starter-walk", "instance", 0, 0);
    format!("starter-walk:{h:016x}")
}

fn cheb_from_gate(pos: WildernessPos) -> i64 {
    let (ax, ay) = pos.absolute();
    let gx = i64::from(GATE_LOCAL_X);
    let gy = i64::from(GATE_LOCAL_Y);
    (ax - gx).abs().max((ay - gy).abs())
}

fn pick_starter_tiles(
    world_seed: u64,
    identity: RegionIdentity,
    region: &crate::geography::GeneratedRegion,
    count: u32,
) -> Vec<WildernessPos> {
    let gate = gate_position();
    if identity.origin_x != gate.identity.origin_x || identity.origin_y != gate.identity.origin_y {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    for ly in 0..REGION_SIZE {
        for lx in 0..REGION_SIZE {
            let lx = u8::try_from(lx).expect("REGION_SIZE fits in u8");
            let ly = u8::try_from(ly).expect("REGION_SIZE fits in u8");
            let Some(tile) = region.tile(lx, ly) else {
                continue;
            };
            if !tile.terrain.traversable() {
                continue;
            }
            let pos = WildernessPos { identity, local_x: lx, local_y: ly };
            if pos == gate {
                continue;
            }
            if cheb_from_gate(pos) > STARTER_MAX_CHEB_FROM_GATE {
                continue;
            }
            candidates.push(pos);
        }
    }
    candidates.sort_by_key(|p| (p.local_x, p.local_y));
    let mut picked = Vec::new();
    for i in 0..count {
        if candidates.is_empty() {
            break;
        }
        let h = thread_placement_hash(world_seed, "starter-walk", "tile", i as i32, 0);
        let idx = (h as usize) % candidates.len();
        let pos = candidates.remove(idx);
        picked.push(pos);
    }
    picked
}

pub fn materialize_starter_thread_for_region(
    world_seed: u64,
    identity: RegionIdentity,
    templates: &TemplateCatalog,
    out: &mut ProceduralRegion,
    used: &mut HashSet<WildernessPos>,
) -> Result<(), crate::geography::GenerateError> {
    let region = generate_region(identity)?;
    let count = STARTER_CLUE_MIN
        + u32::try_from(
            thread_placement_hash(
                world_seed,
                "starter-walk",
                "count",
                identity.origin_x,
                identity.origin_y,
            ) % u64::from(STARTER_CLUE_MAX - STARTER_CLUE_MIN + 1),
        )
        .unwrap_or(0);
    let tiles = pick_starter_tiles(world_seed, identity, &region, count);
    if tiles.is_empty() {
        return Ok(());
    }

    let instance = starter_thread_instance_id(world_seed);
    let template = templates.templates.iter().find(|t| t.id == "residue");
    let mark = template
        .and_then(|t| t.pools.entries.get("mark"))
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_else(|| "tally".into());
    let surface = template
        .and_then(|t| t.pools.entries.get("surface"))
        .and_then(|v| {
            let h = thread_placement_hash(world_seed, "starter-walk", "surface", 0, 0) as usize;
            v.get(h % v.len()).cloned()
        })
        .unwrap_or_else(|| "chalk".into());
    let tag = "starter-walk".to_string();
    let mut clue_ids = Vec::new();
    for (i, pos) in tiles.iter().enumerate() {
        if used.contains(pos) {
            continue;
        }
        used.insert(*pos);
        let clue_id = format!("{instance}:{i}");
        clue_ids.push(clue_id.clone());
        let (ax, ay) = pos.absolute();
        let tier = DistanceTier::from_absolute(ax, ay);
        let text =
            starter_surface_text(world_seed, u32::try_from(i).unwrap_or(u32::MAX), &mark, &surface);
        let clue = ClueDef {
            id: clue_id.clone(),
            kind: ClueKind::Graffiti,
            surface_text: text,
            tags: vec![tag.clone(), "starter".into()],
            journal_title: format!("{mark} near the gate"),
            site: SiteRule::Wild(WildSiteFilter { band: tier.density_band(), district: None }),
            reveals: vec![],
        };
        out.clues.insert(clue_id.clone(), clue);
        out.sites.insert(clue_id, ClueSite::Wild(*pos));
    }
    if clue_ids.len() >= STARTER_CLUE_MIN as usize {
        out.threads.push(ThreadDef {
            id: instance,
            tags: vec![tag],
            clues: clue_ids.iter().filter_map(|id| out.clues.get(id).cloned()).collect(),
        });
    }
    Ok(())
}

fn starter_surface_text(world_seed: u64, index: u32, mark: &str, surface: &str) -> String {
    let variants = [
        format!("A {surface} {mark} repeats three times, then stops."),
        format!("Someone counted in {surface} beside the gate: {mark}, {mark}, {mark}."),
        format!("Fresh {surface} {mark} where the fence ends — deliberate, not weather."),
        format!("The same {mark} in {surface}, twice crossed out, once left clean."),
    ];
    let h = thread_placement_hash(world_seed, "starter-walk", "text", index as i32, 0) as usize;
    variants[h % variants.len()].clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::frontier_epoch;
    use crate::threads::template_load::load_default_templates;
    #[test]
    fn starter_thread_within_gate_radius_for_many_seeds() {
        let templates = load_default_templates();
        let epoch = frontier_epoch();
        let identity = RegionIdentity {
            world: crate::geography::WorldId::Undernet,
            epoch_id: epoch.id,
            origin_x: 0,
            origin_y: 0,
            generation_version: epoch.generation_version,
        };
        for seed in [1_u64, 42, 1000, 4242, 999_999] {
            let mut out = ProceduralRegion::default();
            let mut used = HashSet::new();
            materialize_starter_thread_for_region(seed, identity, &templates, &mut out, &mut used)
                .unwrap();
            assert!(out.clues.len() >= STARTER_CLUE_MIN as usize, "seed {seed}");
            for site in out.sites.values() {
                if let ClueSite::Wild(pos) = site {
                    assert!(
                        cheb_from_gate(*pos) <= STARTER_MAX_CHEB_FROM_GATE,
                        "seed {seed} tile too far"
                    );
                }
            }
        }
    }

    #[test]
    fn starter_thread_deterministic_and_seed_varies_text() {
        let templates = load_default_templates();
        let epoch = frontier_epoch();
        let identity = RegionIdentity {
            world: crate::geography::WorldId::Undernet,
            epoch_id: epoch.id,
            origin_x: 0,
            origin_y: 0,
            generation_version: epoch.generation_version,
        };
        let mut a = ProceduralRegion::default();
        let mut b = ProceduralRegion::default();
        materialize_starter_thread_for_region(
            epoch.world_seed,
            identity,
            &templates,
            &mut a,
            &mut HashSet::new(),
        )
        .unwrap();
        materialize_starter_thread_for_region(
            epoch.world_seed,
            identity,
            &templates,
            &mut b,
            &mut HashSet::new(),
        )
        .unwrap();
        let mut keys_a: Vec<_> = a.clues.keys().collect();
        let mut keys_b: Vec<_> = b.clues.keys().collect();
        keys_a.sort();
        keys_b.sort();
        assert_eq!(keys_a, keys_b);

        let mut other = ProceduralRegion::default();
        materialize_starter_thread_for_region(
            epoch.world_seed.wrapping_add(1),
            identity,
            &templates,
            &mut other,
            &mut HashSet::new(),
        )
        .unwrap();
        assert_ne!(
            a.clues.values().next().map(|c| &c.surface_text),
            other.clues.values().next().map(|c| &c.surface_text)
        );
    }
}
