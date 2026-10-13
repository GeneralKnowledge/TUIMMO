//! Optional deterministic action trace for debugging and regression tests.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceOutcome {
    Success,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionTraceEntry {
    /// Scheduler slot time (world clock is advanced to this before the command).
    pub event_slot_ms: u64,
    /// World `now_ms` immediately before `WorldState::apply`.
    pub world_ms_before_command: u64,
    /// World `now_ms` after the command (includes the engine's +1 ms tick).
    pub world_ms_after_command: u64,
    pub agent_index: u32,
    pub command: String,
    pub outcome: TraceOutcome,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionTrace {
    pub entries: Vec<ActionTraceEntry>,
    pub truncated: bool,
}

impl ActionTrace {
    pub fn push_bounded(&mut self, entry: ActionTraceEntry, limit: u32) {
        if (self.entries.len() as u32) < limit {
            self.entries.push(entry);
        } else {
            self.truncated = true;
        }
    }
}
