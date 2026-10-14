# Thread discovery experiment report

This document separates **static coverage** (what the generator can place) from **simulated discovery** (what Curious/Newcomer agents find). Static numbers do not prove a human will notice a clue; they bound what exists in the world.

## Commands

```bash
# Static audit (authored + procedural placement, no agents)
cargo run -p coffee-sim -- thread-audit --world-seed 42

# Multi-seed discovery benchmark (profiles + placement inventory)
cargo run -p coffee-sim -- discovery-benchmark --agents 8

# Repeatability tests
cargo test -p coffee-sim --test discovery_determinism
cargo test -p coffee-world -- discovery_journey
```

Production defaults unchanged: `COVERAGE_GRID_STEP=4`, `INSPECTABLE_MAX_GAP=4`.

## Metric definitions

| Metric | Meaning |
|--------|---------|
| **configured_clues** | Clues listed in `threads.toml` or procedural schedule |
| **placed_clues** | Resolved `ClueSite` for authored; materialized sites for procedural |
| **distinct_reachable_locations** | Unique walkable tiles or static locations |
| **thread_clues_inspected** | `clue_seen` rows for thread clues (not `coverage` flavor) after a sim run |
| **clue_to_clue_same_thread_transitions** | Same operator discovers a second clue in a thread they already started |
| **threads_reaching_configured_final** | Operators who saw the thread’s configured final clue id |
| **actions_to_first_thread_clue** | Sim action attempts before first thread `clue_seen` |

## Static coverage (epoch world seed, `thread-audit`)

Authored threads (7): all clues place on walkable tiles or known static rooms; no invalid placements in shipped data.

| Thread | Clues | Reachable sites | Gated reveals | Playable next step |
|--------|-------|-----------------|---------------|-------------------|
| the-tally | 3 | 3 | 4 | yes |
| warm-doors | 4 | 4 | 4 | yes |
| second-names | 4 | 4 | 4 | yes |
| wrong-weather | 4 | 4 | 5 | yes |
| long-way | 4 | 4 | 5 | yes |
| off-schedule | 3 | 3 | 3 | yes |
| two-maps | 3 | 3 | 3 | yes |

Procedural (gate origin sample unless noted):

| Template | Notes |
|----------|--------|
| **starter-walk** | 2–4 clues within Chebyshev 20 of Roof Array gate; seed-varied text |
| **residue** | Per-region instance at origin in sample |
| **absent-keeper** | Spawn-modulus gated; may be absent at origin |
| **long-procession** | 21-clue spine across 21 regions; consecutive cheb step mean ~450, max ~1552 |

## Simulated discovery (5 seeds, 8 agents)

Latest benchmark after Newcomer hub path fix (`go squalor` from Roof Array outskirts):

| Profile | Share ≥1 thread clue | Median actions → 1st clue | Median clues/100 actions |
|---------|----------------------|---------------------------|--------------------------|
| Wanderer | 0% | — | 0 |
| Curious | 55% | 51 | 1.00 |
| Newcomer | 25% | 51 | 0 |
| Tracker | 85% | 61 | 0.59 |

Soft goals from PR #51: Curious median under ~60 actions (met). Majority of Newcomers with ≥1 clue in first session (not met at 25%).

Long-procession chapter reach (Tracker, 2000 action cap): mean operators at ch1/ch2/ch3 = 0 — breadcrumb following in sim still insufficient for full spine in one run.

## Reproducible player journey (starter walk)

Commands only; no injected `clue_seen`. From a new operator at the Coffee Shop:

1. `go up` — Roof Array  
2. `outskirts` — outskirts staging  
3. `go squalor` — enter wilderness at the gate tile  
4. `look` — lists a **counted mark** (thread) vs **wall mark** (coverage flavor)  
5. `inspect graffiti` — records `clue_seen` for the nearest starter-walk clue; journal row on first visit  
6. Move to another starter tile (short cardinal walk near gate)  
7. `inspect graffiti` again — second clue in the same procedural thread  

Knowledge-gated example (authored): inspect `warm-chalk-doorframe` graffiti, then after seeing `warm-stairwell-cup`, inspect the doorframe again for the “still tends” reveal (`scenario_b` in `discovery_journey_tests.rs`).

## Known limitations

- Static audit uses the epoch `world_seed` for placement; sim **agent RNG seeds** vary paths, not clue locations.  
- Wanderers rarely inspect thread keywords; 0% discovery is expected.  
- Tracker does not yet complete long-procession chapters in bounded runs.  
- `absent-keeper` density near hub is weight-tuned, not a global density change.

## Distinction

**Coverage** answers “what exists and where.” **Discovery benchmark** answers “what Curious/Newcomer agents inspect under the current sim heuristics.” Neither replaces playtesting.
