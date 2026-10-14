//! Instantiate procedural threads per region (deterministic, not persisted).

use std::collections::{HashMap, HashSet};

use crate::geography::{generate_region, thread_placement_hash, RegionIdentity, WildernessPos};

use super::coverage::coverage_anchors_for_region_with_params;
use super::coverage_params::CoverageParams;
use super::distance_tier::DistanceTier;
use super::long_procession::materialize_long_procession_for_region;
use super::template_load::{
    fill_slots, template_reveal_defs, TemplateCatalog, TemplateDef, TemplateTier,
};
use super::{ClueDef, ClueKind, ClueSite, SiteRule, ThreadDef, WildSiteFilter};

#[derive(Clone, Debug, Default)]
pub struct ProceduralRegion {
    pub threads: Vec<ThreadDef>,
    pub clues: HashMap<String, ClueDef>,
    pub sites: HashMap<String, ClueSite>,
}

pub fn generate_for_region(
    world_seed: u64,
    identity: RegionIdentity,
    templates: &TemplateCatalog,
) -> Result<ProceduralRegion, crate::geography::GenerateError> {
    generate_for_region_with_params(world_seed, identity, templates, CoverageParams::default())
}

pub fn generate_for_region_with_params(
    world_seed: u64,
    identity: RegionIdentity,
    templates: &TemplateCatalog,
    params: CoverageParams,
) -> Result<ProceduralRegion, crate::geography::GenerateError> {
    let _region = generate_region(identity)?;
    let anchors = coverage_anchors_for_region_with_params(world_seed, identity, params)?;
    let tier = DistanceTier::from_absolute(
        i64::from(identity.origin_x) * 16 + 8,
        i64::from(identity.origin_y) * 16 + 8,
    );
    let mut out = ProceduralRegion::default();
    let mut used: HashSet<WildernessPos> = HashSet::new();

    for template in &templates.templates {
        match template.tier {
            TemplateTier::Local => {
                if template.id == "residue" {
                    instantiate_residue(
                        world_seed, identity, template, &anchors, &mut out, &mut used,
                    )?;
                }
            }
            TemplateTier::Regional => {
                if template.id == "absent-keeper"
                    && region_spawn(world_seed, identity, &template.id, 3)
                {
                    instantiate_absent_keeper(
                        world_seed, identity, template, &anchors, &mut out, &mut used,
                    )?;
                }
            }
            TemplateTier::Long => {
                if template.id == "long-procession" {
                    materialize_long_procession_for_region(
                        world_seed, identity, &mut out, &mut used, params,
                    )?;
                }
            }
        }
    }

    for (idx, anchor) in anchors.iter().enumerate() {
        if used.contains(anchor) {
            continue;
        }
        let id = format!(
            "cover:{}:{}:{}:{}",
            identity.origin_x, identity.origin_y, anchor.local_x, anchor.local_y
        );
        let text = coverage_blurb(world_seed, identity, tier, idx);
        let clue = ClueDef {
            id: id.clone(),
            kind: ClueKind::Graffiti,
            surface_text: text,
            tags: vec!["coverage".into()],
            journal_title: "Street detail".into(),
            site: SiteRule::Wild(WildSiteFilter { band: tier.density_band(), district: None }),
            reveals: vec![],
        };
        out.clues.insert(id.clone(), clue);
        out.sites.insert(id, ClueSite::Wild(*anchor));
        used.insert(*anchor);
    }

    Ok(out)
}

fn region_spawn(
    world_seed: u64,
    identity: RegionIdentity,
    template_id: &str,
    modulus: u64,
) -> bool {
    let h = thread_placement_hash(
        world_seed,
        template_id,
        "spawn",
        identity.origin_x,
        identity.origin_y,
    );
    h.is_multiple_of(modulus)
}

fn absent_keeper_anchor_index(slot: Option<&str>, fallback: usize) -> usize {
    match slot {
        Some(s) if s.starts_with("branch_a") => 3 + (fallback % 2),
        Some(s) if s.starts_with("branch_b") => 5,
        _ => fallback.min(2),
    }
}

