//! Longitudinal observation snapshots (read-only; no gameplay effect).

use coffee_world::{SimulationOccupancySummary, WorldState};
use serde::{Deserialize, Serialize};

use crate::agent::SimAgent;
use crate::config::{SnapshotConfig, MAX_SNAPSHOT_COUNT};
use crate::runner::MetricsAccumulator;
use crate::SimulationError;

/// One observation taken at a virtual-time boundary during a run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LongitudinalSnapshot {
    /// Virtual timestamp of this observation (`configured_start + elapsed`).
    pub observation_ms: u64,
    /// Milliseconds since configured run start.
    pub elapsed_virtual_ms: u64,
    /// Authoritative world clock at observation time.
    pub world_clock_ms: u64,
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

pub struct SnapshotCollector {
    interval_ms: u64,
    max_snapshots: u32,
    start_ms: u64,
    agent_count: u32,
    next_observation_ms: u64,
    pub snapshots: Vec<LongitudinalSnapshot>,
}

impl SnapshotCollector {
    pub fn new(
        config: &SnapshotConfig,
        virtual_start_ms: u64,
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
        Ok(Some(Self {
            interval_ms,
            max_snapshots: config.max_snapshots,
            start_ms: virtual_start_ms,
            agent_count,
            next_observation_ms: virtual_start_ms,
            snapshots: Vec::new(),
        }))
    }

    pub fn capture_initial(&mut self, world: &WorldState, metrics: &MetricsSnapshotInput) {
        self.capture_due(world, metrics, self.start_ms);
    }

    pub fn capture_due(
        &mut self,
        world: &WorldState,
        metrics: &MetricsSnapshotInput,
        current_slot_ms: u64,
    ) {
        while self.snapshots.len() < self.max_snapshots as usize
            && current_slot_ms >= self.next_observation_ms
        {
            let observation_ms = self.next_observation_ms;
            self.snapshots.push(build_snapshot(
                observation_ms,
                self.start_ms,
                world,
                metrics,
                self.agent_count,
            ));
            self.next_observation_ms =
                self.next_observation_ms.checked_add(self.interval_ms).unwrap_or(u64::MAX);
        }
    }

    /// Final observation at the last processed scheduler slot when capacity allows.
    pub fn capture_terminal(
        &mut self,
        world: &WorldState,
        metrics: &MetricsSnapshotInput,
        last_event_slot_ms: u64,
    ) {
        if self.snapshots.len() >= self.max_snapshots as usize {
            return;
        }
        if self.snapshots.last().is_some_and(|snap| snap.observation_ms == last_event_slot_ms) {
            return;
        }
        self.snapshots.push(build_snapshot(
            last_event_slot_ms,
            self.start_ms,
            world,
            metrics,
            self.agent_count,
        ));
    }
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
