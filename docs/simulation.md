# Population simulation (Phase 1–2.2)

`coffee-sim` is an offline harness that drives synthetic players through the same
[`WorldState`](../crates/coffee-world/src/state/mod.rs) command path used by the
live server. It does not open SSH, render a TUI, or write to the production SQLite
database.

Phase 2 adds **behavioural profiles** (weighted action preferences), **population
mix configuration**, **profile-aware reporting**, and a **compare** command for two
mixes under identical seeds and horizons.

Phase 2.1 validates that those profiles produce **interpretable behavioural differences**
under controlled conditions, that **report metrics match their definitions**, and that
**population comparisons** are isolated and easier to interpret (including derived rates).

Phase 2.2 adds **longitudinal snapshots** (optional virtual-time sampling), **geographical
occupancy aggregates**, and a bounded **`scale` experiment** command for population-size
comparisons over multi-week horizons — without wall-clock waiting or new game mechanics.

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
| `social` | 0 / 0 / 0 / 50 / 50 |
| `mixed` | 20 / 20 / 20 / 20 / 20 |

Library-only preset for calibration (social + sparse visitors):

| Preset | Weights |
|--------|---------|
| `PopulationMix::social_and_visitors()` | 0 / 0 / 0 / 50 / 50 |

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
| `--snapshot-interval-ms` | Virtual ms between longitudinal snapshots (`0` = disabled) |
| `--snapshot-max` | Maximum snapshots stored per run (default `128`, hard cap `4096`) |
| `--json` | Print the report as JSON |

Long multi-week run with weekly snapshots (JSON):

```bash
cargo run -p coffee-sim -- \
  --players 25 \
  --days 56 \
  --seed 77 \
  --max-actions 400 \
  --snapshot-interval-ms 604800000 \
  --json > long_run.json
```

Population-size experiment (same mix, independent worlds, max 6 sizes):

```bash
cargo run -p coffee-sim -- scale \
  --sizes 10,25,50 \
  --days 28 \
  --seed 88 \
  --wanderer-weight 60 --investigator-weight 30 --social-weight 10 \
  --snapshot-interval-ms 604800000 \
  --json > scale_explore.json
```

`scale` reuses global `--*-weight` flags for the mix (defaults remain wanderer-only unless changed). Each size runs in a **separate** world with the same seed and horizon.

Multi-seed experiment (conditions × replicate seeds, aggregate statistics):

```bash
cargo run -p coffee-sim -- experiment \
  --sizes 10,25 \
  --replicates 5 \
  --base-seed 1000 \
  --days 7 \
  --max-actions 80 \
  --wanderer-weight 60 --investigator-weight 30 --social-weight 10 \
  --json > experiment_explore.json
```

Explicit seeds (overrides `--replicates` / `--base-seed`):

```bash
cargo run -p coffee-sim -- experiment \
  --sizes 10 \
  --seeds 100,101,102 \
  --days 5 \
  --max-actions 60 \
  --json
```

Compare profile mixes at the same sizes (each size × mix becomes one **condition**):

```bash
cargo run -p coffee-sim -- experiment \
  --sizes 10,25 \
  --mixes exploration,home \
  --replicates 8 \
  --base-seed 2000 \
  --days 14 \
  --max-actions 120 \
  --json
```

Reproducing a run: record `--seed`, `--players`, profile weights (or compare presets),
`--days`, `--action-interval-ms`, `--start-ms`, `--max-actions`, `--trace`, and snapshot settings.
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

### Observable behavioural indicators (Phase 2.1)

These are **proxies** measured from command outcomes and simulator memory, not claims
about real player psychology.

| Profile | What to inspect in reports | Notes |
|---------|---------------------------|--------|
| Wanderer | `successful_exploration_moves`, `unique_locations_visited`, `unique_regions_visited` | Preference for movement; success requires engine acceptance |
| Investigator | `inspect_attempts` / `successful_inspections`, `journal_records` | Journals only when the game offers journal context (same as live play) |
| Homebody | `home_claims_active`, `repeat_visits`, `home_returns` | Home claim ≠ emotional attachment; returns counted only on successful move onto home place key |
| Social player | `social_attempts` / `social_successes` | Success = engine accepted command; not a relationship metric |
| Occasional visitor | `timing.inactive_slots`, `derived_rates.inactive_slot_share`, lower `total_attempted` vs active profiles | Offline gaps schedule slots **without** attempted commands |