fn pick_anchor<'a>(
    anchors: &'a [WildernessPos],
    used: &HashSet<WildernessPos>,
    index: usize,
) -> Option<&'a WildernessPos> {
    let free: Vec<&WildernessPos> = anchors.iter().filter(|p| !used.contains(p)).collect();
    if free.is_empty() {
        return None;
    }
    Some(free[index % free.len()])
}

fn instantiate_residue(
    world_seed: u64,
    identity: RegionIdentity,
    template: &TemplateDef,
    anchors: &[WildernessPos],
    out: &mut ProceduralRegion,
    used: &mut HashSet<WildernessPos>,
) -> Result<(), crate::geography::GenerateError> {
    let instance = format!("{}:{}:{}:0", template.id, identity.origin_x, identity.origin_y);
    let mut slots = HashMap::new();
    let mark = pick_pool(world_seed, identity, template, "mark", 0);
    let surface = pick_pool(world_seed, identity, template, "surface", 1);
    let count: u32 = 3
        + (thread_placement_hash(
            world_seed,
            "residue",
            "count",
            identity.origin_x,
            identity.origin_y,
        ) % 5) as u32;
    slots.insert("mark".into(), mark.clone());
    slots.insert("surface".into(), surface);
    slots.insert("count".into(), count.to_string());
    slots.insert("count_minus_two".into(), count.saturating_sub(2).max(1).to_string());
    let tag = fill_slots(&template.tags[0], &slots);
    let mut clue_ids = Vec::new();
    let mut built = Vec::new();
    let mut reserved_anchors = Vec::new();
    for (i, slot) in template.clues.iter().enumerate() {
        let clue_id = format!("{instance}:{i}");
        clue_ids.push(clue_id.clone());
        let Some(anchor) = pick_anchor(anchors, used, i).copied() else {
            break;
        };
        used.insert(anchor);
        reserved_anchors.push(anchor);
        built.push(ClueDef {
            id: clue_id.clone(),
            kind: slot.kind,
            surface_text: fill_slots(&slot.text, &slots),
            tags: vec![tag.clone()],
            journal_title: format!("{mark} mark"),
            site: SiteRule::Wild(WildSiteFilter {
                band: DistanceTier::from_absolute(
                    i64::from(identity.origin_x) * 16 + i64::from(anchor.local_x),
                    i64::from(identity.origin_y) * 16 + i64::from(anchor.local_y),
                )
                .density_band(),
                district: None,
            }),
            reveals: vec![],
        });
        out.sites.insert(clue_id, ClueSite::Wild(anchor));
    }
    if built.len() < template.clues.len() {
        for id in &clue_ids {
            out.clues.remove(id);
            out.sites.remove(id);
        }
        for anchor in reserved_anchors {
            used.remove(&anchor);
        }
        return Ok(());
    }
    if let Some(last) = built.last_mut() {
        last.reveals = template_reveal_defs(template, &slots, &clue_ids);
    }
    for clue in built {
        out.clues.insert(clue.id.clone(), clue);
    }
    out.threads.push(ThreadDef {
        id: instance,
        tags: vec![tag],
        clues: clue_ids.iter().filter_map(|id| out.clues.get(id).cloned()).collect(),
    });
    Ok(())
}

