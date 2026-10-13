//! Longitudinal observation snapshots (read-only; no gameplay effect).

use coffee_world::{SimulationOccupancySummary, WorldState};
use serde::{Deserialize, Serialize};

use crate::agent::SimAgent;
use crate::config::{SnapshotConfig, MAX_SNAPSHOT_COUNT};
use crate::runner::MetricsAccumulator;
use crate::scheduler::EventScheduler;
use crate::SimulationError;

/// Why this row exists in the snapshot history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    /// Run start (`virtual_start_ms`), before any gameplay scheduler event.
    Initial,
    /// Scheduled grid boundary reached via an observation event.
    Interval,
    /// Final processed gameplay state when capacity allows and it is not already recorded.
    Terminal,
}

/// One observation taken at a virtual-time boundary during a run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LongitudinalSnapshot {
    /// Virtual timestamp of this observation (`configured_start + elapsed`).
    pub observation_ms: u64,
    /// Milliseconds since configured run start.
    pub elapsed_virtual_ms: u64,
    /// Authoritative world clock at that observation.
    pub world_clock_ms: u64,
    pub kind: ObservationKind,
    pub cumulative: CumulativeObservation,
    pub world_state: WorldStateObservation,
    pub occupancy: OccupancyObservation,
}

/// Event counters accumulated from the start of the run through this observation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CumulativeObservation {
    pub attempted_commands: u64,
    pub successful_commands: u64,
    pub inactive_slots: u64,
    pub events_processed: u64,
    /// Distinct place keys after successful moves that changed location (simulator tally).
    pub unique_locations_visited_move_based: usize,
    /// Distinct wilderness region keys reached via qualifying activity (simulator memory).
    pub unique_wilderness_regions_explored: usize,
    pub repeat_visits: u64,
    pub home_returns: u64,
    pub inspect_attempts: u64,
    pub successful_inspections: u64,
    pub social_attempts: u64,
    pub social_successes: u64,
    pub journal_records: u64,
}

/// Current world-state counts at observation time (not lifetime event totals).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldStateObservation {
    pub active_home_claims: usize,
    pub regions_materialized: usize,
    pub discoveries_recorded: usize,
    pub unique_locations_occupied: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OccupancyObservation {
    pub occupied_location_count: usize,
    pub max_agents_sharing_one_location: u32,
    pub agents_in_hub_building: u32,
    pub agents_in_wilderness_near_hub: u32,
    pub agents_in_wilderness_fringe: u32,
    pub agents_in_wilderness_deep_squalor: u32,
    pub agents_in_other_static_places: u32,
    /// Fraction of agents standing in Hub building rooms (undefined when no agents).
    pub hub_agent_share: Option<f64>,
    /// Fraction of agents in deep Squalor wilderness tiles.
    pub deep_squalor_agent_share: Option<f64>,
}

/// Why no terminal row appears (when snapshots are enabled).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalSnapshotStatus {
    #[default]
    /// Gameplay ran but terminal handling has not completed (internal only).
    Pending,
    /// A distinct terminal row was stored.
    Included,
    /// Final gameplay slot already has an interval or initial row at the same `observation_ms`.
    DuplicateLastObservation,
    /// `max_snapshots` exhausted before a terminal row could be stored.
    OmittedCapacity,
    /// No gameplay scheduler events were processed.
    NoGameplayEvents,
}

/// Metadata for interval grid completeness (present when snapshot sampling is enabled).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSamplingSummary {
    pub interval_ms: u64,
    pub max_snapshots: u32,
    /// Grid boundaries that would fall inside the horizon but were not scheduled due to `max_snapshots`.
    pub grid_boundaries_skipped_capacity: u64,
    /// Whether a distinct terminal row was appended after gameplay finished.
    pub terminal_included: bool,
    pub terminal_status: TerminalSnapshotStatus,
}

impl From<SimulationOccupancySummary> for OccupancyObservation {
    fn from(summary: SimulationOccupancySummary) -> Self {
        Self {
            occupied_location_count: summary.occupied_location_count,
            max_agents_sharing_one_location: summary.max_agents_sharing_one_location,
            agents_in_hub_building: summary.agents_in_hub_building,
            agents_in_wilderness_near_hub: summary.agents_in_wilderness_near_hub,
            agents_in_wilderness_fringe: summary.agents_in_wilderness_fringe,
            agents_in_wilderness_deep_squalor: summary.agents_in_wilderness_deep_squalor,
            agents_in_other_static_places: summary.agents_in_other_static_places,
            hub_agent_share: None,
            deep_squalor_agent_share: None,
        }
    }
}

