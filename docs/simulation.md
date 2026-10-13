# Population simulation (Phase 1)

`coffee-sim` is an offline harness that drives synthetic players through the same
[`WorldState`](https://github.com/GeneralKnowledge/TUIMMO/blob/main/crates/coffee-world/src/state/mod.rs)
command path used by the live server. It does not open SSH, render a TUI, or write
to the production SQLite database.

## Build and run

```bash
cargo run -p coffee-sim -- --players 10 --days 7 --seed 42
```

Useful flags:

| Flag | Meaning |
|------|---------|
| `--players` | Synthetic agent count (1–256) |
| `--days` | Virtual duration in days |
| `--seed` | RNG seed for action choices (default `42`) |
| `--action-interval-ms` | Virtual gap between each agent's actions (default 1 hour) |
| `--max-actions` | Stop after N attempted commands (`0` = unlimited) |
| `--start-ms` | Virtual epoch start (milliseconds since Unix epoch) |
| `--json` | Print the report as JSON |

Reproducing a run: record `--seed`, `--players`, `--days`, `--action-interval-ms`,
`--start-ms`, and `--max-actions`. Equivalent settings produce equivalent reports
and gameplay outcomes (real wall-clock duration is excluded).

## Virtual time

Simulations use [`WorldState::for_simulation`](../../crates/coffee-world/src/sim.rs),
which disables the wall clock. The scheduler sets the world timestamp before each
action; [`advance_virtual_time`](../../crates/coffee-world/src/sim.rs) jumps gaps
between events. Each successful `apply` / `connect` still adds one millisecond,
matching test mode. Time-dependent rules (personal home habitation, notices, etc.)
read the same `now_ms` field as production.

## Synthetic identities

Agent [`PlayerId`](../../crates/terminus-core/src/id.rs) and account UUIDs are
deterministic UUID v5 values in a simulation-only namespace. They are created with
[`starter_operator`](../../crates/coffee-world/src/state/mod.rs) and never touch
account registration or the server database.

## Game pathways exercised

Phase 1 agents share a short hub script (`look`, `go up`, `outskirts`, `north`),
then choose wilderness actions (cardinal moves, `inspect`, optional `home claim`)
from a seeded RNG. All commands go through [`WorldState::apply`](../../crates/coffee-world/src/state/mod.rs)
with normal validation and rollback semantics.

## Report

The text report lists configuration, execution totals (attempted / successful /
rejected / failed), and world aggregates (regions, discoveries, visits, homes).
JSON output uses the same schema as [`RunReport`](../../crates/coffee-sim/src/report.rs).

## Not implemented yet

- SQLite persistence / reload during sim runs
- LLM or archetype behaviours
- SSH load testing or concurrent sessions
- Live production world access
- Long-horizon home return experiments beyond basic claim + virtual time

Determinism applies to this scheduler and action-selection model only; it does not
predict real human play patterns or maximum concurrent SSH capacity.
