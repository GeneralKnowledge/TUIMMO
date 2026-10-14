//! Offline metrics for thread clue discovery (simulation reporting only).

use std::collections::{BTreeMap, HashMap};

use terminus_core::PlayerId;

use serde::{Deserialize, Serialize};

use super::{thread_final_clue_id, ThreadState};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadSimReport {
    pub thread_id: String,
    pub final_clue_id: String,
    pub operators_reaching_final_clue: u32,
    pub clues_by_operator_hour: BTreeMap<String, BTreeMap<u64, u32>>,
}

/// Aggregate clue_seen rows into per-thread and per-operator-hour counts.
pub fn simulation_thread_reports(
    threads: &ThreadState,
    operator_names: &HashMap<PlayerId, String>,
    sim_start_ms: u64,
) -> Vec<ThreadSimReport> {
    let hour_ms = 3_600_000_u64;
    let mut out = Vec::new();
    for thread in &threads.catalog.threads {
        let Some(final_id) = thread_final_clue_id(thread) else {
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
