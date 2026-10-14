//! Ten concurrent Newcomers on one seed — near-hub UGC policy smoke test.

use coffee_world::UgcStructureAudit;
use serde::{Deserialize, Serialize};

use crate::population::PopulationMix;
use crate::runner::{run_with_config_and_world, RunConfig};
use crate::SimulationError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewcomerBurstReport {
    pub seed: u64,
    pub agents: u32,
    pub near_hub_signs: u32,
    pub near_hub_trails: u32,
    pub near_hub_markers: u32,
    pub near_hub_chalk: u32,
    pub fringe_or_deep_signs: u32,
}

impl From<(u64, u32, UgcStructureAudit)> for NewcomerBurstReport {
    fn from((seed, agents, audit): (u64, u32, UgcStructureAudit)) -> Self {
        Self {
            seed,
            agents,
            near_hub_signs: audit.near_hub_signs,
            near_hub_trails: audit.near_hub_trails,
            near_hub_markers: audit.near_hub_markers,
            near_hub_chalk: audit.near_hub_chalk,
            fringe_or_deep_signs: audit.fringe_or_deep_signs,
        }
    }
}

pub fn run_newcomer_burst(seed: u64, agents: u32) -> Result<NewcomerBurstReport, SimulationError> {
    let config = RunConfig {
        seed,
        agent_count: agents,
        population: PopulationMix::monolithic_newcomer(),
        virtual_days: 2.0,
        max_actions: Some(400),
        include_coverage_metrics: false,
        ..RunConfig::default()
    };
    let (_report, world) = run_with_config_and_world(config)?;
    Ok(NewcomerBurstReport::from((seed, agents, world.simulation_ugc_structure_audit())))
}

pub fn format_newcomer_burst(report: &NewcomerBurstReport) -> String {
    format!(
        "Newcomer burst (seed {}, {} agents)\n\
         Near-hub signs/trails/markers: {}/{}/{} (expect 0/0/0)\n\
         Near-hub chalk: {}\n\
         Fringe+ deep signs: {}",
        report.seed,
        report.agents,
        report.near_hub_signs,
        report.near_hub_trails,
        report.near_hub_markers,
        report.near_hub_chalk,
        report.fringe_or_deep_signs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newcomer_burst_has_no_near_hub_permanent_traces() {
        let report = run_newcomer_burst(42, 10).expect("burst");
        assert_eq!(report.near_hub_signs, 0);
        assert_eq!(report.near_hub_trails, 0);
        assert_eq!(report.near_hub_markers, 0);
    }
}