Validation tests live in [`profile_validation.rs`](../crates/coffee-sim/tests/profile_validation.rs)
(monolithic mixes, seed `77`, 4 agents, 7 virtual days, 200 action cap unless noted).

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

Min-heap: `time_ms`, then event kind (observations before agent actions when snapshots are
enabled), then `agent_index`, then `sequence`.

## Termination and report timing

| Field | Meaning |
|-------|---------|
| `config.population` | Profile weights used for this run |
| `timing.configured_horizon_ms` | `start + duration` (scheduling window upper bound; not forced onto the world clock) |
| `timing.last_event_slot_ms` | Last processed **gameplay** (agent action) scheduler slot |
| `timing.last_scheduler_slot_ms` | Last processed scheduler event of any kind (gameplay or observation) |
| `timing.effective_termination_slot_ms` | Gameplay-effective stop slot (same as `last_event_slot_ms` when gameplay ended the run) |
| `timing.final_world_ms` | Authoritative world clock when the run loop exits (after the last gameplay command tick; **not** advanced by discarded post-termination observations) |
| `timing.inactive_slots` | Scheduler events with no attempted command (offline / idle / missing context) |
| `timing.stop_reason` | `queue_exhausted`, `horizon_reached`, or `action_limit` |

### Effective termination (Phase 2.2.2)

Observations measure the simulation; they must not keep it running.

- **Action limit** — After the configured number of attempted commands is reached, the loop
  stops immediately. Pending observation events (including grid boundaries after the last
  gameplay slot) are **not** processed and do not advance `final_world_ms`.
- **Queue exhausted** — When the last gameplay event has been processed and no agent actions
  remain in the heap, later grid observations are **not** processed (clock frozen at the last
  command state).
- **Horizon** — The next event with `time_ms > configured_horizon_ms` is not processed;
  `stop_reason` becomes `horizon_reached`. Observations at or below the horizon still follow
  the observation-before-gameplay ordering rule.

Gameplay RNG, scheduling, and command outcomes are identical with snapshots enabled or disabled.
Only observation-specific fields (`longitudinal_snapshots`, `snapshot_sampling`,
`timing.observation_events_processed`, and usually `timing.last_scheduler_slot_ms`) differ.

## Metrics

### Global / world

| Metric | Definition |
|--------|------------|
| `unique_locations_visited` | Distinct place keys after a **successful** move that **changed** location |
| `unique_regions_visited` | Union of wilderness region keys from simulator memory (all agents) |
| `unique_locations_occupied` | Distinct place keys where agents stand at end of run (world snapshot) |
| `inspect_attempts` / `successful_inspections` | Commands starting with `inspect`; success only when `WorldState::apply` succeeds |
| `exploration_move_attempts` / `successful_exploration_moves` | Cardinal exit words; success when apply succeeds (location may still be unchanged if move fails silently — success counter follows apply outcome) |
| `home_claims_active` | Active personal homes in world state at end of run |
| `journal_records` | Successful `journal record` commands (simulator count) |
| `social_attempts` / `social_successes` | `say` / `emote` / `whisper` prefixes; success only when the engine accepts |
| `repeat_visits` / `home_returns` | Simulator memory tallies (repeat = known location revisited; home return = successful move onto own home key) |

**Accounting rules (Phase 2.1):**

- Rejected or failed commands never increment successful counters, visits, or journal/social success.
- Failed moves do not add to `unique_locations_visited`.
- Inactive scheduler slots (empty command, offline gap, or missing action context) do **not** increment `execution.total_attempted`.
- Per-profile additive counters (`journal_records`, `social_*`, inspections, repeats, inactive slots) sum to world totals where applicable.
- `per_profile.unique_locations_visited` is the **union** of agent memories for that profile, not a sum — it can differ from the global move-based unique count.

### Per agent (`per_agent`)

