//! Read-only helpers and virtual clock controls for offline population simulation.
//!
//! Production servers use [`WorldState::new`] with the system clock. Simulators
//! should use [`WorldState::for_simulation`] and [`WorldState::advance_virtual_time`].

use std::collections::BTreeMap;

use terminus_core::{GameError, PlayerId, SessionId};

use crate::geography::GENERATION_V1;
use crate::geography::{DensityBand, UrbanContext, WildernessPos, OUTSKIRTS_FROM, ROOF_LOCATION};
use crate::home::Habitation;
use crate::model::STARTER_LOCATION;
use crate::state::WorldState;

/// Stable world-generation label recorded in simulation reports.
pub const WORLD_GENERATION_VERSION: u16 = GENERATION_V1;

/// Aggregate world outcomes after a simulation run (read-only; no gameplay effect).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldOutcomeSnapshot {
    pub region_count: usize,
    pub discovery_count: usize,
    pub active_home_count: usize,
    pub unique_locations_occupied: usize,
    /// Field journal content keys with at least two distinct recorders.
    pub corroborated_field_keys: usize,
}

/// Aggregate agent placement for simulation reporting (no per-location keys exposed).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SimulationOccupancySummary {
    /// Distinct place keys with one or more agents present.
    pub occupied_location_count: usize,
    /// Largest number of agents sharing a single place key.
    pub max_agents_sharing_one_location: u32,
    /// Agents in authored Hub rooms (`coffee-shop`, `roof-array`).
    pub agents_in_hub_building: u32,
    /// Wilderness tiles classified as near-Hub density band.
    pub agents_in_wilderness_near_hub: u32,
    /// Wilderness tiles in the outer fringe band.
    pub agents_in_wilderness_fringe: u32,
    /// Wilderness tiles in deep Squalor density band.
    pub agents_in_wilderness_deep_squalor: u32,
    /// Other static places (e.g. subway stations), not Hub building or wilderness.
    pub agents_in_other_static_places: u32,
}

impl WorldState {
    /// Offline simulation clock: no wall clock, optional starting timestamp.
    ///
    /// Each [`WorldState::apply`] and [`WorldState::connect_player`] still advances
    /// time by one millisecond (same as [`WorldState::for_tests`]). Use
    /// [`Self::advance_virtual_time`] between scheduled actions for larger jumps.
    pub fn for_simulation(initial_now_ms: u64) -> Self {
        let mut world = Self::for_tests();
        world.now_ms = initial_now_ms;
        world
    }

    /// Current authoritative timestamp (virtual or system, depending on mode).
    pub fn simulation_now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Advance virtual time when the system clock is disabled.
    ///
    /// No-op while `use_system_clock` is true so production timing is unchanged.
    pub fn advance_virtual_time(&mut self, delta_ms: u64) {
        if self.use_system_clock {
            return;
        }
        self.now_ms = self.now_ms.saturating_add(delta_ms);
    }

    /// Align virtual time after registration setup so scheduled play starts at the configured epoch.
    ///
    /// `connect_player` / `apply` each advance the test clock by 1 ms; call this once all
    /// synthetic agents are connected and before scheduling their first actions.
    pub fn align_simulation_clock(&mut self, now_ms: u64) {
        if self.use_system_clock {
            return;
        }
        self.now_ms = now_ms;
    }

