//! CLI for the Coffee.shop population simulator.

use std::process::ExitCode;

use clap::Parser;
use coffee_sim::runner::RunConfig;
use coffee_sim::scheduler::SchedulerConfig;
use coffee_sim::SimulationRunner;

#[derive(Parser)]
#[command(
    name = "coffee-sim",
    about = "Deterministic population simulator for Coffee.shop (offline, no SSH/TUI)"
)]
struct Cli {
    /// Number of synthetic agents (1–256).
    #[arg(long, default_value_t = 8)]
    players: u32,

    /// Virtual days to simulate from the starting timestamp.
    #[arg(long, default_value_t = 7.0)]
    days: f64,

    /// Random seed (action choices and scheduling jitter).
    #[arg(long, default_value = "42")]
    seed: u64,

    /// Virtual milliseconds between consecutive actions for each agent.
    #[arg(long, default_value_t = 60 * 60 * 1000)]
    action_interval_ms: u64,

    /// Optional cap on total attempted commands (0 = unlimited).
    #[arg(long, default_value_t = 0)]
    max_actions: u64,

    /// Virtual start timestamp in milliseconds since Unix epoch.
    #[arg(long, default_value_t = 1_700_000_000_000u64)]
    start_ms: u64,

    /// Record a bounded action trace in the JSON report.
    #[arg(long)]
    trace: bool,

    /// Emit JSON instead of human-readable text.
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let config = RunConfig {
        seed: cli.seed,
        agent_count: cli.players,
        virtual_days: cli.days,
        scheduler: SchedulerConfig {
            action_interval_ms: cli.action_interval_ms,
            idle_extra_ms: 6 * 60 * 60 * 1000,
        },
        max_actions: if cli.max_actions == 0 { None } else { Some(cli.max_actions) },
        virtual_start_ms: cli.start_ms,
        record_action_trace: cli.trace,
        ..RunConfig::default()
    };

    match SimulationRunner::new(config).and_then(|runner| runner.run()) {
        Ok(report) => {
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report).expect("serialize report"));
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