Same successful-outcome counters as above for the assigned profile only. `unique_locations_visited`
on an agent is the size of that agent's visited-location memory (updated on successful commands),
which can differ slightly from the global move-only unique count.

### Per profile (`per_profile`)

Same counters aggregated per profile name (`wanderer`, `investigator`, …), including
`social_attempts` and `inactive_slots`.

### Derived rates (`RunReport::derived_rates()` / compare JSON `derived_a` / `derived_b`)

| Field | Numerator / denominator |
|-------|-------------------------|
| `exploration_move_success_rate` | successful exploration moves / attempts |
| `inspect_success_rate` | successful inspections / inspect attempts |
| `social_success_rate` | social successes / social attempts |
| `unique_locations_per_agent` | world unique locations visited / agent count |
| `attempted_commands_per_agent` | total attempted / agent count |
| `inactive_slot_share` | inactive slots / events processed |

When the denominator is zero, the rate is JSON `null` (text compare prints `(undefined)`).
This avoids treating “no opportunities” as a zero success rate.

## Longitudinal snapshots (Phase 2.2 / 2.2.1)

Enable with `--snapshot-interval-ms` > 0. JSON reports include `longitudinal_snapshots`
and `snapshot_sampling` (both omitted when disabled). Sampling is **read-only**: observation
events advance the world clock only, never execute commands, consume gameplay RNG, or change
which agent actions occur.

### When a snapshot is taken

1. **Initial** — Immediately after bootstrap at `virtual_start_ms`, before any gameplay
   scheduler event (`kind: initial`).
2. **Interval grid** — At each scheduled boundary `start + n × interval` that fits inside
   the configured horizon. Boundaries are enqueued as **observation events** on the same
   min-heap as gameplay. The runner processes an observation only when virtual time actually
   reaches that boundary; it never copies later world state into an earlier timestamp.
3. **Terminal** — After gameplay finishes, if storage capacity remains and the last processed
   gameplay slot is not already recorded, one row at that slot (`kind: terminal`).

If `max_snapshots` is too small to store every in-horizon grid point, extra grid boundaries
are **skipped** (not fabricated). The count appears in
`snapshot_sampling.grid_boundaries_skipped_capacity`.

Capacity accounting: one slot for the initial row, up to one reserved slot for a possible
terminal row, remaining slots for interval grid observations. The store never exceeds
`max_snapshots`.

### Event ordering (gameplay vs observation)

Heap order: ascending `time_ms`, then **observation before agent action**, then ascending
`agent_index`, then `sequence`.

At equal virtual time, the observation runs first (world clock advanced, counters and
placement recorded), then gameplay events for that timestamp. Observation events do not
increment `timing.events_processed` or `timing.inactive_slots`; they increment
`timing.observation_events_processed` instead.

When no agent is scheduled exactly on a grid boundary, the observation event still runs at
that boundary (clock advance only).

### Snapshot fields

| Field | Meaning |
|-------|---------|
| `observation_ms` | Virtual time this row represents (grid boundary, start, or last gameplay slot for terminal) |
| `elapsed_virtual_ms` | `observation_ms - configured_start` |
| `world_clock_ms` | Authoritative world clock after processing through this observation (matches `observation_ms` once the clock has caught up) |
| `kind` | `initial`, `interval`, or `terminal` |
| `cumulative.*` | Gameplay counters **since run start** through this observation (not including later gameplay at the same timestamp) |
| `world_state.*` | **Current** world counts at this observation |
| `occupancy.*` | **Current** agent placement aggregates from [`simulation_occupancy_summary`](../crates/coffee-world/src/sim.rs) |

**Distinctions:**

- **Visited (move-based)** — in `cumulative.unique_locations_visited_move_based`; requires successful moves that changed location. Not inferred from region generation.
- **Explored regions** — `cumulative.unique_wilderness_regions_explored` from simulator memory (regions reached via qualifying activity).
- **Materialized regions** — `world_state.regions_materialized` from engine geography state (includes on-demand generation).
- **Occupied locations** — distinct place keys with ≥1 agent now (`world_state.unique_locations_occupied` / `occupancy.occupied_location_count`).
- **Hub vs Squalor (occupancy)** — Hub building = `coffee-shop`, `roof-array`, `roof-array` gateway room; wilderness bands use engine `UrbanContext` density (`near_hub`, `fringe`, `deep_squalor`).