fn instantiate_absent_keeper(
    world_seed: u64,
    identity: RegionIdentity,
    template: &TemplateDef,
    anchors: &[WildernessPos],
    out: &mut ProceduralRegion,
    used: &mut HashSet<WildernessPos>,
) -> Result<(), crate::geography::GenerateError> {
    let instance = format!("{}:{}:{}:0", template.id, identity.origin_x, identity.origin_y);
    let mut slots = HashMap::new();
    let object = pick_pool(world_seed, identity, template, "object", 0);
    slots.insert("object".into(), object);
    let tag = fill_slots(&template.tags[0], &slots);
    let mut clue_ids = Vec::new();
    let mut reserved_anchors = Vec::new();
    for (i, slot) in template.clues.iter().enumerate() {
        let clue_id = format!("{instance}:{i}");
        clue_ids.push(clue_id.clone());
        let anchor_index = absent_keeper_anchor_index(slot.slot.as_deref(), i);
        let Some(anchor) = pick_anchor(anchors, used, anchor_index).copied() else {
            break;
        };
        used.insert(anchor);
        reserved_anchors.push(anchor);
        let reveals = if i + 1 == template.clues.len() {
            template_reveal_defs(template, &slots, &clue_ids)
        } else {
            vec![]
        };
        let clue = ClueDef {
            id: clue_id.clone(),
            kind: slot.kind,
            surface_text: fill_slots(&slot.text, &slots),
            tags: vec![tag.clone()],
            journal_title: "Tended place".into(),
            site: SiteRule::Wild(WildSiteFilter {
                band: DistanceTier::from_absolute(
                    i64::from(identity.origin_x) * 16 + i64::from(anchor.local_x),
                    i64::from(identity.origin_y) * 16 + i64::from(anchor.local_y),
                )
                .density_band(),
                district: None,
            }),
            reveals,
        };
        out.clues.insert(clue_id.clone(), clue);
        out.sites.insert(clue_id, ClueSite::Wild(anchor));
    }
    if clue_ids.len() < template.clues.len() {
        for id in &clue_ids {
            out.clues.remove(id);
            out.sites.remove(id);
        }
        for anchor in reserved_anchors {
            used.remove(&anchor);
        }
        return Ok(());
    }
    out.threads.push(ThreadDef {
        id: instance,
        tags: vec![tag],
        clues: clue_ids.iter().filter_map(|id| out.clues.get(id).cloned()).collect(),
    });
    Ok(())
}

fn pick_pool(
    world_seed: u64,
    identity: RegionIdentity,
    template: &TemplateDef,
    pool: &str,
    salt: u64,
) -> String {
    let values = template.pools.entries.get(pool).cloned().unwrap_or_default();
    if values.is_empty() {
        return "mark".into();
    }
    let h = thread_placement_hash(
        world_seed,
        &template.id,
        &format!("pool:{pool}:{salt}"),
        identity.origin_x,
        identity.origin_y,
    );
    values[h as usize % values.len()].clone()
}

fn coverage_blurb(
    world_seed: u64,
    identity: RegionIdentity,
    tier: DistanceTier,
    idx: usize,
) -> String {
    let early = [
        "A scuff in the dust where something heavy rested recently.",
        "A nail hole with no nail, still holding a ring of cleaner plaster.",
        "Chalk dust in a corner, as if someone counted under their breath.",
        "A strip of tape on the wall, faded to the colour of old paper.",
        "A coin-sized rust stain where a bracket was removed.",
        "Pencil lines on the sill, too straight to be accidental.",
        "A smell of cold ash with no ash visible.",
        "A hand-width polish on the rail where no one walks anymore.",
    ];
    let late = [
        "The same scuff repeats on the next landing, shallower.",
        "Mortar newer than the bricks around it, patched in haste.",
        "A groove at waist height, worn by a route that no longer exists.",
        "Salt crystals in a crack, as if someone tried to draw a line.",
        "A nail driven into stone, useless but deliberate.",
        "Dust swept into a corner, then left.",
        "A number written and crossed out, written again.",
        "A thread caught on a splinter, still holding colour.",
    ];
    let pool = match tier {
        DistanceTier::Near | DistanceTier::Mid => &early[..],
        DistanceTier::Far | DistanceTier::Deep => &late[..],
    };
    let h = thread_placement_hash(
        world_seed,
        "coverage-blurb",
        &format!("{idx}"),
        identity.origin_x,
        identity.origin_y,
    );
    pool[h as usize % pool.len()].to_string()
}
