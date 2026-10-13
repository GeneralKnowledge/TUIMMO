//! Simulator-only memory (not authoritative game state).

use std::collections::{BTreeSet, VecDeque};

/// Per-agent behavioural memory for action selection and metrics.
#[derive(Clone, Debug, Default)]
pub struct AgentMemory {
    pub visited_locations: BTreeSet<String>,
    pub inspected_locations: BTreeSet<String>,
    pub region_keys_visited: BTreeSet<String>,
    pub repeat_visits: u64,
    pub journal_records: u64,
    pub social_attempts: u64,
    pub social_successes: u64,
    pub successful_inspections: u64,
    pub home_returns: u64,
    last_locations: VecDeque<String>,
    last_move_from: Option<String>,
    last_exit: Option<String>,
}

impl AgentMemory {
    pub fn record_location(&mut self, location: &str, region_key: Option<&str>) {
        if self.visited_locations.insert(location.to_string()) {
            if let Some(key) = region_key {
                self.region_keys_visited.insert(key.to_string());
            }
        } else {
            self.repeat_visits += 1;
        }
        self.last_locations.push_back(location.to_string());
        while self.last_locations.len() > 3 {
            self.last_locations.pop_front();
        }
    }

    pub fn record_inspection(&mut self, location: &str) {
        self.inspected_locations.insert(location.to_string());
    }

    pub fn record_move(&mut self, from: &str, exit: &str) {
        self.last_move_from = Some(from.to_string());
        self.last_exit = Some(exit.to_string());
    }

    pub fn repeats_last_exit(&self, from: &str, exit: &str) -> bool {
        self.last_move_from.as_deref() == Some(from) && self.last_exit.as_deref() == Some(exit)
    }

    pub fn knows_location(&self, location: &str) -> bool {
        self.visited_locations.contains(location)
    }

    pub fn inspected_here(&self, location: &str) -> bool {
        self.inspected_locations.contains(location)
    }
}
