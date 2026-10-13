# Population simulation (Phase 1)

`coffee-sim` is an offline harness that drives synthetic players through the same
[`WorldState`](../crates/coffee-world/src/state/mod.rs) command path used by the
live server. It does not open SSH, render a TUI, or write to the production SQLite
database.

## Build and run

```bash
cargo run -p coffee-sim -- --players 10 --days 7 --seed 42
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `--players` | Synthetic agent count (1–256) |
| `--days` | Virtual duration in days (must be finite and positive) |
| `--seed` | RNG seed for action choices (default `42`) |
| `--action-interval-ms` | Virtual gap between each agent's actions (default 1 hour, minimum 1) |
| `--max-actions` | Stop after N attempted commands (`0` = unlimited) |
| `--start-ms` | Virtual epoch start (milliseconds since Unix epoch) |
| `--trace` | Include a bounded action trace in JSON output |
| `--json` | Print the report as JSON |

Reproducing a run: record `--seed`, `--players`, `--days`, `--action-interval-ms`,
`--start-ms`, `--max-actions`, and `--trace`. Equivalent settings produce equivalent
gameplay outcomes and virtual event histories. Wall-clock duration is excluded from
reproducibility comparisons.

## Virtual time policy

Simulations use [`WorldState::for_simulation`](../crates/coffee-world/src/sim.rs),
which disables the wall clock.

1. **Registration** — Each `connect_player` advances the test clock by 1 ms (same
   as production test mode). After all agents connect,
   [`align_simulation_clock`](../crates/coffee-world/src/sim.rs) resets `now_ms` to
   the configured start so scheduling is not skewed by agent count.

2. **Scheduled events** — Before each action, the runner advances the world clock
   to the event **slot time** (never backward). The command runs through
   `WorldState::apply`, which adds **+1 ms** for its tick. Gameplay therefore
   observes `slot_ms + 1` during handler execution.

3. **Between actions** — Larger gaps use [`advance_virtual_time`](../crates/coffee-world/src/sim.rs).

4. **End of run** — The world clock stays at the last processed command. It is **not**
   fast-forwarded to the configured horizon unless an event was scheduled there.

### Event ordering

The scheduler is a min-heap ordered by:

1. `time_ms` ascending  
2. `agent_index` ascending  
3. `sequence` ascending (FIFO for ties)

## Termination and report timing

The report separates:

| Field | Meaning |
|-------|---------|
| `config.virtual_start_ms` | Configured epoch |
| `config.virtual_duration_ms` | Configured span |
| `timing.configured_horizon_ms` | `start + duration` (scheduling window) |
| `timing.last_event_slot_ms` | Last processed scheduler slot |
| `timing.final_world_ms` | Authoritative `now_ms` after the last command |
| `timing.events_processed` | Scheduler events handled (includes idle slots) |
| `timing.stop_reason` | `queue_exhausted`, `horizon_reached`, or `action_limit` |

A one-day configured duration does **not** imply the last action occurred at the end
of that day unless events were actually scheduled and processed there.

## Metrics

| Metric | Definition |
|--------|------------|
| `unique_locations_visited` | Distinct place keys after a **successful** move that changed location |
| `unique_locations_occupied` | Distinct place keys where agents stand at end state |
| `regions_materialized` | Regions in geography state |
| `discoveries_recorded` | Discovery rows (not the same as visits) |
| `inspect_attempts` / `successful_inspections` | Commands starting with `inspect` |
| `exploration_move_attempts` / `successful_exploration_moves` | Cardinal exit words |
| `home_claims_active` | Active personal homes in world state |

Rejected commands never increment successful counters or visits.

## Game pathways exercised

Phase 1 agents share a short hub script (`look`, `go up`, `outskirts`, `north`),
then choose wilderness actions (cardinal moves, `inspect`, optional `home claim`)
from a seeded RNG. All commands go through [`WorldState::apply`](../crates/coffee-world/src/state/mod.rs)
with normal validation and rollback semantics.

## Performance note

Agents are stored in a vector indexed by `agent_index` (O(1) lookup per event).
The scheduler heap is O(log n) per schedule/pop; total work scales with events
processed.

## Not implemented yet

- SQLite persistence / reload during sim runs
- LLM or archetype behaviours
- SSH load testing or concurrent sessions
- Live production world access
- Parallel or distributed simulation

Determinism applies to this scheduler and action-selection model only; it does not
predict real human play patterns or maximum concurrent SSH capacity.
