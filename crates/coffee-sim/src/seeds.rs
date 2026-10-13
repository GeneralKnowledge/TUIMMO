//! Deterministic replicate seed schedules.

use serde::{Deserialize, Serialize};

use crate::SimulationError;

/// Stride mixed into [`replicate_seed`] (documented constant; not execution-order dependent).
pub const REPLICATE_SEED_STRIDE: u64 = 0x9E37_79B9_7F4A_7C15;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum ReplicateSeedSchedule {
    /// Use these seeds exactly (one replicate per entry).
    Explicit { seeds: Vec<u64> },
    /// Derive `replicate_count` distinct seeds from `base_seed`.
    FromBase { base_seed: u64, replicate_count: u32 },
}

impl ReplicateSeedSchedule {
    pub fn resolve(&self) -> Result<Vec<u64>, SimulationError> {
        match self {
            Self::Explicit { seeds } => {
                if seeds.is_empty() {
                    return Err(SimulationError::EmptySeedSchedule);
                }
                Ok(seeds.clone())
            }
            Self::FromBase { base_seed, replicate_count } => {
                if *replicate_count == 0 {
                    return Err(SimulationError::EmptySeedSchedule);
                }
                Ok((0..*replicate_count).map(|index| replicate_seed(*base_seed, index)).collect())
            }
        }
    }
}

/// Deterministic SplitMix-style derivation: same `(base, index)` → same seed on all platforms.
pub fn replicate_seed(base_seed: u64, replicate_index: u32) -> u64 {
    let mut x =
        base_seed.wrapping_add(u64::from(replicate_index).wrapping_mul(REPLICATE_SEED_STRIDE));
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_seeds_are_deterministic_and_distinct_for_small_range() {
        let a: Vec<_> = (0..5).map(|i| replicate_seed(1000, i)).collect();
        let b: Vec<_> = (0..5).map(|i| replicate_seed(1000, i)).collect();
        assert_eq!(a, b);
        assert_eq!(a.len(), a.iter().collect::<std::collections::HashSet<_>>().len());
    }
}