pub(crate) struct SnapshotCollector {
    interval_ms: u64,
    max_snapshots: u32,
    start_ms: u64,
    agent_count: u32,
    interval_observation_ms: Vec<u64>,
    grid_boundaries_skipped: u64,
    terminal_included: bool,
    terminal_status: TerminalSnapshotStatus,
    pub(crate) snapshots: Vec<LongitudinalSnapshot>,
}

impl SnapshotCollector {
    pub(crate) fn new(
        config: &SnapshotConfig,
        virtual_start_ms: u64,
        horizon_ms: u64,
        agent_count: u32,
    ) -> Result<Option<Self>, SimulationError> {
        let Some(interval_ms) = config.interval_ms else {
            return Ok(None);
        };
        if interval_ms == 0 {
            return Err(SimulationError::ZeroSnapshotInterval);
        }
        if config.max_snapshots == 0 {
            return Err(SimulationError::ZeroSnapshotLimit);
        }
        if config.max_snapshots > MAX_SNAPSHOT_COUNT {
            return Err(SimulationError::TooManySnapshots { max: MAX_SNAPSHOT_COUNT });
        }

        let (interval_observation_ms, grid_boundaries_skipped) = plan_interval_observations(
            virtual_start_ms,
            horizon_ms,
            interval_ms,
            config.max_snapshots,
        );

        Ok(Some(Self {
            interval_ms,
            max_snapshots: config.max_snapshots,
            start_ms: virtual_start_ms,
            agent_count,
            interval_observation_ms,
            grid_boundaries_skipped,
            terminal_included: false,
            terminal_status: TerminalSnapshotStatus::Pending,
            snapshots: Vec::new(),
        }))
    }

    pub(crate) fn enqueue_interval_observations(&self, scheduler: &mut EventScheduler) {
        for &time_ms in &self.interval_observation_ms {
            scheduler.schedule_observation(time_ms);
        }
    }

    pub(crate) fn sampling_summary(&self) -> SnapshotSamplingSummary {
        let terminal_status = match self.terminal_status {
            TerminalSnapshotStatus::Pending => TerminalSnapshotStatus::NoGameplayEvents,
            other => other,
        };
        SnapshotSamplingSummary {
            interval_ms: self.interval_ms,
            max_snapshots: self.max_snapshots,
            grid_boundaries_skipped_capacity: self.grid_boundaries_skipped,
            terminal_included: self.terminal_included,
            terminal_status,
        }
    }

    pub(crate) fn finalize_terminal_status(&mut self, had_gameplay_events: bool) {
        if self.terminal_status != TerminalSnapshotStatus::Pending {
            return;
        }
        self.terminal_status = if had_gameplay_events {
            TerminalSnapshotStatus::DuplicateLastObservation
        } else {
            TerminalSnapshotStatus::NoGameplayEvents
        };
    }

    pub(crate) fn capture_initial(&mut self, world: &WorldState, metrics: &MetricsSnapshotInput) {
        if self.snapshots.len() >= self.max_snapshots as usize {
            return;
        }
        self.snapshots.push(build_snapshot(
            self.start_ms,
            self.start_ms,
            ObservationKind::Initial,
            world,
            metrics,
            self.agent_count,
        ));
    }

    pub(crate) fn capture_interval(
        &mut self,
        boundary_ms: u64,
        world: &WorldState,
        metrics: &MetricsSnapshotInput,
    ) {
        if self.snapshots.len() >= self.max_snapshots as usize {
            return;
        }
        self.snapshots.push(build_snapshot(
            boundary_ms,
            self.start_ms,
            ObservationKind::Interval,
            world,
            metrics,
            self.agent_count,
        ));
    }

    /// Final observation at the last processed gameplay slot when capacity allows.
    pub(crate) fn capture_terminal(
        &mut self,
        world: &WorldState,
        metrics: &MetricsSnapshotInput,
        last_gameplay_slot_ms: u64,
    ) {
        if self.snapshots.len() >= self.max_snapshots as usize {
            self.terminal_status = TerminalSnapshotStatus::OmittedCapacity;
            return;
        }
        if self.snapshots.last().is_some_and(|snap| snap.observation_ms == last_gameplay_slot_ms) {
            self.terminal_status = TerminalSnapshotStatus::DuplicateLastObservation;
            return;
        }
        self.snapshots.push(build_snapshot(
            last_gameplay_slot_ms,
            self.start_ms,
            ObservationKind::Terminal,
            world,
            metrics,
            self.agent_count,
        ));
        self.terminal_included = true;
        self.terminal_status = TerminalSnapshotStatus::Included;
    }
}