`snapshot_sampling.terminal_included` is `true` only when a distinct terminal row was appended.
`snapshot_sampling.terminal_status` is one of:

| Status | Meaning |
|--------|---------|
| `included` | Terminal row stored |
| `duplicate_last_observation` | Final gameplay slot already captured (interval/initial) |
| `omitted_capacity` | `max_snapshots` exhausted |
| `no_gameplay_events` | No agent actions ran |

Terminal rows use `observation_ms = effective_termination_slot_ms` and `world_clock_ms =
timing.final_world_ms` (the clock after the last command, typically one tick after the slot).
Cumulative metrics on a terminal row match the run report when included; interval rows at
earlier times may legitimately show lower totals.

`config.allocated_profile_counts` records actual profile allocation after largest-remainder rounding.

## Game pathways and limits

All gameplay goes through [`WorldState::apply`](../crates/coffee-world/src/state/mod.rs).
The simulator does not mutate world state directly or fabricate discoveries.

**Supported social behaviour:** when the game view lists other players in the same
location, Social players may `say` / `emote` / `whisper` through the normal path.
There is no separate social layer; if agents are not co-located, social commands are
simply not preferred (low weight), which is **missing opportunity**, not a failed attempt.

**Known limitations:**

- No pathfinding “return home”; home returns are counted only when a successful move
  lands on the agent’s active home place key.
- `journal record` only succeeds when the game offers journal context (same as live play).
- Inspect targets use `inspect ground` in wilderness where exits exist; no invented clues.
- Whisper requires a valid co-present name from the game view.
- The simulator is not a validated model of human psychology; reproducibility ≠ realism.

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

Each run uses an **independent** in-memory `WorldState`, RNG stream (same seed, separate runs),
and agent memory. Only `mix_a` / `mix_b` profile weights should differ unless you intentionally
change other `RunConfig` fields.

**What comparison establishes:** under identical horizon, seed, player count, and scheduler
settings, differences in metrics reflect profile mix and resulting world interactions.

**What it does not establish:** real-world player counts, long-term friendships, mystery solve
rates, server capacity, or months-long world evolution without further validation.

JSON compare output adds `derived_a` and `derived_b` (backwards compatible: existing delta
fields unchanged).

## Calibration workflow (baseline experiment)

Use the simulator as a **hypothesis-driven instrument**, not a prediction engine.

### 1. State a hypothesis

Example: “A home-oriented mix produces more active home claims but fewer distinct places
visited per agent than an exploration-heavy mix, even when total attempted commands differ
because of visitor inactive slots.”

### 2. Hold settings constant

Fix `--players`, `--days`, `--seed`, `--action-interval-ms`, `--start-ms`, and `--max-actions`.
Change only profile weights (or compare presets).

### 3. Record configuration

Save the full CLI or `RunConfig`, including weights and seed.

### 4. Run three baseline comparisons

```bash
# A vs B: exploration-heavy vs home-oriented (built-in presets)
cargo run -p coffee-sim -- compare --mix-a exploration --mix-b home \
  --players 4 --days 7 --seed 77 --max-actions 200 --json > compare_explore_home.json

# Single-run social + visitor mix (library preset or manual weights)
cargo run -p coffee-sim -- --players 4 --days 7 --seed 77 --max-actions 200 \
  --wanderer-weight 0 --investigator-weight 0 --homebody-weight 0 \
  --social-weight 50 --visitor-weight 50 --json > run_social_visitor.json
```

### 5. Read global, profile, and derived fields

- Global: `world.*`, `execution.total_attempted`, `timing.inactive_slots`
- Per profile: `per_profile.*`
- Rates: `derived_rates()` or compare `derived_a` / `derived_b`

### 6. Separate volume from behaviour

Compare `attempted_commands_per_agent` and `inactive_slot_share` before interpreting raw
totals. A lower unique-location count may reflect fewer attempts or more offline time, not
weaker wander preference.

