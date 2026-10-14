//! Cross-region long-procession spine (deterministic absolute tiles).

use std::collections::HashSet;

use crate::geography::{
    generate_region, origins_for_absolute, thread_placement_hash, RegionIdentity, WildernessPos,
    GATE_LOCAL_X, GATE_LOCAL_Y, REGION_SIZE,
};

use super::coverage_params::CoverageParams;
use super::distance_tier::DistanceTier;
use super::procedural::ProceduralRegion;
use super::template_load::{load_default_templates, template_reveal_defs};
use super::{ClueDef, ClueKind, ClueSite, SiteRule, ThreadDef, WildSiteFilter};

pub const CHAPTER_1_CLUES: u32 = 5;
pub const CHAPTER_2_CLUES: u32 = 8;
pub const CHAPTER_3_CLUES: u32 = 8;
pub const TOTAL_CLUES: u32 = CHAPTER_1_CLUES + CHAPTER_2_CLUES + CHAPTER_3_CLUES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpineCluePlan {
    pub index: u32,
    pub chapter: u8,
    pub abs_x: i64,
    pub abs_y: i64,
    pub clue_id: String,
}

pub fn spine_id(world_seed: u64) -> String {
    let h = thread_placement_hash(world_seed, "long-procession", "spine", 0, 0);
    format!("{h:016x}")
}

pub fn thread_instance_id(world_seed: u64) -> String {
    format!("long-procession:spine:{}", spine_id(world_seed))
}

pub fn clue_id_for_index(world_seed: u64, index: u32) -> String {
    format!("{}:{index}", thread_instance_id(world_seed))
}

fn chapter_for_index(index: u32) -> u8 {
    if index < CHAPTER_1_CLUES {
        1
    } else if index < CHAPTER_1_CLUES + CHAPTER_2_CLUES {
        2
    } else {
        3
    }
}

fn chapter_allows_tier(chapter: u8, tier: DistanceTier) -> bool {
    match chapter {
        1 => matches!(tier, DistanceTier::Near | DistanceTier::Mid),
        2 => matches!(tier, DistanceTier::Mid | DistanceTier::Far),
        _ => matches!(tier, DistanceTier::Far | DistanceTier::Deep),
    }
}

fn pick_absolute_tile(world_seed: u64, index: u32, chapter: u8) -> (i64, i64) {
    for salt in 0..256_u32 {
        let h = thread_placement_hash(
            world_seed,
            "long-procession",
            &format!("tile:{index}:{salt}"),
            0,
            0,
        );
        let radius = match chapter {
            1 => 80_i64,
            2 => 400_i64,
            _ => 1200_i64,
        };
        let ax = i64::from(GATE_LOCAL_X) + ((h & 0xFFFF) as i64 % (radius * 2 + 1)) - radius;
        let ay =
            i64::from(GATE_LOCAL_Y) + (((h >> 16) & 0xFFFF) as i64 % (radius * 2 + 1)) - radius;
        let tier = DistanceTier::from_absolute(ax, ay);
        if chapter_allows_tier(chapter, tier) {
            return (ax, ay);
        }
    }
    let h = thread_placement_hash(
        world_seed,
        "long-procession",
        &format!("tile-fallback:{index}"),
        0,
        0,
    );
    (
        i64::from(GATE_LOCAL_X) + (h as i64 % 512) - 256,
        i64::from(GATE_LOCAL_Y) + ((h >> 32) as i64 % 512) - 256,
    )
}

pub(crate) fn snap_walkable(
    identity: RegionIdentity,
    abs_x: i64,
    abs_y: i64,
) -> Option<WildernessPos> {
    let Ok((ox, oy, lx, ly)) = origins_for_absolute(abs_x, abs_y) else {
        return None;
    };
    let pos_identity = RegionIdentity { origin_x: ox, origin_y: oy, ..identity };
    let region = generate_region(pos_identity).ok()?;
    if region.tile(lx, ly).is_some_and(|t| t.terrain.traversable()) {
        return Some(WildernessPos { identity: pos_identity, local_x: lx, local_y: ly });
    }
    for attempt in 0..REGION_SIZE * REGION_SIZE {
        let ax = (i32::from(lx) + attempt % REGION_SIZE) as u8;
        let ay = (i32::from(ly) + (attempt / REGION_SIZE) % REGION_SIZE) as u8;
        if region.tile(ax, ay).is_some_and(|t| t.terrain.traversable()) {
            return Some(WildernessPos { identity: pos_identity, local_x: ax, local_y: ay });
        }
    }
    None
}

