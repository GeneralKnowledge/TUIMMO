//! Duration arithmetic and configuration limits.

use serde::{Deserialize, Serialize};

use crate::runner::MS_PER_DAY;
use crate::SimulationError;

/// Upper bound for synthetic agents in phase 1.x.
pub const MAX_AGENT_COUNT: u32 = 256;

/// Default cap on optional action trace entries.
pub const DEFAULT_TRACE_LIMIT: u32 = 5_000;

/// Default cap on longitudinal snapshots when sampling is enabled.
pub const DEFAULT_SNAPSHOT_MAX: u32 = 128;

/// Hard upper bound on stored snapshots per run.
pub const MAX_SNAPSHOT_COUNT: u32 = 4_096;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Virtual milliseconds between observations (`None` = disabled).
    pub interval_ms: Option<u64>,
    pub max_snapshots: u32,
}

impl SnapshotConfig {
    pub fn disabled() -> Self {
        Self { interval_ms: None, max_snapshots: DEFAULT_SNAPSHOT_MAX }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValidatedSchedule {
    pub virtual_start_ms: u64,
    pub virtual_duration_ms: u64,
    pub configured_horizon_ms: u64,
}

pub fn virtual_duration_from_days(days: f64) -> Result<u64, SimulationError> {
    if !days.is_finite() || days <= 0.0 {
        return Err(SimulationError::InvalidDuration);
    }
    let ms = days * MS_PER_DAY as f64;
    if !ms.is_finite() || ms > u64::MAX as f64 {
        return Err(SimulationError::DurationOverflow);
    }
    Ok(ms.round() as u64)
}

pub fn configured_horizon_ms(start_ms: u64, duration_ms: u64) -> Result<u64, SimulationError> {
    start_ms.checked_add(duration_ms).ok_or(SimulationError::TimestampOverflow)
}