    /// Snapshot of persisted geography and home outcomes for reporting.
    pub fn outcome_snapshot(&self) -> WorldOutcomeSnapshot {
        let unique_locations_occupied = self
            .operators
            .values()
            .map(|op| op.location_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        WorldOutcomeSnapshot {
            region_count: self.geography.regions.len(),
            discovery_count: self.geography.discoveries.len(),
            active_home_count: self.homes.homes.iter().filter(|h| h.is_active()).count(),
            unique_locations_occupied,
            corroborated_field_keys: self.geography.corroborated_field_key_count(),
        }
    }

    /// Where agents stand right now, aggregated for offline experiments.
    pub fn simulation_occupancy_summary(&self) -> SimulationOccupancySummary {
        let mut per_location: BTreeMap<String, u32> = BTreeMap::new();
        for op in self.operators.values() {
            *per_location.entry(op.location_id.clone()).or_default() += 1;
        }
        let mut summary = SimulationOccupancySummary {
            occupied_location_count: per_location.len(),
            max_agents_sharing_one_location: per_location.values().copied().max().unwrap_or(0),
            ..Default::default()
        };
        for (location, count) in per_location {
            summary = summary.add_agents(classify_sim_location(&location), count);
        }
        summary
    }

    /// Base habitation for a player's active home at the current world time.
    pub fn active_home_place_key(&self, player: PlayerId) -> Option<String> {
        self.homes.active_for(player).map(|home| home.place_key.clone())
    }

    /// Place key for a session (same as live `Game::location_key`).
    pub fn location_key_for_sim(&self, session_id: SessionId) -> Result<String, GameError> {
        let player = self.player_of(session_id).ok_or(GameError::UnknownSession)?;
        self.operator(player).map(|op| op.location_id.clone()).ok_or(GameError::UnknownSession)
    }

    pub fn active_home_base_habitation(&self, player: PlayerId) -> Option<Habitation> {
        self.homes.active_for(player).map(|home| home.base_habitation(self.now_ms))
    }

    /// Full habitation (time, returns, exploration traits) at the current world time.
    pub fn active_home_habitation(&self, player: PlayerId) -> Option<Habitation> {
        self.homes.active_for(player).map(|home| {
            let traits = self.exploration_traits_for(player);
            home.habitation(self.now_ms, traits)
        })
    }
}

fn classify_sim_location(location: &str) -> SimLocationZone {
    if location == STARTER_LOCATION || location == ROOF_LOCATION || location == OUTSKIRTS_FROM {
        return SimLocationZone::HubBuilding;
    }
    if let Some(pos) = WildernessPos::parse(location) {
        return match UrbanContext::at(pos).band {
            DensityBand::NearHub => SimLocationZone::WildernessNearHub,
            DensityBand::Fringe => SimLocationZone::WildernessFringe,
            DensityBand::Deep => SimLocationZone::WildernessDeepSqualor,
        };
    }
    SimLocationZone::OtherStatic
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SimLocationZone {
    HubBuilding,
    WildernessNearHub,
    WildernessFringe,
    WildernessDeepSqualor,
    OtherStatic,
}

impl SimulationOccupancySummary {
    fn add_agents(mut self, zone: SimLocationZone, count: u32) -> Self {
        match zone {
            SimLocationZone::HubBuilding => self.agents_in_hub_building += count,
            SimLocationZone::WildernessNearHub => self.agents_in_wilderness_near_hub += count,
            SimLocationZone::WildernessFringe => self.agents_in_wilderness_fringe += count,
            SimLocationZone::WildernessDeepSqualor => {
                self.agents_in_wilderness_deep_squalor += count
            }
            SimLocationZone::OtherStatic => self.agents_in_other_static_places += count,
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use terminus_core::{Permissions, SessionIdentity};

    use crate::geography::Terrain;
    use crate::state::starter_operator;

    use super::*;

    #[test]
    fn advance_virtual_time_is_noop_with_system_clock() {
        let mut world = WorldState::new();
        let before = world.simulation_now_ms();
        world.advance_virtual_time(1_000_000);
        assert!(world.simulation_now_ms() >= before);
    }

    #[test]
    fn simulation_clock_jumps_for_home_habitation() {
        let mut world = WorldState::for_simulation(0);
        let player = terminus_core::PlayerId::new();
        world.insert_operator(starter_operator(player, uuid::Uuid::new_v4(), "t", "t"));
        let identity = SessionIdentity::new(player, "t", Permissions::player());
        let session = identity.session_id;
        world.connect_player(identity).unwrap();
        for line in ["go up", "outskirts"] {
            world.apply(session, line).unwrap();
        }
        let identity = world.geography.regions.values().next().unwrap().identity;
        world.apply_terrain_mod(identity, 8, 9, Terrain::Grass, "trail", player).unwrap();
        world.apply(session, "north").unwrap();
        world.apply(session, "home claim").unwrap();
        assert_eq!(world.active_home_base_habitation(player), Some(Habitation::Unsettled));
        world.advance_virtual_time(crate::home::SETTLING_TIME_MS);
        assert_eq!(world.active_home_base_habitation(player), Some(Habitation::SettlingIn));
    }
}
