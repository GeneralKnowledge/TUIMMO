//! CLI for the Coffee.shop population simulator.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coffee_sim::compare::{compare_populations, ComparisonRequest};
use coffee_sim::config::{SnapshotConfig, DEFAULT_SNAPSHOT_MAX};
use coffee_sim::experiment::{run_population_scale_experiment, PopulationScaleRequest};
use coffee_sim::multi_seed::{
    run_multi_seed_experiment, ExperimentCondition, MultiSeedExperimentRequest,
};
use coffee_sim::population::PopulationMix;
use coffee_sim::runner::RunConfig;
use coffee_sim::scheduler::SchedulerConfig;
use coffee_sim::seeds::ReplicateSeedSchedule;
use coffee_sim::SimulationRunner;

#[derive(Parser)]
#[command(
    name = "coffee-sim",
    about = "Deterministic population simulator for Coffee.shop (offline, no SSH/TUI)"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Number of synthetic agents (1–256).
    #[arg(long, global = true, default_value_t = 8)]
    players: u32,

    /// Virtual days to simulate from the starting timestamp.
    #[arg(long, global = true, default_value_t = 7.0)]
    days: f64,

    /// Random seed (action choices and scheduling jitter).
    #[arg(long, global = true, default_value = "42")]
    seed: u64,

    /// Virtual milliseconds between consecutive actions for each agent.
    #[arg(long, global = true, default_value_t = 60 * 60 * 1000)]
    action_interval_ms: u64,

    /// Optional cap on total attempted commands (0 = unlimited).
    #[arg(long, global = true, default_value_t = 0)]
    max_actions: u64,

    /// Virtual start timestamp in milliseconds since Unix epoch.
    #[arg(long, global = true, default_value_t = 1_700_000_000_000u64)]
    start_ms: u64,

    /// Profile weight: wanderer (proportion with other --*-weight flags).
    #[arg(long, global = true, default_value_t = 100)]
    wanderer_weight: u32,

    #[arg(long, global = true, default_value_t = 0)]
    investigator_weight: u32,

    #[arg(long, global = true, default_value_t = 0)]
    homebody_weight: u32,

    #[arg(long, global = true, default_value_t = 0)]
    social_weight: u32,

    #[arg(long, global = true, default_value_t = 0)]
    visitor_weight: u32,

    /// Record a bounded action trace in the JSON report.
    #[arg(long, global = true)]
    trace: bool,

    /// Emit JSON instead of human-readable text.
    #[arg(long, global = true)]
    json: bool,

    /// Virtual milliseconds between longitudinal snapshots (0 = disabled).
    #[arg(long, global = true, default_value_t = 0)]
    snapshot_interval_ms: u64,

    /// Maximum longitudinal snapshots stored per run.
    #[arg(long, global = true, default_value_t = DEFAULT_SNAPSHOT_MAX)]
    snapshot_max: u32,
}

