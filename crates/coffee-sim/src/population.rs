//! Population composition and deterministic profile assignment.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::profile::Profile;
use crate::SimulationError;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopulationMix {
    pub wanderer: u32,
    pub investigator: u32,
    pub homebody: u32,
    pub social_player: u32,
    pub occasional_visitor: u32,
}

impl PopulationMix {
    pub fn uniform_wanderer() -> Self {
        Self { wanderer: 100, ..Default::default() }
    }

    pub fn exploration_heavy() -> Self {
        Self { wanderer: 60, investigator: 30, social_player: 10, ..Default::default() }
    }

    pub fn home_oriented() -> Self {
        Self {
            homebody: 50,
            occasional_visitor: 35,
            wanderer: 10,
            social_player: 5,
            ..Default::default()
        }
    }

    pub fn validate(&self) -> Result<(), SimulationError> {
        let sum = self.wanderer
            + self.investigator
            + self.homebody
            + self.social_player
            + self.occasional_visitor;
        if sum == 0 {
            return Err(SimulationError::InvalidPopulationMix(
                "at least one profile weight must be non-zero".into(),
            ));
        }
        Ok(())
    }

    /// Proportional counts summing to `agent_count` (largest-remainder method).
    pub fn allocate(&self, agent_count: u32) -> Result<Vec<Profile>, SimulationError> {
        self.validate()?;
        if agent_count == 0 {
            return Err(SimulationError::NoAgents);
        }
        let entries: [(Profile, u32); 5] = [
            (Profile::Wanderer, self.wanderer),
            (Profile::Investigator, self.investigator),
            (Profile::Homebody, self.homebody),
            (Profile::SocialPlayer, self.social_player),
            (Profile::OccasionalVisitor, self.occasional_visitor),
        ];
        let sum: u32 = entries.iter().map(|(_, w)| w).sum();
        let mut counts = [0u32; 5];
        let mut assigned = 0u32;
        let mut remainders = Vec::new();
        for (idx, (_, weight)) in entries.iter().enumerate() {
            let exact = u64::from(agent_count) * u64::from(*weight) / u64::from(sum);
            counts[idx] = exact as u32;
            assigned += counts[idx];
            let rem = (u64::from(agent_count) * u64::from(*weight)) % u64::from(sum);
            remainders.push((idx, rem));
        }
        remainders.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut left = agent_count.saturating_sub(assigned);
        for (idx, _) in remainders {
            if left == 0 {
                break;
            }
            counts[idx] += 1;
            left -= 1;
        }
        let mut out = Vec::with_capacity(agent_count as usize);
        for (idx, (profile, _)) in entries.iter().enumerate() {
            for _ in 0..counts[idx] {
                out.push(*profile);
            }
        }
        debug_assert_eq!(out.len(), agent_count as usize);
        Ok(out)
    }
}

pub fn assign_profiles(
    seed: u64,
    agent_count: u32,
    mix: &PopulationMix,
) -> Result<Vec<Profile>, SimulationError> {
    let mut profiles = mix.allocate(agent_count)?;
    let mut rng = StdRng::seed_from_u64(seed.wrapping_mul(0x517CC1B7_27220A95));
    for i in (1..profiles.len()).rev() {
        let j = rng.random_range(0..=i);
        profiles.swap(i, j);
    }
    Ok(profiles)
}