pub fn build_spine_schedule(world_seed: u64) -> Vec<SpineCluePlan> {
    let mut out = Vec::new();
    for index in 0..TOTAL_CLUES {
        let chapter = chapter_for_index(index);
        let (abs_x, abs_y) = pick_absolute_tile(world_seed, index, chapter);
        out.push(SpineCluePlan {
            index,
            chapter,
            abs_x,
            abs_y,
            clue_id: clue_id_for_index(world_seed, index),
        });
    }
    out
}

pub fn first_clue_of_chapter(schedule: &[SpineCluePlan], chapter: u8) -> Option<&SpineCluePlan> {
    schedule.iter().find(|c| c.chapter == chapter)
}

pub fn breadcrumb_text(from_ax: i64, from_ay: i64, to_ax: i64, to_ay: i64) -> String {
    let dx = to_ax - from_ax;
    let dy = to_ay - from_ay;
    let Ok((target_ox, target_oy, _, _)) = origins_for_absolute(to_ax, to_ay) else {
        return String::new();
    };
    if dx.abs() >= dy.abs() {
        if dx >= 0 {
            format!(
                "The same mark continues far to the east, toward region ({target_ox}, {target_oy})."
            )
        } else {
            format!(
                "The same mark continues far to the west, toward region ({target_ox}, {target_oy})."
            )
        }
    } else if dy >= 0 {
        format!("The count continues far to the north, toward region ({target_ox}, {target_oy}).")
    } else {
        format!("The count continues far to the south, toward region ({target_ox}, {target_oy}).")
    }
}

#[allow(dead_code)]
pub fn region_pair_hash(world_seed: u64, a: RegionIdentity, b: RegionIdentity) -> u64 {
    let (x0, y0, x1, y1) = if (a.origin_x, a.origin_y) <= (b.origin_x, b.origin_y) {
        (a.origin_x, a.origin_y, b.origin_x, b.origin_y)
    } else {
        (b.origin_x, b.origin_y, a.origin_x, a.origin_y)
    };
    let clue = format!("pair:{x1}:{y1}");
    thread_placement_hash(world_seed, "long-procession", &clue, x0, y0)
}