#[derive(Subcommand)]
enum Commands {
    /// Run two population mixes under the same seed and duration, then compare metrics.
    Compare {
        #[arg(long, default_value = "exploration")]
        mix_a: PresetMix,
        #[arg(long, default_value = "home")]
        mix_b: PresetMix,
    },
    /// Run the same mix at several population sizes (bounded, independent worlds).
    Scale {
        /// Comma-separated player counts (e.g. `10,25,50`).
        #[arg(long, value_delimiter = ',', default_value = "10,25,50")]
        sizes: Vec<u32>,
    },
    /// Multi-seed experiment: conditions × replicate seeds with aggregate statistics.
    Experiment {
        /// Comma-separated player counts (one condition per size unless `--mixes` is set).
        #[arg(long, value_delimiter = ',', default_value = "10,25")]
        sizes: Vec<u32>,
        /// Number of replicate seeds derived from `--base-seed` (ignored when `--seeds` is set).
        #[arg(long, default_value_t = 3)]
        replicates: u32,
        /// Base seed for replicate derivation (defaults to global `--seed`).
        #[arg(long)]
        base_seed: Option<u64>,
        /// Explicit comma-separated replicate seeds (overrides `--replicates` / `--base-seed`).
        #[arg(long, value_delimiter = ',')]
        seeds: Option<Vec<u64>>,
        /// Optional preset mixes; when set, runs every size × mix combination.
        #[arg(long, value_delimiter = ',')]
        mixes: Option<Vec<PresetMix>>,
        /// Include full per-run reports in JSON (default: compact replicate summaries only).
        #[arg(long)]
        include_runs: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PresetMix {
    Exploration,
    Home,
    Uniform,
    Social,
    Mixed,
}

impl PresetMix {
    fn label(self) -> &'static str {
        match self {
            Self::Exploration => "exploration",
            Self::Home => "home",
            Self::Uniform => "uniform",
            Self::Social => "social",
            Self::Mixed => "mixed",
        }
    }

    fn population(self) -> PopulationMix {
        match self {
            Self::Exploration => PopulationMix::exploration_heavy(),
            Self::Home => PopulationMix::home_oriented(),
            Self::Uniform => PopulationMix::uniform_wanderer(),
            Self::Social => PopulationMix::social_and_visitors(),
            Self::Mixed => PopulationMix::all_profiles_mixed(),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Experiment {
            ref sizes,
            replicates,
            base_seed,
            ref seeds,
            ref mixes,
            include_runs,
        }) => {
            let base = base_config(&cli);
            let seed_schedule = if let Some(explicit) = seeds {
                ReplicateSeedSchedule::Explicit { seeds: explicit.clone() }
            } else {
                ReplicateSeedSchedule::FromBase {
                    base_seed: base_seed.unwrap_or(cli.seed),
                    replicate_count: replicates,
                }
            };
            let conditions = build_experiment_conditions(sizes, mixes, &base);
            match run_multi_seed_experiment(MultiSeedExperimentRequest {
                base,
                conditions,
                seed_schedule,
                include_full_run_reports: include_runs,
            }) {
                Ok(report) => {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).expect("serialize"));
                    } else {
                        println!("{}", report.to_pretty_text());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("experiment failed: {err}");
                    ExitCode::from(1)
                }
            }
        }
        Some(Commands::Scale { ref sizes }) => {
            let base = base_config(&cli);
            match run_population_scale_experiment(PopulationScaleRequest {
                base,
                player_counts: sizes.clone(),
            }) {
                Ok(report) => {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).expect("serialize"));
                    } else {
                        println!("{}", report.to_pretty_text());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("scale experiment failed: {err}");
                    ExitCode::from(1)
                }
            }
        }
        Some(Commands::Compare { mix_a, mix_b }) => {
            let base = base_config(&cli);
            match compare_populations(ComparisonRequest {
                base,
                mix_a: mix_a.population(),
                mix_b: mix_b.population(),
            }) {
                Ok(report) => {
                    if cli.json {
                        println!("{}", serde_json::to_string_pretty(&report).expect("serialize"));
                    } else {
                        println!("{}", report.to_pretty_text());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("comparison failed: {err}");
                    ExitCode::from(1)
                }
            }
        }
        None => {
            let config = base_config(&cli);
            match SimulationRunner::new(config).and_then(|runner| runner.run()) {
                Ok(report) => {
                    if cli.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&report).expect("serialize report")
                        );
                    } else {
                        println!("{}", report.to_pretty_text());
                    }
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("simulation failed: {err}");
                    ExitCode::from(1)
                }
            }
        }
    }
}

fn build_experiment_conditions(
    sizes: &[u32],
    mixes: &Option<Vec<PresetMix>>,
    base: &RunConfig,
) -> Vec<ExperimentCondition> {
    let mut conditions = Vec::new();
    match mixes {
        Some(presets) if !presets.is_empty() => {
            for size in sizes {
                for mix in presets {
                    let population = mix.population();
                    conditions.push(ExperimentCondition {
                        label: format!("N={size}:{}", mix.label()),
                        agent_count: *size,
                        population,
                    });
                }
            }
        }
        _ => {
            for size in sizes {
                conditions.push(ExperimentCondition {
                    label: format!("N={size}"),
                    agent_count: *size,
                    population: base.population.clone(),
                });
            }
        }
    }
    conditions
}

fn base_config(cli: &Cli) -> RunConfig {
    RunConfig {
        seed: cli.seed,
        agent_count: cli.players,
        population: PopulationMix {
            wanderer: cli.wanderer_weight,
            investigator: cli.investigator_weight,
            homebody: cli.homebody_weight,
            social_player: cli.social_weight,
            occasional_visitor: cli.visitor_weight,
        },
        virtual_days: cli.days,
        scheduler: SchedulerConfig {
            action_interval_ms: cli.action_interval_ms,
            idle_extra_ms: 6 * 60 * 60 * 1000,
        },
        max_actions: if cli.max_actions == 0 { None } else { Some(cli.max_actions) },
        virtual_start_ms: cli.start_ms,
        record_action_trace: cli.trace,
        snapshots: SnapshotConfig {
            interval_ms: if cli.snapshot_interval_ms == 0 {
                None
            } else {
                Some(cli.snapshot_interval_ms)
            },
            max_snapshots: cli.snapshot_max,
        },
        ..RunConfig::default()
    }
}
