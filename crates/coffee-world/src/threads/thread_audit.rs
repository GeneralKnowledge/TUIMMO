//! Static coverage audit for authored and procedural clue threads (deterministic).

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::geography::{frontier_epoch, gate_identity, generate_region, RegionIdentity, WorldId};
use crate::static_location;

use super::knowledge::thread_final_clue_id;
use super::load::load_default_threads;
use super::long_procession::{
    build_spine_schedule, snap_walkable, thread_instance_id, TOTAL_CLUES,
};
use super::placement::build_placement_map;
use super::procedural::{generate_for_region_with_params, ProceduralRegion};
use super::starter_thread::{materialize_starter_thread_for_region, STARTER_MAX_CHEB_FROM_GATE};
use super::template_load::load_default_templates;
use super::CoverageParams;
use super::{chebyshev_from_gate, ClueDef, ClueSite, KnowledgeRequirement, ThreadDef};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CluePlacementAudit {
    pub clue_id: String,
    pub placed: bool,
    pub reachable: bool,
    pub inspectable: bool,
    pub knowledge_gated_reveals: u32,
    pub issues: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadConclusionKind {
    /// Last clue in thread definition (sim uses this as “final” for metrics).
    FinalClueConfigured,
    /// No final clue id (open procedural fragment).
    OpenEnded,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuthoredThreadAuditRow {
    pub thread_id: String,
    pub configured_clues: u32,
    pub placed_clues: u32,
    pub distinct_reachable_locations: u32,
    pub clues_with_inspect_output: u32,
    pub knowledge_gated_reveals: u32,
    pub next_step_discoverable: bool,
    pub conclusion: ThreadConclusionKind,
    pub invalid_or_unreachable_clues: u32,
    pub issues: Vec<String>,
    pub clues: Vec<CluePlacementAudit>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProceduralTemplateAudit {
    pub template_id: String,
    pub notes: String,
    pub configured_or_scheduled_clues: u32,
    pub placed_on_walkable: u32,
    pub distinct_reachable_locations: u32,
    pub invalid_or_unreachable: u32,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LongProcessionRouteAudit {
    pub clues_scheduled: u32,
    pub clues_reachable: u32,
    pub distinct_regions: u32,
    pub max_cheb_step_between_consecutive: i64,
    pub mean_cheb_step_between_consecutive: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadCoverageAuditReport {
    pub world_seed: u64,
    pub authored_threads: Vec<AuthoredThreadAuditRow>,
    pub procedural_templates: Vec<ProceduralTemplateAudit>,
    pub long_procession_route: LongProcessionRouteAudit,
}

fn site_key(site: &ClueSite) -> String {
    match site {
        ClueSite::Wild(pos) => pos.key(),
        ClueSite::Static(id) => format!("static:{id}"),
    }
}

fn wild_reachable(pos: crate::geography::WildernessPos) -> bool {
    generate_region(pos.identity)
        .ok()
        .and_then(|region| region.tile(pos.local_x, pos.local_y))
        .is_some_and(|tile| tile.terrain.traversable())
}

fn static_reachable(location_id: &str) -> bool {
    static_location(location_id).is_some()
}

fn site_reachable(site: &ClueSite) -> bool {
    match site {
        ClueSite::Wild(pos) => wild_reachable(*pos),
        ClueSite::Static(id) => static_reachable(id),
    }
}

fn count_gated_reveals(clue: &ClueDef) -> u32 {
    clue.reveals
        .iter()
        .filter(|reveal| !matches!(&reveal.requires, KnowledgeRequirement::All(items) if items.is_empty()))
        .count() as u32
}

fn next_step_discoverable(
    thread: &ThreadDef,
    clue_rows: &[CluePlacementAudit],
    placement: &BTreeMap<String, ClueSite>,
) -> bool {
    let reachable_sites: BTreeSet<String> = clue_rows
        .iter()
        .filter(|row| row.reachable)
        .filter_map(|row| placement.get(&row.clue_id))
        .map(site_key)
        .collect();
    if reachable_sites.len() >= 2 {
        return true;
    }
    for clue in &thread.clues {
        for reveal in &clue.reveals {
            if reveal.text.contains("region (") {
                return true;
            }
            match &reveal.requires {
                KnowledgeRequirement::SeenTag { distinct_sites, .. } if *distinct_sites >= 2 => {
                    return true;
                }
                KnowledgeRequirement::AnyOfClues { n, .. } if *n >= 2 => return true,
                KnowledgeRequirement::All(items) if items.len() >= 2 => return true,
                _ => {}
            }
        }
    }
    !clue_rows.is_empty() && clue_rows.iter().any(|r| r.reachable)
}

fn audit_authored_thread(
    thread: &ThreadDef,
    world_seed: u64,
    placement: &BTreeMap<String, ClueSite>,
) -> AuthoredThreadAuditRow {
    let mut clue_rows = Vec::new();
    let mut issues = Vec::new();
    let mut site_keys = BTreeSet::new();
    let mut invalid = 0_u32;

    for clue in &thread.clues {
        let mut row_issues = Vec::new();
        let placed = placement.get(&clue.id);
        let placed_ok = placed.is_some();
        let reachable = placed.is_some_and(site_reachable);
        if !placed_ok {
            row_issues.push("no placement".into());
            invalid += 1;
        } else if !reachable {
            row_issues.push("unreachable site".into());
            invalid += 1;
        }
        if clue.surface_text.trim().is_empty() {
            row_issues.push("empty surface_text".into());
        }
        if let Some(site) = placed {
            let key = site_key(site);
            if !site_keys.insert(key.clone()) {
                row_issues.push(format!("duplicate site {key}"));
            }
        }
        let inspectable = !clue.surface_text.trim().is_empty() && reachable;
        let gated = count_gated_reveals(clue);
        if !row_issues.is_empty() {
            issues.extend(row_issues.iter().cloned());
        }
        clue_rows.push(CluePlacementAudit {
            clue_id: clue.id.clone(),
            placed: placed_ok,
            reachable,
            inspectable,
            knowledge_gated_reveals: gated,
            issues: row_issues,
        });
    }

    let placed_clues = clue_rows.iter().filter(|r| r.placed).count() as u32;
    let distinct_reachable = clue_rows
        .iter()
        .filter(|r| r.reachable)
        .filter_map(|r| placement.get(&r.clue_id))
        .map(site_key)
        .collect::<BTreeSet<_>>()
        .len() as u32;
    let inspect_output = clue_rows.iter().filter(|r| r.inspectable).count() as u32;
    let gated_total: u32 = clue_rows.iter().map(|r| r.knowledge_gated_reveals).sum();

    let conclusion = if thread_final_clue_id(thread).is_some() {
        ThreadConclusionKind::FinalClueConfigured
    } else {
        ThreadConclusionKind::OpenEnded
    };

    let next = next_step_discoverable(thread, &clue_rows, placement);

    let _ = world_seed;
    AuthoredThreadAuditRow {
        thread_id: thread.id.clone(),
        configured_clues: u32::try_from(thread.clues.len()).unwrap_or(u32::MAX),
        placed_clues,
        distinct_reachable_locations: distinct_reachable,
        clues_with_inspect_output: inspect_output,
        knowledge_gated_reveals: gated_total,
        next_step_discoverable: next,
        conclusion,
        invalid_or_unreachable_clues: invalid,
        issues,
        clues: clue_rows,
    }
}

fn audit_residue_like(template_id: &str, world_seed: u64) -> ProceduralTemplateAudit {
    let templates = load_default_templates();
    let identity = gate_identity();
    let mut issues = Vec::new();
    let Ok(region) = generate_for_region_with_params(
        world_seed,
        identity,
        &templates,
        CoverageParams::default(),
    ) else {
        return ProceduralTemplateAudit {
            template_id: template_id.into(),
            notes: "origin region generation failed".into(),
            configured_or_scheduled_clues: 0,
            placed_on_walkable: 0,
            distinct_reachable_locations: 0,
            invalid_or_unreachable: 0,
            issues: vec!["generate_for_region failed".into()],
        };
    };
    let prefix = format!("{template_id}:");
    let thread_clues: Vec<_> =
        region.clues.keys().filter(|id| id.starts_with(&prefix)).cloned().collect();
    let mut sites = BTreeSet::new();
    let mut reachable = 0_u32;
    let mut invalid = 0_u32;
    for id in &thread_clues {
        let Some(site) = region.sites.get(id) else {
            invalid += 1;
            continue;
        };
        if site_reachable(site) {
            reachable += 1;
            sites.insert(site_key(site));
        } else {
            invalid += 1;
        }
    }
    if thread_clues.is_empty() {
        issues.push("no instances in origin sample region".into());
    }
    ProceduralTemplateAudit {
        template_id: template_id.into(),
        notes: "sampled at gate origin region only".into(),
        configured_or_scheduled_clues: u32::try_from(thread_clues.len()).unwrap_or(u32::MAX),
        placed_on_walkable: reachable,
        distinct_reachable_locations: u32::try_from(sites.len()).unwrap_or(u32::MAX),
        invalid_or_unreachable: invalid,
        issues,
    }
}

fn audit_starter_walk(world_seed: u64) -> ProceduralTemplateAudit {
    let templates = load_default_templates();
    let identity = gate_identity();
    let mut out = ProceduralRegion::default();
    let mut used = HashSet::new();
    let mut issues = Vec::new();
    if materialize_starter_thread_for_region(world_seed, identity, &templates, &mut out, &mut used)
        .is_err()
    {
        return ProceduralTemplateAudit {
            template_id: "starter-walk".into(),
            notes: "materialize failed".into(),
            configured_or_scheduled_clues: 0,
            placed_on_walkable: 0,
            distinct_reachable_locations: 0,
            invalid_or_unreachable: 0,
            issues: vec!["materialize_starter_thread failed".into()],
        };
    }
    let mut sites = BTreeSet::new();
    let mut reachable = 0_u32;
    let mut invalid = 0_u32;
    for (id, site) in &out.sites {
        if !id.contains("starter-walk") {
            continue;
        }
        if let ClueSite::Wild(pos) = site {
            let dist = chebyshev_from_gate(pos.absolute().0, pos.absolute().1);
            if dist > STARTER_MAX_CHEB_FROM_GATE {
                issues.push(format!("clue {id} beyond gate radius ({dist})"));
            }
        }
        if site_reachable(site) {
            reachable += 1;
            sites.insert(site_key(site));
        } else {
            invalid += 1;
        }
    }
    ProceduralTemplateAudit {
        template_id: "starter-walk".into(),
        notes: format!("within Chebyshev {STARTER_MAX_CHEB_FROM_GATE} of gate"),
        configured_or_scheduled_clues: u32::try_from(out.clues.len()).unwrap_or(u32::MAX),
        placed_on_walkable: reachable,
        distinct_reachable_locations: u32::try_from(sites.len()).unwrap_or(u32::MAX),
        invalid_or_unreachable: invalid,
        issues,
    }
}

fn audit_long_procession_route(world_seed: u64) -> LongProcessionRouteAudit {
    let schedule = build_spine_schedule(world_seed);
    let epoch = frontier_epoch();
    let base = RegionIdentity {
        world: WorldId::Undernet,
        epoch_id: epoch.id,
        origin_x: 0,
        origin_y: 0,
        generation_version: epoch.generation_version,
    };
    let mut reachable = 0_u32;
    let mut regions = BTreeSet::new();
    let mut steps = Vec::new();
    let mut last: Option<(i64, i64)> = None;
    for plan in &schedule {
        if snap_walkable(base, plan.abs_x, plan.abs_y).is_some() {
            reachable += 1;
        }
        if let Ok((ox, oy, _, _)) = crate::geography::origins_for_absolute(plan.abs_x, plan.abs_y) {
            regions.insert((ox, oy));
        }
        if let Some(prev) = last {
            let dx = (plan.abs_x - prev.0).abs();
            let dy = (plan.abs_y - prev.1).abs();
            steps.push(dx.max(dy));
        }
        last = Some((plan.abs_x, plan.abs_y));
    }
    let max_step = steps.iter().copied().max().unwrap_or(0);
    let mean_step = if steps.is_empty() {
        0.0
    } else {
        f64::from(steps.iter().sum::<i64>() as i32) / f64::from(steps.len() as u32)
    };
    LongProcessionRouteAudit {
        clues_scheduled: TOTAL_CLUES,
        clues_reachable: reachable,
        distinct_regions: u32::try_from(regions.len()).unwrap_or(u32::MAX),
        max_cheb_step_between_consecutive: max_step,
        mean_cheb_step_between_consecutive: mean_step,
    }
}

/// Full static audit for shipped threads at `world_seed` (deterministic, sorted by thread id).
pub fn measure_thread_coverage_audit(world_seed: u64) -> ThreadCoverageAuditReport {
    let catalog = load_default_threads();
    let placement_map = build_placement_map(&catalog, world_seed);
    let placement: BTreeMap<String, ClueSite> = placement_map.into_iter().collect();

    let mut authored: Vec<AuthoredThreadAuditRow> = catalog
        .threads
        .iter()
        .map(|thread| audit_authored_thread(thread, world_seed, &placement))
        .collect();
    authored.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));

    let route = audit_long_procession_route(world_seed);
    let procedural_templates = vec![
        audit_starter_walk(world_seed),
        audit_residue_like("residue", world_seed),
        audit_residue_like("absent-keeper", world_seed),
        ProceduralTemplateAudit {
            template_id: "long-procession".into(),
            notes: format!("global spine instance {}", thread_instance_id(world_seed)),
            configured_or_scheduled_clues: TOTAL_CLUES,
            placed_on_walkable: route.clues_reachable,
            distinct_reachable_locations: route.distinct_regions,
            invalid_or_unreachable: TOTAL_CLUES.saturating_sub(route.clues_reachable),
            issues: Vec::new(),
        },
    ];

    ThreadCoverageAuditReport {
        world_seed,
        authored_threads: authored,
        procedural_templates,
        long_procession_route: route,
    }
}

pub fn format_thread_coverage_audit(report: &ThreadCoverageAuditReport) -> String {
    let mut lines = vec![
        format!("Thread coverage audit (world_seed={})", report.world_seed),
        String::new(),
        "Authored threads:".into(),
    ];
    for row in &report.authored_threads {
        lines.push(format!(
            "  {}: configured={} placed={} reachable_sites={} inspectable={} gated_reveals={} next_step_playable={} invalid={} conclusion={:?}",
            row.thread_id,
            row.configured_clues,
            row.placed_clues,
            row.distinct_reachable_locations,
            row.clues_with_inspect_output,
            row.knowledge_gated_reveals,
            row.next_step_discoverable,
            row.invalid_or_unreachable_clues,
            row.conclusion,
        ));
        if !row.issues.is_empty() {
            lines.push(format!("    issues: {}", row.issues.join("; ")));
        }
    }
    lines.push(String::new());
    lines.push("Procedural / onboarding:".into());
    for row in &report.procedural_templates {
        lines.push(format!(
            "  {}: scheduled/placed={} walkable={} distinct_sites={} invalid={} ({})",
            row.template_id,
            row.configured_or_scheduled_clues,
            row.placed_on_walkable,
            row.distinct_reachable_locations,
            row.invalid_or_unreachable,
            row.notes,
        ));
    }
    let route = &report.long_procession_route;
    lines.push(format!(
        "Long-procession route: reachable {}/{} clues across {} regions; consecutive cheb steps mean={:.1} max={}",
        route.clues_reachable,
        route.clues_scheduled,
        route.distinct_regions,
        route.mean_cheb_step_between_consecutive,
        route.max_cheb_step_between_consecutive,
    ));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threads::load::load_threads_from_str;

    const FIXTURE: &str = include_str!("../../data/threads.toml");

    #[test]
    fn audit_is_deterministic_for_seed() {
        let seed = frontier_epoch().world_seed;
        let a = measure_thread_coverage_audit(seed);
        let b = measure_thread_coverage_audit(seed);
        assert_eq!(a, b);
    }

    #[test]
    fn audit_differs_by_seed_for_wild_placement() {
        let catalog = load_threads_from_str(FIXTURE).unwrap();
        let a = build_placement_map(&catalog, 1);
        let b = build_placement_map(&catalog, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn shipped_authored_threads_all_place() {
        let report = measure_thread_coverage_audit(frontier_epoch().world_seed);
        for row in &report.authored_threads {
            assert_eq!(
                row.configured_clues, row.placed_clues,
                "thread {} missing placements",
                row.thread_id
            );
            assert_eq!(
                row.invalid_or_unreachable_clues, 0,
                "thread {} has invalid clues: {:?}",
                row.thread_id, row.issues
            );
        }
    }

    #[test]
    fn long_procession_first_clue_reachable_in_game() {
        let seed = frontier_epoch().world_seed;
        let schedule = build_spine_schedule(seed);
        let first = schedule.first().expect("spine");
        let pos =
            snap_walkable(gate_identity(), first.abs_x, first.abs_y).expect("first clue walkable");
        assert!(wild_reachable(pos));
    }
}