pub fn materialize_long_procession_for_region(
    world_seed: u64,
    identity: RegionIdentity,
    out: &mut ProceduralRegion,
    used: &mut HashSet<WildernessPos>,
    _params: CoverageParams,
) -> Result<(), crate::geography::GenerateError> {
    let templates = load_default_templates();
    let Some(template) = templates.templates.iter().find(|t| t.id == "long-procession") else {
        return Ok(());
    };

    let schedule = build_spine_schedule(world_seed);
    let instance = thread_instance_id(world_seed);
    let tag = template.tags.first().cloned().unwrap_or_else(|| "procession".into());
    let mut clue_ids = Vec::new();
    let mut clue_defs = Vec::new();

    for plan in &schedule {
        let Ok((ox, oy, _, _)) = origins_for_absolute(plan.abs_x, plan.abs_y) else {
            continue;
        };
        if ox != identity.origin_x || oy != identity.origin_y {
            continue;
        }
        let Some(pos) = snap_walkable(identity, plan.abs_x, plan.abs_y) else {
            continue;
        };
        let tier = DistanceTier::from_absolute(plan.abs_x, plan.abs_y);
        if !chapter_allows_tier(plan.chapter, tier) {
            continue;
        }
        if used.contains(&pos) {
            continue;
        }
        used.insert(pos);

        let chapter_idx = (plan.chapter as usize).saturating_sub(1);
        let chapter = template.chapters.get(chapter_idx);
        let pool_name = chapter.map(|c| c.pool.as_str()).unwrap_or("procession_early");
        let pool = template.pools.entries.get(pool_name).cloned().unwrap_or_default();
        let text_idx = (thread_placement_hash(
            world_seed,
            "long-procession",
            &format!("text:{}", plan.index),
            identity.origin_x,
            identity.origin_y,
        ) as usize)
            % pool.len().max(1);
        let mut text = pool.get(text_idx).cloned().unwrap_or_else(|| "A mark.".into());

        let is_last_in_chapter = match plan.chapter {
            1 => plan.index + 1 == CHAPTER_1_CLUES,
            2 => plan.index + 1 == CHAPTER_1_CLUES + CHAPTER_2_CLUES,
            _ => plan.index + 1 == TOTAL_CLUES,
        };
        if chapter.is_some_and(|c| c.breadcrumb) && is_last_in_chapter {
            if let Some(next) = first_clue_of_chapter(&schedule, plan.chapter + 1) {
                text.push(' ');
                text.push_str(&breadcrumb_text(plan.abs_x, plan.abs_y, next.abs_x, next.abs_y));
            }
        }

        clue_ids.push(plan.clue_id.clone());
        clue_defs.push((
            plan.clue_id.clone(),
            ClueDef {
                id: plan.clue_id.clone(),
                kind: ClueKind::Graffiti,
                surface_text: text,
                tags: vec![tag.clone(), format!("procession-ch{}", plan.chapter)],
                journal_title: "Procession mark".into(),
                site: SiteRule::Wild(WildSiteFilter { band: tier.density_band(), district: None }),
                reveals: vec![],
            },
            pos,
        ));
    }

    if clue_defs.is_empty() {
        return Ok(());
    }

    for (id, clue, pos) in clue_defs.iter_mut() {
        let plan = schedule.iter().find(|p| p.clue_id == *id).expect("plan");
        let is_last_in_chapter = match plan.chapter {
            1 => plan.index + 1 == CHAPTER_1_CLUES,
            2 => plan.index + 1 == CHAPTER_1_CLUES + CHAPTER_2_CLUES,
            _ => plan.index + 1 == TOTAL_CLUES,
        };
        if is_last_in_chapter {
            let chapter_clues: Vec<String> = schedule
                .iter()
                .filter(|p| p.chapter == plan.chapter)
                .map(|p| p.clue_id.clone())
                .collect();
            clue.reveals = template_reveal_defs(template, &Default::default(), &chapter_clues);
        }
        out.clues.insert(id.clone(), clue.clone());
        out.sites.insert(id.clone(), ClueSite::Wild(*pos));
    }

    out.threads.push(ThreadDef {
        id: instance,
        tags: vec![tag],
        clues: clue_ids.iter().filter_map(|id| out.clues.get(id).cloned()).collect(),
    });

    Ok(())
}

type RegionPairClues =
    (Vec<(String, ClueDef, WildernessPos)>, Vec<(String, ClueDef, WildernessPos)>);

/// Clues placed when generating `a` then `b` vs `b` then `a` for regions containing shared tiles.
#[allow(dead_code)]
pub fn shared_clues_for_region_pair(
    world_seed: u64,
    region_a: RegionIdentity,
    region_b: RegionIdentity,
) -> RegionPairClues {
    let mut out_a = ProceduralRegion::default();
    let mut used_a = HashSet::new();
    let _ = materialize_long_procession_for_region(
        world_seed,
        region_a,
        &mut out_a,
        &mut used_a,
        CoverageParams::default(),
    );
    let mut out_b = ProceduralRegion::default();
    let mut used_b = HashSet::new();
    let _ = materialize_long_procession_for_region(
        world_seed,
        region_b,
        &mut out_b,
        &mut used_b,
        CoverageParams::default(),
    );

    fn collect(region: &ProceduralRegion) -> Vec<(String, ClueDef, WildernessPos)> {
        region
            .sites
            .iter()
            .filter_map(|(id, site)| {
                let ClueSite::Wild(pos) = site else {
                    return None;
                };
                let clue = region.clues.get(id)?.clone();
                Some((id.clone(), clue, *pos))
            })
            .collect()
    }
    (collect(&out_a), collect(&out_b))
}