### 7. Sensitivity runs (multi-seed experiments)

Use `experiment` with `--replicates` or explicit `--seeds`. The same seed schedule is reused
for every condition (population size / mix), so cross-condition comparisons are easier to
interpret. Changing population size changes agent allocation and interactions even when the
replicate seed matches — that is expected simulator behaviour, not a configuration bug.

Treat spread across replicates as **descriptive variability** under this behavioural model,
not evidence about real human players. A handful of replicates helps debugging; stable
estimates usually need more (within workload limits).

## Multi-seed experiments (Phase 2.3)

Hierarchy:

| Level | Meaning |
|-------|---------|
| **Experiment** | Shared horizon, scheduler, action limit, snapshot settings |
| **Condition** | One `(agent_count, profile mix)` pair |
| **Replicate** | One independent `RunConfig` with a recorded seed |
| **Aggregate** | Mean / SD / min / max / median (and rate summaries) across replicates |

### Seed schedules

- **`explicit`** — JSON `seed_schedule.mode = "explicit"` with exact `seeds` list.
- **`from_base`** — Seeds derived by [`replicate_seed`](../crates/coffee-sim/src/seeds.rs)
  (`base_seed`, replicate index). Documented constant stride; no wall-clock or execution-order
  dependence.

Every replicate row records the **actual seed used**. Re-running with identical JSON/API
settings yields identical aggregates. Duplicate explicit seeds are allowed (identical
replicates).

### Workload limits

| Constant | Default cap |
|----------|-------------|
| Conditions | 12 |
| Replicates per condition | 32 |
| Total runs (`conditions × replicates`) | 96 |

Invalid workloads are rejected **before** any simulation runs.

### Aggregates and undefined values

- Numeric metrics: **sample** standard deviation (Bessel `n-1`) when `n ≥ 2`; `null` when
  undefined.
- **Rates:** per-run rates summarized in `distribution`; **pooled** numerators/denominators
  reported separately as `pooled_rate` (not interchangeable with the mean of per-run rates).
- Undefined rates (zero denominator on a run) are excluded from rate distributions
  (`n_undefined` counted). Genuine zero numerators remain valid measurements.
- Missing snapshot rows are not fabricated; time-series points include `replicate_count`
  for how many runs contributed at each `(elapsed_virtual_ms, kind)`.

### Terminology (timing vs snapshots)

See Phase 2.2.2 termination section for `effective_termination_slot_ms` and terminal snapshot
rules. Multi-seed snapshot series treat **terminal** observations separately from **interval**
grid rows (aligned by elapsed virtual time, not array index).

### Recommended protocol

1. Fix virtual horizon and `--max-actions`.
2. Choose several population sizes (and optional `--mixes`).
3. Use the same replicate seed schedule for all conditions (`--base-seed` or `--seeds`).
4. Compare absolute totals and per-agent metrics plus aggregate SD / IQR.
5. Enable snapshots only when longitudinal summaries are needed (gameplay unchanged).
6. Repeat with a second mix to see whether patterns depend on the behavioural model.

Library entry point: [`run_multi_seed_experiment`](../crates/coffee-sim/src/multi_seed.rs).

### 8. Avoid single-run conclusions

Do not generalize from one lucky or empty journal run; check denominators (e.g. zero social
attempts → undefined social rate).

## Validation suite

```bash
cargo test -p coffee-sim
```

- `sim_harness.rs` — scheduling, reproducibility, metric reconciliation, engine path smoke tests
- `profile_validation.rs` — profile separation, mixed populations, compare isolation, derived rates
- `longitudinal_validation.rs` — snapshot ordering, terminal reconciliation, scale experiments, long horizons
- `multi_seed_experiment.rs` — seed schedules, aggregates, workload limits, snapshot series alignment

## Not implemented

- SQLite persistence during sim runs
- LLM / natural-language agents
- SSH load testing or concurrent sessions
- New player-facing mechanics invented for the simulator
- Parallel or distributed simulation
- Statistical analysis framework beyond deterministic regression tests

Determinism applies to this scheduler and profile model only; it does not predict
real human play or server capacity.
