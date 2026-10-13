//! Deterministic virtual-time event queue.
//!
//! Ordering: ascending `time_ms`, then [`ScheduledEventKind`] (observations before agent
//! actions), then ascending `agent_index`, then `sequence`.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledEventKind {
    /// Read-only longitudinal sample at a virtual-time grid boundary.
    Observation,
    /// Gameplay scheduler slot for one agent.
    AgentAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledEvent {
    pub time_ms: u64,
    pub kind: ScheduledEventKind,
    pub agent_index: u32,
    pub sequence: u64,
}

impl PartialOrd for ScheduledEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.time_ms.cmp(&other.time_ms) {
            Ordering::Equal => match self.kind.cmp(&other.kind) {
                Ordering::Equal => match self.agent_index.cmp(&other.agent_index) {
                    Ordering::Equal => self.sequence.cmp(&other.sequence),
                    other => other,
                },
                other => other,
            },
            other => other,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct MinEvent(ScheduledEvent);

impl Ord for MinEvent {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.cmp(&self.0)
    }
}

impl PartialOrd for MinEvent {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Min-heap on virtual time (`BinaryHeap` is max-heap; [`MinEvent`] inverts ordering).
#[derive(Debug, Default)]
pub struct EventScheduler {
    heap: BinaryHeap<MinEvent>,
    next_sequence: u64,
}

impl EventScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn schedule(&mut self, time_ms: u64, agent_index: u32) {
        self.schedule_agent(time_ms, agent_index);
    }

    pub fn schedule_agent(&mut self, time_ms: u64, agent_index: u32) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.heap.push(MinEvent(ScheduledEvent {
            time_ms,
            kind: ScheduledEventKind::AgentAction,
            agent_index,
            sequence,
        }));
    }

    pub fn schedule_observation(&mut self, time_ms: u64) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.heap.push(MinEvent(ScheduledEvent {
            time_ms,
            kind: ScheduledEventKind::Observation,
            agent_index: 0,
            sequence,
        }));
    }

    pub fn pop_next(&mut self) -> Option<ScheduledEvent> {
        self.heap.pop().map(|MinEvent(event)| event)
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn has_pending_agent_action(&self) -> bool {
        self.heap.iter().any(|MinEvent(event)| event.kind == ScheduledEventKind::AgentAction)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Virtual milliseconds between an agent's consecutive actions.
    pub action_interval_ms: u64,
    /// Extra idle gap inserted when an agent chooses to wait.
    pub idle_extra_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { action_interval_ms: 60 * 60 * 1000, idle_extra_ms: 6 * 60 * 60 * 1000 }
    }
}

impl SchedulerConfig {
    pub fn next_event_ms(&self, after_slot_ms: u64, idle: bool) -> u64 {
        let extra = if idle { self.idle_extra_ms } else { 0 };
        after_slot_ms.saturating_add(self.action_interval_ms).saturating_add(extra)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_events_precede_agent_actions_at_equal_time() {
        let mut scheduler = EventScheduler::new();
        scheduler.schedule_agent(100, 1);
        scheduler.schedule_observation(100);
        scheduler.schedule_agent(100, 0);
        let first = scheduler.pop_next().expect("event");
        assert_eq!(first.kind, ScheduledEventKind::Observation);
        let second = scheduler.pop_next().expect("event");
        assert_eq!(second.kind, ScheduledEventKind::AgentAction);
        assert_eq!(second.agent_index, 0);
        let third = scheduler.pop_next().expect("event");
        assert_eq!(third.agent_index, 1);
    }
}
