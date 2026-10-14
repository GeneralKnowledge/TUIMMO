//! Configurable coverage grid (defaults match shipped constants).

/// Grid and gap settings for procedural inspectable placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoverageParams {
    pub grid_step: u8,
    pub max_gap: u8,
}

impl Default for CoverageParams {
    fn default() -> Self {
        Self {
            grid_step: super::coverage::COVERAGE_GRID_STEP,
            max_gap: super::coverage::INSPECTABLE_MAX_GAP,
        }
    }
}

impl CoverageParams {
    pub fn new(grid_step: u8, max_gap: u8) -> Self {
        Self { grid_step, max_gap }
    }

    pub fn validate(self) -> Result<Self, &'static str> {
        if self.max_gap < self.grid_step {
            return Err("max_gap must be >= grid_step");
        }
        if self.grid_step == 0 {
            return Err("grid_step must be positive");
        }
        Ok(self)
    }
}
