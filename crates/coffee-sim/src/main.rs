//! CLI for the Coffee.shop population simulator.

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use coffee_sim::compare::{compare_populations, ComparisonRequest};
use coffee_sim::population::PopulationMix;
use coffee_sim::runner::RunConfig;
use coffee_sim::scheduler::SchedulerConfig;
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
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum PresetMix {
    Exploration,
    Home,
    Uniform,
}

impl PresetMix {
    fn population(self) -> PopulationMix {
        match self {
            Self::Exploration => PopulationMix::exploration_heavy(),
            Self::Home => PopulationMix::home_oriented(),
            Self::Uniform => PopulationMix::uniform_wanderer(),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Some(Commands::Compare { mix_a, mix_b }) = cli.command {
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
    } else {
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
        ..RunConfig::default()
    }
}
