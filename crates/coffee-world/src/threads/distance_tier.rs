//! Distance tiers from the Hub gate — config boundaries, not hard-coded logic.

use crate::geography::{DensityBand, GATE_LOCAL_X, GATE_LOCAL_Y};

/// Chebyshev distance from the Hub gate at which `Mid` begins (exclusive upper bound for `Near`).
pub const TIER_NEAR_MAX_CHEB: i64 = 20;
/// Upper bound (inclusive) for `Mid`; beyond this is `Far` until `TIER_FAR_MAX_CHEB`.
pub const TIER_MID_MAX_CHEB: i64 = 48;
/// Upper bound for `Far`; beyond is `Deep`.
pub const TIER_FAR_MAX_CHEB: i64 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DistanceTier {
    Near,
    Mid,
    Far,
    Deep,
}

impl DistanceTier {
    pub fn from_chebyshev(dist: i64) -> Self {
        if dist <= TIER_NEAR_MAX_CHEB {
            Self::Near
        } else if dist <= TIER_MID_MAX_CHEB {
            Self::Mid
        } else if dist <= TIER_FAR_MAX_CHEB {
            Self::Far
        } else {
            Self::Deep
        }
    }

    pub fn from_absolute(abs_x: i64, abs_y: i64) -> Self {
        let gate_x = i64::from(GATE_LOCAL_X);
        let gate_y = i64::from(GATE_LOCAL_Y);
        let dist = (abs_x - gate_x).abs().max((abs_y - gate_y).abs());
        Self::from_chebyshev(dist)
    }

    pub fn allows_chapter(self, allowed: &[&str]) -> bool {
        let name = match self {
            Self::Near => "near",
            Self::Mid => "mid",
            Self::Far => "far",
            Self::Deep => "deep",
        };
        allowed.contains(&name)
    }

    /// Legacy three-band presentation (unchanged for existing filters).
    pub fn density_band(self) -> DensityBand {
        match self {
            Self::Near => DensityBand::NearHub,
            Self::Mid => DensityBand::Fringe,
            Self::Far | Self::Deep => DensityBand::Deep,
        }
    }
}
