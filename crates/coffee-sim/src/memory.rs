//! Simulator-only memory (not authoritative game state).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

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
    /// Theory subjects seen via inspect hints or theory read (player-visible only).
    pub theory_subject_hints: BTreeSet<String>,
    pub public_trace_locations: BTreeSet<String>,
    /// location → trace kinds encountered at that tile
    trace_kinds_by_location: BTreeMap<String, BTreeSet<String>>,
    pub revisited_after_trace: BTreeSet<String>,
    last_locations: VecDeque<String>,
    last_move_from: Option<String>,
    last_exit: Option<String>,
    inspected_keywords: BTreeMap<String, BTreeSet<String>>,
    pub counted_clue_ids: BTreeSet<String>,
    pub actions_at_first_thread_clue: Option<u64>,
    pub thread_clues_found: u32,
    pub breadcrumb_target_region: Option<(i32, i32)>,
    pub newcomer_actions_remaining: Option<u32>,
}

impl AgentMemory {
    pub fn record_location(&mut self, location: &str, region_key: Option<&str>) {
        if self.public_trace_locations.contains(location)
            && self.visited_locations.contains(location)
        {
            self.revisited_after_trace.insert(location.to_string());
        }
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

    pub fn inspected_keyword_at(&self, location: &str, keyword: &str) -> bool {
        self.inspected_keywords.get(location).is_some_and(|set| set.contains(keyword))
    }

    pub fn record_inspected_keyword(&mut self, location: &str, keyword: &str) {
        self.inspected_keywords
            .entry(location.to_string())
            .or_default()
            .insert(keyword.to_string());
    }

    pub fn knows_region(&self, region_key: &str) -> bool {
        self.region_keys_visited.contains(region_key)
    }

    pub fn record_theory_subject_hint(&mut self, subject: &str) {
        self.theory_subject_hints.insert(subject.to_string());
    }

    pub fn record_theory_read(&mut self, subject: &str) {
        self.theory_subject_hints.insert(subject.to_string());
    }

    pub fn record_public_trace_at(&mut self, location: &str, kind: &str) {
        self.public_trace_locations.insert(location.to_string());
        self.trace_kinds_by_location
            .entry(location.to_string())
            .or_default()
            .insert(kind.to_string());
    }

    pub fn public_traces_at(&self, location: &str) -> Vec<String> {
        self.trace_kinds_by_location
            .get(location)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn note_thread_clue_found(&mut self, clue_id: &str, actions: u64) {
        if !self.counted_clue_ids.insert(clue_id.to_string()) {
            return;
        }
        self.thread_clues_found += 1;
        if self.actions_at_first_thread_clue.is_none() {
            self.actions_at_first_thread_clue = Some(actions);
        }
    }
}
