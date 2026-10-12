# Setting and subway (milestone design)

This note records the megacity re-theme and the subway vertical slice. It is not a
full city redesign. Terminus stays game-agnostic; all fiction and transit live in
`coffee-world`.

## Setting decisions

| Term | Meaning |
| --- | --- |
| **Hub** | The inhabited central district. The Coffee Shop, Neon Alley, Exchange, and related hand-authored rooms sit inside it. |
| **Coffee Shop** | A specific Hub establishment and the default spawn. Not renamed to Hub. |
| **Squalor** | The abandoned urban wilderness beyond the Roof Array fence. Formerly presented as “the outskirts.” |
| **X Disaster** | The catastrophe that emptied most of the city. Established as a fact; not explained. |
| **Undernet** | Retained as the world’s network / product name (`world_name`). It is the surviving civic fabric, not a separate plane. |

Procedural regions keep generation version `1` and the origin fingerprint
`0x8812_F1DB_241C_39A1`. Terrain enum values (`grass`, `forest`, …) stay as
persistence tokens. Only player-facing labels and prose are urbanised.

The delisted-floor mystery, survey stone, archive seal, waypoints, journals, and
shared theories are unchanged in mechanics. Copy is adjusted only where it
conflicted with the urban setting.

## What is retained unchanged

- Terminus crates and the Game / Hub / Store transaction model
- Place keys (`static id`, `w:…`, `i:…`)
- Region birth identity, epoch 1 seed, generator thresholds
- City-gate seam at birth `(0,0)` tile `(8,8)`
- Access controls (archive cipher plate, cabin policies, private journals)

## Subway data model

Stations are **static places** (same table and seeding path as Undernet rooms).
Surface entrances are **authored wilderness seams**, like the city gate: they do
not rewrite the generator baseline or the fingerprint.

```text
Surface tile (WildernessPos)  --down/metro-->  Station (static id)
Station                       --up/surface-->  Surface tile
Station A  --line / east-->  Station B   (one hop ≈ 8 surface tiles)
```

Stable identities:

| Id | Role | Surface absolute tile |
| --- | --- | --- |
| `metro-ash-plaza` | Station under the Hub-side entrance | `(2, 8)` in region `(0, 0)` |
| `metro-ward-west` | Station eight tiles west | `(-6, 8)` → region `(-1, 0)` local `(10, 8)` |

Surface distance between entrances is exactly **8 cardinal tiles** (one axis).
One subway hop between those stations is one movement unit that covers that
span. Diagonals are not used; approximation is “8 orthogonal surface tiles per
subway hop,” matching the square grid.

Entrances and station-to-surface links are resolved in code (wilderness keys
cannot live in the static `exits` FK table). Station-to-station links are seeded
static exits so they reseed like other Undernet doors and survive restart
without a new table.

Ordinary routes are deterministic. No random relocation. Future anomalous
connections can be additional authored exits or `world_connections` rows with
stable ids; this slice does not add an anomaly engine.

## Trade-offs

- **Chosen:** static stations + surface seams. Reuses movement, inspect, map,
  waypoints, and seeding. No schema migration.
- **Rejected:** separate subway coordinate space. Would duplicate travel and
  place presentation for little gain at this scale.
- **Rejected:** multiplying player coordinates by eight. That teleports without
  topology and breaks region accounting.
- **Limitation:** only two stations. A full metro remains a later milestone;
  add stations the same way (entrance seam + static room + seeded hop).

## Adding a future station

1. Pick a walkable-or-seamed surface tile and document its absolute coordinates.
2. Add a `LocationDef` and inspect objects in `subway.rs`.
3. Register the entrance in `ENTRANCES` with matching up/down keywords.
4. Seed a hop exit whose surface span is a multiple of eight when that is the
   intended scale.
5. Extend tests for distance, restart, and access (subway must not open the
   Black Archive or other sealed rooms).
