//! Offline metrics for thread clue discovery (simulation reporting only).

use std::collections::{BTreeMap, HashMap, HashSet};

use terminus_core::PlayerId;

use serde::{Deserialize, Serialize};

use super::clue_class::is_thread_clue;
use super::coverage_metrics::{chebyshev_from_gate, HubDistanceBucket};
use super::{thread_final_clue_id, ClueSite, ThreadState};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSimReport {
    pub thread_id: String,
    pub final_clue_id: String,
    pub operators_reaching_final_clue: u32,
    pub clues_by_operator_hour: BTreeMap<String, BTreeMap<u64, u32>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LongThreadProgressReport {
    pub operators_with_chapter_1: u32,
    pub operators_with_chapter_2: u32,
    pub operators_with_chapter_3: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperatorClueDiscovery {
    pub operator_name: String,
    pub time_to_first_clue_ms: Option<u64>,
    pub clues_per_hour_by_bucket: BTreeMap<String, BTreeMap<u64, u32>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClueDiscoveryReport {
    pub operators: Vec<OperatorClueDiscovery>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperatorActionDiscovery {
    pub operator_name: String,
    pub actions_attempted: u64,
    pub actions_to_first_thread_clue: Option<u64>,
    pub thread_clues_found: u32,
    pub clues_per_100_actions: f64,
    pub found_any_thread_clue: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActionDiscoveryReport {
    pub operators: Vec<OperatorActionDiscovery>,
}

fn long_procession_chapter(clue_id: &str) -> Option<u8> {
    if !clue_id.starts_with("long-procession:spine:") {
        return None;
    }
    let index: u32 = clue_id.rsplit(':').next()?.parse().ok()?;
    if index < 5 {
        Some(1)
    } else if index < 13 {
        Some(2)
    } else {
        Some(3)
    }
}

fn hub_bucket_for_clue(threads: &ThreadState, clue_id: &str) -> Option<HubDistanceBucket> {
    let site = threads.site_for_clue(clue_id)?;
    let pos = match site {
        ClueSite::Wild(p) => p,
        ClueSite::Static(_) => return None,
    };
    let (ax, ay) = pos.absolute();
    Some(HubDistanceBucket::from_chebyshev_from_gate(chebyshev_from_gate(ax, ay)))
}

/// Aggregate clue_seen rows into per-thread and per-operator-hour counts.
pub fn simulation_thread_reports(
    threads: &ThreadState,
    operator_names: &HashMap<PlayerId, String>,
    sim_start_ms: u64,
) -> Vec<ThreadSimReport> {
    let hour_ms = 3_600_000_u64;
    let mut out = Vec::new();
    for thread in threads.all_thread_defs() {
        let Some(final_id) = thread_final_clue_id(&thread) else {
            continue;
        };
        let mut reaching_final = 0_u32;
        let mut clues_by_operator_hour: BTreeMap<String, BTreeMap<u64, u32>> = BTreeMap::new();
        for operator in operator_names.keys() {
            let mut seen_final = false;
            for row in threads.seen.iter().filter(|row| row.operator_id == *operator) {
                if row.clue_id == final_id {
                    seen_final = true;
                }
                if !thread.clues.iter().any(|clue| clue.id == row.clue_id) {
                    continue;
                }
                let hour = row.first_seen_at.saturating_sub(sim_start_ms) / hour_ms;
                let name =
                    operator_names.get(operator).cloned().unwrap_or_else(|| operator.to_string());
                *clues_by_operator_hour.entry(name).or_default().entry(hour).or_default() += 1;
            }
            if seen_final {
                reaching_final += 1;
            }
        }
        out.push(ThreadSimReport {
            thread_id: thread.id.clone(),
            final_clue_id: final_id.to_string(),
            operators_reaching_final_clue: reaching_final,
            clues_by_operator_hour,
        });
    }
    out
}

pub fn simulation_long_thread_progress(
    threads: &ThreadState,
    operator_names: &HashMap<PlayerId, String>,
) -> LongThreadProgressReport {
    let mut ch1 = HashSet::new();
    let mut ch2 = HashSet::new();
    let mut ch3 = HashSet::new();
    for operator in operator_names.keys() {
        for row in threads.seen.iter().filter(|row| row.operator_id == *operator) {
            let Some(chapter) = long_procession_chapter(&row.clue_id) else {
                continue;
            };
            match chapter {
                1 => {
                    ch1.insert(*operator);
                }
                2 => {
                    ch2.insert(*operator);
                }
                _ => {
                    ch3.insert(*operator);
                }
            }
        }
    }
    LongThreadProgressReport {
        operators_with_chapter_1: u32::try_from(ch1.len()).unwrap_or(u32::MAX),
        operators_with_chapter_2: u32::try_from(ch2.len()).unwrap_or(u32::MAX),
        operators_with_chapter_3: u32::try_from(ch3.len()).unwrap_or(u32::MAX),
    }
}

pub fn simulation_clue_discovery_reports(
    threads: &ThreadState,
    operator_names: &HashMap<PlayerId, String>,
    sim_start_ms: u64,
) -> ClueDiscoveryReport {
    let hour_ms = 3_600_000_u64;
    let mut operators = Vec::new();
    for (operator, name) in operator_names {
        let rows: Vec<_> = threads.seen.iter().filter(|r| r.operator_id == *operator).collect();
        let time_to_first_clue_ms =
            rows.iter().map(|r| r.first_seen_at.saturating_sub(sim_start_ms)).min();
        let mut clues_per_hour_by_bucket: BTreeMap<String, BTreeMap<u64, u32>> = BTreeMap::new();
        for row in rows {
            let Some(bucket) = hub_bucket_for_clue(threads, &row.clue_id) else {
                continue;
            };
            let hour = row.first_seen_at.saturating_sub(sim_start_ms) / hour_ms;
            *clues_per_hour_by_bucket
                .entry(bucket.label().to_string())
                .or_default()
                .entry(hour)
                .or_default() += 1;
        }
        operators.push(OperatorClueDiscovery {
            operator_name: name.clone(),
            time_to_first_clue_ms,
            clues_per_hour_by_bucket,
        });
    }
    operators.sort_by(|a, b| a.operator_name.cmp(&b.operator_name));
    ClueDiscoveryReport { operators }
}

/// Primary discovery metrics: actions-based, counting thread clues only (not coverage flavor).
pub fn simulation_action_discovery_reports(
    threads: &ThreadState,
    operator_names: &HashMap<PlayerId, String>,
    actions_attempted: &HashMap<PlayerId, u64>,
    first_thread_clue_action: &HashMap<PlayerId, u64>,
) -> ActionDiscoveryReport {
    let mut operators = Vec::new();
    for (operator, name) in operator_names {
        let actions = actions_attempted.get(operator).copied().unwrap_or(0);
        let thread_clues_found = u32::try_from(
            threads
                .seen
                .iter()
                .filter(|r| r.operator_id == *operator)
                .filter(|r| threads.clue_def(&r.clue_id).is_some_and(|c| is_thread_clue(&c)))
                .count(),
        )
        .unwrap_or(u32::MAX);
        let actions_to_first_thread_clue = first_thread_clue_action.get(operator).copied();
        let clues_per_100 =
            if actions == 0 { 0.0 } else { f64::from(thread_clues_found) / actions as f64 * 100.0 };
        operators.push(OperatorActionDiscovery {
            operator_name: name.clone(),
            actions_attempted: actions,
            actions_to_first_thread_clue,
            thread_clues_found,
            clues_per_100_actions: clues_per_100,
            found_any_thread_clue: thread_clues_found > 0,
        });
    }
    operators.sort_by(|a, b| a.operator_name.cmp(&b.operator_name));
    ActionDiscoveryReport { operators }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiscoveryRunSummary {
    pub configured_authored_clues: u32,
    pub placed_authored_clues: u32,
    pub distinct_authored_reachable_sites: u32,
    pub thread_clues_inspected: u32,
    pub threads_with_any_discovery: u32,
    pub clue_to_clue_same_thread_transitions: u32,
    pub threads_reaching_configured_final: u32,
    pub invalid_or_unreachable_authored_clues: u32,
}

/// Aggregate discovery outcomes from simulation `clue_seen` rows (not static placement alone).
pub fn simulation_discovery_run_summary(threads: &ThreadState) -> DiscoveryRunSummary {
    use super::thread_audit::measure_thread_coverage_audit;

    let audit = measure_thread_coverage_audit(threads.world_seed());
    let configured_authored_clues: u32 =
        audit.authored_threads.iter().map(|row| row.configured_clues).sum();
    let placed_authored_clues: u32 =
        audit.authored_threads.iter().map(|row| row.placed_clues).sum();
    let distinct_authored_reachable_sites: u32 =
        audit.authored_threads.iter().map(|row| row.distinct_reachable_locations).sum();
    let invalid_or_unreachable_authored_clues: u32 =
        audit.authored_threads.iter().map(|row| row.invalid_or_unreachable_clues).sum();

    let mut thread_clues_inspected = 0_u32;
    let mut threads_with_discovery = HashSet::new();
    let mut transitions = 0_u32;
    let mut finals = 0_u32;

    let mut by_operator: HashMap<PlayerId, Vec<(u64, String)>> = HashMap::new();
    for row in &threads.seen {
        let Some(clue) = threads.clue_def(&row.clue_id) else {
            continue;
        };
        if !is_thread_clue(&clue) {
            continue;
        }
        thread_clues_inspected += 1;
        if let Some(thread) = threads.catalog.thread_for_clue(&row.clue_id) {
            threads_with_discovery.insert(thread.id.clone());
        }
        by_operator
            .entry(row.operator_id)
            .or_default()
            .push((row.first_seen_at, row.clue_id.clone()));
    }

    for rows in by_operator.values_mut() {
        rows.sort_by_key(|(at, _)| *at);
        let mut seen_in_thread: HashMap<String, Vec<String>> = HashMap::new();
        for (_, clue_id) in rows.iter() {
            let Some(thread) = threads.catalog.thread_for_clue(clue_id) else {
                continue;
            };
            let bucket = seen_in_thread.entry(thread.id.clone()).or_default();
            if let Some(prev) = bucket.last() {
                if prev != clue_id {
                    transitions += 1;
                }
            }
            bucket.push(clue_id.clone());
            if thread_final_clue_id(thread).is_some_and(|final_id| final_id == *clue_id) {
                finals += 1;
            }
        }
    }

    DiscoveryRunSummary {
        configured_authored_clues,
        placed_authored_clues,
        distinct_authored_reachable_sites,
        thread_clues_inspected,
        threads_with_any_discovery: u32::try_from(threads_with_discovery.len()).unwrap_or(u32::MAX),
        clue_to_clue_same_thread_transitions: transitions,
        threads_reaching_configured_final: finals,
        invalid_or_unreachable_authored_clues,
    }
}
