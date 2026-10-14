# Traversal, trailblazing, and player traces

Two channels carry orientation knowledge:

| Channel | Lifetime | Where | Use for |
|---------|----------|-------|---------|
| **Tile traces** | Chalk (TTL) or costly sign/trail | `world_structures` on a wilderness tile | Fork warnings, short notes |
| **Archive** | Long | Coffee Shop `board`, `theory`, journal | Surveys, route write-ups |

## Near-hub policy (Chebyshev ≤ 20 from the gate)

- **No** permanent `sign`, `trail`, or `mark` on tiles in this band (gate tile included).
- **Chalk** allowed: one per tile, 7-day TTL (refresh with `chalk refresh`).
- Procedural starter threads and urban traces remain; player sign spam is blocked.

## Fringe and deep

- Signs: up to **3** active per operator per region.
- Trail posts: **1** active per operator per region.
- Chalk TTL: **14 days**.
- **Look** shows at most **2** foreign trace lines per tile; extra marks are omitted from prose until TTL/cleanup.

## Commands

- `listen` — band label, distance from gate, metro stair hint, nearest eligible trace (no clue spoilers).
- `chalk <message>` / `chalk refresh` — ephemeral marks.
- `traces` — list your active signs, trails, and chalk in the current region.
- `survey status` — private aggregate (journal count, clues seen, place key, theories).
- `survey publish <text>` — posts to the Coffee Shop noticeboard (durable archive).

## Atmosphere and map

- **Atmosphere** is derived from world seed, region, and day bucket (no extra DB state).
- Low clarity adds orientation hints on `look` and may fog non-adjacent tiles on wilderness `map` (`?` symbols).

## Simulator

Run a 10-Newcomer burst (shared seed) to measure near-hub sign rejections:

```bash
cargo run -p coffee-sim -- newcomer-burst --agents 10 --seed 42
```

See also [thread discovery experiment](thread-discovery-experiment.md) for clue metrics.
