# Population simulation (Phase 1–2)

`coffee-sim` is an offline harness that drives synthetic players through the same
[`WorldState`](../crates/coffee-world/src/state/mod.rs) command path used by the
live server. It does not open SSH, render a TUI, or write to the production SQLite
database.

Phase 2 adds **behavioural profiles** (weighted action preferences), **population
mix configuration**, **profile-aware reporting**, and a **compare** command for two
mixes under identical seeds and horizons.

## Build and run

Single run (default: 100% wanderer weight):

```bash
cargo run -p coffee-sim -- --players 10 --days 7 --seed 42
```

Mixed population (weights are proportions, not percentages — they are normalized):

```bash
cargo run -p coffee-sim -- \
  --players 20 \
  --days 7 \
  --seed 42 \
  --wanderer-weight 40 \
  --investigator-weight 20 \
  --homebody-weight 15 \
  --social-weight 10 \
  --visitor-weight 15
```

Compare two preset mixes (exploration-heavy vs home-oriented) with the same seed and horizon:

```bash
cargo run -p coffee-sim -- compare \
  --mix-a exploration \
  --mix-b home \
  --players 12 \
  --days 3 \
  --seed 77 \
  --max-actions 150
```

Preset mixes:

| Preset | Weights (wanderer / investigator / homebody / social / visitor) |
|--------|-------------------------------------------------------------------|
| `exploration` | 60 / 30 / 0 / 10 / 0 |
| `home` | 10 / 0 / 50 / 5 / 35 |
| `uniform` | 100 / 0 / 0 / 0 / 0 |

### CLI flags

| Flag | Meaning |
|------|---------|
| `--players` | Synthetic agent count (1–256) |
| `--days` | Virtual duration in days (must be finite and positive) |
| `--seed` | RNG seed for profile shuffle and action choices (default `42`) |
| `--action-interval-ms` | Virtual gap between each agent's actions (default 1 hour, minimum 1) |
| `--max-actions` | Stop after N attempted commands (`0` = unlimited) |
| `--start-ms` | Virtual epoch start (milliseconds since Unix epoch) |
| `--wanderer-weight` … `--visitor-weight` | Profile mix weights (at least one non-zero) |
| `--trace` | Include a bounded action trace in JSON output |
| `--json` | Print the report as JSON |

Reproducing a run: record `--seed`, `--players`, profile weights (or compare presets),
`--days`, `--action-interval-ms`, `--start-ms`, `--max-actions`, and `--trace`.
Equivalent settings produce equivalent gameplay outcomes and virtual event histories.
Wall-clock duration is excluded from reproducibility comparisons (`for_reproducibility_comparison()`).

## Behavioural profiles

Profiles are **tendencies**, not scripts. Each event, the simulator builds an
[`ActionContext`](../crates/coffee-sim/src/navigation.rs) from the real game view
(exits, co-present players, home place key) and picks a weighted command in
[`profile::choose_action`](../crates/coffee-sim/src/profile.rs).

| Profile | Intent | Typical commands |
|---------|--------|------------------|
| Wanderer | Coverage | Cardinal moves, `inspect ground`, `look` |
| Investigator | Depth | `inspect ground`, `journal record`, moves |
| Homebody | Familiar place | `home claim`, local `look` / inspect near home |
| Social player | Co-located chat | `say`, `emote`, `whisper` when others present |
| Occasional visitor | Sparse sessions | Short action bursts, then **offline gaps** (2 virtual days) via the scheduler — no busy-loop waiting |

Simulator-only [`AgentMemory`](../crates/coffee-sim/src/memory.rs) tracks visits,
inspections, and repeat patterns for weighting. It is **not** authoritative game state
and is never shared between agents.

Assignment: [`PopulationMix::allocate`](../crates/coffee-sim/src/population.rs) uses
largest-remainder counting to match `--players`, then a seeded shuffle assigns profiles
to agent indices deterministically.

## Virtual time policy

Simulations use [`WorldState::for_simulation`](../crates/coffee-world/src/sim.rs),
which disables the wall clock.

1. **Registration** — Each `connect_player` advances the test clock by 1 ms. After all
   agents connect, [`align_simulation_clock`](../crates/coffee-world/src/sim.rs) resets
   `now_ms` to the configured start.

2. **Scheduled events** — Before each action, the runner advances the world clock to the
   event **slot time**. Commands run through `WorldState::apply` (+1 ms tick).

3. **Between actions** — [`advance_virtual_time`](../crates/coffee-world/src/sim.rs) and
   Occasional Visitor **offline gaps** (multi-day jumps without attempted commands).

4. **End of run** — The world clock stays at the last processed command (not forced to horizon).

### Event ordering

Min-heap: `time_ms`, then `agent_index`, then `sequence`.

## Termination and report timing

| Field | Meaning |
|-------|---------|
| `config.population` | Profile weights used for this run |
| `timing.configured_horizon_ms` | `start + duration` |
| `timing.inactive_slots` | Scheduler events with no attempted command (offline / idle) |
| `timing.stop_reason` | `queue_exhausted`, `horizon_reached`, or `action_limit` |

## Metrics

### Global / world

| Metric | Definition |
|--------|------------|
| `unique_locations_visited` | Distinct place keys after a **successful** move that changed location |
| `unique_regions_visited` | Union of wilderness region keys from simulator memory |
| `inspect_attempts` / `successful_inspections` | Commands starting with `inspect` |
| `exploration_move_attempts` / `successful_exploration_moves` | Cardinal exit words |
| `home_claims_active` | Active personal homes in world state |
| `journal_records` | Successful `journal record` commands (simulator count) |
| `social_attempts` / `social_successes` | `say` / `emote` / `whisper` prefixes; success only when the engine accepts |
| `repeat_visits` / `home_returns` | Simulator memory tallies |

Rejected commands never increment successful counters or visits.

### Per profile (`per_profile`)

Same counters aggregated per profile name (`wanderer`, `investigator`, …), including
`inactive_slots` for visitors.

## Game pathways and limits

All gameplay goes through [`WorldState::apply`](../crates/coffee-world/src/state/mod.rs).
The simulator does not mutate world state directly or fabricate discoveries.

**Supported social behaviour:** when the game view lists other players in the same
location, Social players may `say` / `emote` / `whisper` through the normal path.
There is no separate social layer; if agents are not co-located, social commands are
simply not preferred.

**Known limitations:**

- No pathfinding “return home”; home returns are counted only when a successful move
  lands on the agent’s active home place key.
- `journal record` only succeeds when the game offers journal context (same as live play).
- Inspect targets use `inspect ground` in wilderness where exits exist; no invented clues.
- Whisper requires a valid co-present name from the game view.

## Compare API

Library entry point:

```rust
use coffee_sim::{compare_populations, ComparisonRequest, PopulationMix, RunConfig};

let report = compare_populations(ComparisonRequest {
    base: RunConfig { seed: 42, agent_count: 10, ..Default::default() },
    mix_a: PopulationMix::exploration_heavy(),
    mix_b: PopulationMix::home_oriented(),
})?;
```

Each run uses an independent in-memory `WorldState`.

## Not implemented

- SQLite persistence during sim runs
- LLM / natural-language agents
- SSH load testing or concurrent sessions
- New player-facing mechanics invented for the simulator
- Parallel or distributed simulation

Determinism applies to this scheduler and profile model only; it does not predict
real human play or server capacity.
