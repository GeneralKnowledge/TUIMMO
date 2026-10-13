//! Cross-operator interaction metrics derived from gameplay-visible outcomes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use coffee_world::WorldState;
use serde::{Deserialize, Serialize};
use terminus_core::PlayerId;

use crate::agent::SimAgent;
use crate::memory::AgentMemory;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct InteractionMetrics {
    pub public_traces_created: u64,
    pub traces_encountered_by_later_operators: u64,
    pub distinct_encountering_operators: u32,
    pub theory_subjects_multi_author: u32,
    pub locations_revisited_after_trace: u32,
    pub connected_sequences: u32,
    pub follow_up_actions_after_encounter: u32,
    pub encounter_to_follow_up_rate: Option<f64>,
    pub trace_encounter_rate: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryLine {
    pub slot_ms: u64,
    pub agent: String,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryTranscript {
    pub lines: Vec<StoryLine>,
}

#[derive(Clone, Debug)]
struct TraceRecord {
    creator_index: u32,
    location: String,
    kind: String,
    subject: Option<String>,
    created_slot_ms: u64,
}

#[derive(Default)]
pub struct InteractionTracker {
    traces: Vec<TraceRecord>,
    /// (trace_idx, agent_index) — at most one encounter per pair.
    encounters: HashSet<(usize, u32)>,
    sequences: Vec<StoryTranscript>,
    pending_encounter: HashMap<u32, (usize, u64)>,
    follow_up_actions: u32,
}

impl InteractionTracker {
    pub fn record_trace_created(
        &mut self,
        agent_index: u32,
        location: &str,
        kind: &str,
        subject: Option<String>,
        slot_ms: u64,
    ) {
        self.traces.push(TraceRecord {
            creator_index: agent_index,
            location: location.to_string(),
            kind: kind.to_string(),
            subject,
            created_slot_ms: slot_ms,
        });
    }

    pub fn note_encounters(
        &mut self,
        agent_index: u32,
        memory: &AgentMemory,
        location: &str,
        slot_ms: u64,
    ) {
        let kinds = memory.public_traces_at(location);
        if kinds.is_empty() {
            return;
        }
        for (idx, trace) in self.traces.iter().enumerate() {
            if trace.creator_index == agent_index || trace.location != location {
                continue;
            }
            let matched = kinds.iter().any(|k| k == &trace.kind)
                || trace
                    .subject
                    .as_ref()
                    .is_some_and(|s| kinds.iter().any(|k| k == &format!("theory:{s}")));
            if !matched {
                continue;
            }
            if self.encounters.insert((idx, agent_index)) {
                self.pending_encounter.insert(agent_index, (idx, slot_ms));
            }
        }
    }

    pub fn record_meaningful_action(
        &mut self,
        agent_index: u32,
        agent_name: &str,
        command: &str,
        location: &str,
        slot_ms: u64,
    ) {
        let meaningful = command.starts_with("theory post")
            || (command.starts_with("sign ") && !command.contains("read"))
            || command.starts_with("trail ")
            || command.starts_with("journal record")
            || command.starts_with("inspect ");
        if !meaningful {
            return;
        }
        let Some((trace_idx, enc_slot)) = self.pending_encounter.remove(&agent_index) else {
            return;
        };
        let trace = &self.traces[trace_idx];
        self.follow_up_actions += 1;
        let lines = vec![
            StoryLine {
                slot_ms: trace.created_slot_ms,
                agent: format!("sim-{:04}", trace.creator_index),
                summary: format!(
                    "Left {} at {} {}",
                    trace.kind,
                    trace.location,
                    trace.subject.as_deref().unwrap_or("")
                ),
            },
            StoryLine {
                slot_ms: enc_slot,
                agent: agent_name.to_string(),
                summary: format!("Encountered prior {} at {}", trace.kind, location),
            },
            StoryLine {
                slot_ms,
                agent: agent_name.to_string(),
                summary: format!("Responded: {command}"),
            },
        ];
        self.sequences.push(StoryTranscript { lines });
    }

    pub fn finalize(
        self,
        world: &WorldState,
        agents: &[SimAgent],
        player_to_index: &HashMap<PlayerId, u32>,
    ) -> (InteractionMetrics, StoryTranscript) {
        let mut metrics = InteractionMetrics {
            public_traces_created: self.traces.len() as u64,
            traces_encountered_by_later_operators: self.encounters.len() as u64,
            distinct_encountering_operators: self
                .encounters
                .iter()
                .map(|(_, idx)| *idx)
                .collect::<BTreeSet<_>>()
                .len() as u32,
            follow_up_actions_after_encounter: self.follow_up_actions,
            connected_sequences: self.sequences.len() as u32,
            ..Default::default()
        };

        let mut authors_per_subject: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        for (topic, authors) in world.simulation_theory_authors_by_topic() {
            for author in authors {
                if let Some(&idx) = player_to_index.get(&author) {
                    authors_per_subject.entry(topic.clone()).or_default().insert(idx);
                }
            }
        }
        metrics.theory_subjects_multi_author =
            authors_per_subject.values().filter(|set| set.len() >= 2).count() as u32;

        for agent in agents {
            for loc in &agent.memory.revisited_after_trace {
                if agent.memory.public_trace_locations.contains(loc) {
                    metrics.locations_revisited_after_trace += 1;
                }
            }
        }

        metrics.trace_encounter_rate = if metrics.public_traces_created == 0 {
            None
        } else {
            Some(
                metrics.traces_encountered_by_later_operators as f64
                    / metrics.public_traces_created as f64,
            )
        };
        metrics.encounter_to_follow_up_rate = if metrics.traces_encountered_by_later_operators == 0
        {
            None
        } else {
            Some(
                metrics.follow_up_actions_after_encounter as f64
                    / metrics.traces_encountered_by_later_operators as f64,
            )
        };

        let representative = self
            .sequences
            .first()
            .cloned()
            .unwrap_or_default()
            .lines
            .into_iter()
            .collect::<Vec<_>>();
        (metrics, StoryTranscript { lines: representative })
    }
}

pub fn player_index_map(agents: &[SimAgent]) -> HashMap<PlayerId, u32> {
    agents.iter().map(|a| (a.player_id, a.index)).collect()
}