/// Returns scheduled interval boundary times and how many in-horizon grid points were skipped for capacity.
fn plan_interval_observations(
    start_ms: u64,
    horizon_ms: u64,
    interval_ms: u64,
    max_snapshots: u32,
) -> (Vec<u64>, u64) {
    let all_boundaries = all_interval_boundaries(start_ms, horizon_ms, interval_ms);
    let remaining_after_initial = max_snapshots.saturating_sub(1);
    if remaining_after_initial == 0 {
        return (Vec::new(), all_boundaries.len() as u64);
    }
    let reserve_terminal = 1.min(remaining_after_initial);
    let interval_slots = remaining_after_initial.saturating_sub(reserve_terminal);
    let scheduled_count = interval_slots.min(all_boundaries.len() as u32) as usize;
    let skipped = all_boundaries.len().saturating_sub(scheduled_count) as u64;
    (all_boundaries.into_iter().take(scheduled_count).collect(), skipped)
}

fn all_interval_boundaries(start_ms: u64, horizon_ms: u64, interval_ms: u64) -> Vec<u64> {
    let mut boundaries = Vec::new();
    let Some(mut next) = start_ms.checked_add(interval_ms) else {
        return boundaries;
    };
    loop {
        if next > horizon_ms {
            break;
        }
        boundaries.push(next);
        next = match next.checked_add(interval_ms) {
            Some(value) => value,
            None => break,
        };
    }
    boundaries
}

pub(crate) struct MetricsSnapshotInput<'a> {
    pub metrics: &'a MetricsAccumulator,
    pub total_attempted: u64,
    pub successful_commands: u64,
    pub inactive_slots: u64,
    pub events_processed: u64,
    pub agents: &'a [SimAgent],
}

fn build_snapshot(
    observation_ms: u64,
    start_ms: u64,
    kind: ObservationKind,
    world: &WorldState,
    input: &MetricsSnapshotInput<'_>,
    agent_count: u32,
) -> LongitudinalSnapshot {
    let world_snap = world.outcome_snapshot();
    let occupancy_raw = world.simulation_occupancy_summary();
    let mut occupancy = OccupancyObservation::from(occupancy_raw);
    let total_agents = agent_count.max(1);
    occupancy.hub_agent_share = share(occupancy.agents_in_hub_building, total_agents);
    occupancy.deep_squalor_agent_share =
        share(occupancy.agents_in_wilderness_deep_squalor, total_agents);

    let mut regions = std::collections::BTreeSet::new();
    for agent in input.agents {
        regions.extend(agent.memory.region_keys_visited.iter().cloned());
    }
    regions.extend(input.metrics.regions_visited.iter().cloned());

    LongitudinalSnapshot {
        observation_ms,
        elapsed_virtual_ms: observation_ms.saturating_sub(start_ms),
        world_clock_ms: world.simulation_now_ms(),
        kind,
        cumulative: CumulativeObservation {
            attempted_commands: input.total_attempted,
            successful_commands: input.successful_commands,
            inactive_slots: input.inactive_slots,
            events_processed: input.events_processed,
            unique_locations_visited_move_based: input.metrics.locations_visited.len(),
            unique_wilderness_regions_explored: regions.len(),
            repeat_visits: input.agents.iter().map(|agent| agent.memory.repeat_visits).sum(),
            home_returns: input.metrics.home_returns,
            inspect_attempts: input.metrics.inspect_attempts,
            successful_inspections: input.metrics.successful_inspections,
            social_attempts: input.metrics.social_attempts,
            social_successes: input.metrics.social_successes,
            journal_records: input.metrics.journal_records,
        },
        world_state: WorldStateObservation {
            active_home_claims: world_snap.active_home_count,
            regions_materialized: world_snap.region_count,
            discoveries_recorded: world_snap.discovery_count,
            unique_locations_occupied: world_snap.unique_locations_occupied,
        },
        occupancy,
    }
}

fn share(count: u32, total: u32) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(f64::from(count) / f64::from(total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_planning_reserves_terminal_slot_and_counts_skips() {
        let start = 1_000u64;
        let horizon = start + 10 * MS_FOR_TEST;
        let interval = MS_FOR_TEST;
        let (scheduled, skipped) = plan_interval_observations(start, horizon, interval, 3);
        assert_eq!(scheduled.len(), 1);
        assert_eq!(scheduled[0], start + interval);
        assert_eq!(skipped, 9);
    }

    #[test]
    fn boundary_enumeration_stops_on_overflow() {
        let start = u64::MAX - 1;
        let boundaries = all_interval_boundaries(start, u64::MAX, 4);
        assert!(boundaries.is_empty());
    }

    const MS_FOR_TEST: u64 = 1000;
}
