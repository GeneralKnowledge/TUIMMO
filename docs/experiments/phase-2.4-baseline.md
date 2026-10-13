# Phase 2.4 — Controlled baseline calibration (exploration-heavy mix)

## 1. Purpose and research questions

This experiment records the **first reproducible multi-seed baseline** for TUIMMO’s synthetic population under the current world and behavioural profiles. It is **measurement**, not a simulator feature phase.

**Research questions**

1. How does **population size** (10 / 25 / 50 agents) affect total activity, per-agent activity, and spatial discovery when profile weights are held constant?
2. Do larger populations mainly add **parallel exploration**, **spatial overlap**, or both?
3. How do cumulative discovery and command counts evolve over a **28-day virtual horizon** with weekly snapshots?
4. How large is **between-seed variability** relative to differences between population sizes?

This does **not** claim realistic human behaviour, production SSH capacity, or predictive validity.

---

## 2. Repository commit and reproducible command

| Field | Value |
|--------|--------|
| Repository | `https://github.com/GeneralKnowledge/TUIMMO` |
| Git commit (experiment run) | `4dc6512eabd47101ec79b655f016d85d2db78515` |
| Branch at run time | `main` (includes Phase 2.3 #33 and Phase 2.3.1 #34) |

**Exact command** (release build recommended for the full matrix):

```bash
cargo run --release -p coffee-sim -- experiment \
  --sizes 10,25,50 \
  --replicates 20 \
  --base-seed 1000 \
  --days 28 \
  --wanderer-weight 60 \
  --investigator-weight 30 \
  --social-weight 10 \
  --snapshot-interval-ms 604800000 \
  --json > baseline-experiment.json
```

**Effective global settings** (from JSON `experiment` block and CLI defaults):

| Setting | Value |
|---------|--------|
| Virtual duration | 28.0 days |
| Virtual start (`--start-ms`) | `1700000000000` |
| Action interval (`--action-interval-ms`) | `3600000` (1 virtual hour) |
| Max actions (`--max-actions`) | omitted → `0` → **no cap** (`null` in JSON) |
| Snapshot interval | `604800000` ms (7 virtual days) |
| Snapshot max | `128` |
| Profile weights (w / i / h / s / v) | 60 / 30 / 0 / 10 / 0 |

**Raw artifact:** Full JSON (~114 KiB, 60 runs) was written to  
`/opt/cursor/artifacts/baseline-experiment.json` on the cloud agent that executed this baseline. It is **not** checked into the repo; regenerate with the command above on the same commit.

---

## 3. Experimental design and controls

| Control | Implementation |
|---------|----------------|
| Conditions | One condition per population size: N=10, N=25, N=50 |
| Replicates | 20 per condition → **60 runs** |
| Seed schedule | `from_base` with `base_seed=1000`; same **20 derived seeds** for every condition |
| World state | Independent world per replicate (same seed does not share state across conditions) |
| Behavioural mix | Global weights 60/30/10 (exploration-heavy); **not** `--mixes` presets |
| Only varied factor | `agent_count` per condition |
| Longitudinal sampling | Weekly interval snapshots; aggregates under each condition’s `snapshot_series` |

### Profile weights and allocated counts

Weights are **proportions** (normalized by sum). Allocation uses **largest-remainder** counts summing to N (`PopulationMix::allocate`).

| N | Weights (W/I/H/S/V) | Allocated counts (W / I / S) |
|---|---------------------|------------------------------|
| 10 | 60 / 30 / 0 / 10 / 0 | 6 / 3 / 1 |
| 25 | 60 / 30 / 0 / 10 / 0 | 15 / 8 / 2 |
| 50 | 60 / 30 / 0 / 10 / 0 | 30 / 15 / 5 |

(Homebody and occasional visitor weights are zero → zero agents.)

### Derived replicate seeds (`seeds_used`)

Indices `0..19` use `replicate_seed(1000, index)` (see `crates/coffee-sim/src/seeds.rs`):

`4332104999045480776`, `15022492692291828655`, `14266957338849687121`, `5715306481986817959`, `12535570497727931551`, `15459611581812441186`, `1429410483394773236`, `11976406938499015031`, `9645082984685971274`, `8450917575532391301`, `13198386819996916331`, `15130455330898545880`, `4686906960306480316`, `17304219534521234593`, `8438919598853000343`, `8601765545377070667`, `5359225427314914868`, `3616392152730960348`, `3976641873453246744`, `15799039541941293328`

---

## 4. Run counts and termination validation

| Check | Result |
|-------|--------|
| `total_runs_requested` | 60 |
| `total_runs_completed` | 60 |
| Replicates per condition | 20 / 20 / 20 |
| Same seed per replicate index across conditions | Verified (pairwise match on all 20 indices) |
| `max_actions` in JSON | `null` (unlimited) |
| Snapshot series location | Only under `conditions[i].snapshot_series` (no top-level series) |
| `replicate_count` on snapshot points | 20 for every point in every condition |

### Termination

| Stop reason | Count (all 60 runs) |
|-------------|---------------------|
| `queue_exhausted` | 60 |
| `horizon_reached` | 0 |
| `action_limit` | 0 |

**Interpretation:** The scheduler **consumes all gameplay events** that were enqueued within the configured window. The loop then exits with `queue_exhausted` rather than dequeuing an event beyond the horizon (`horizon_reached`). This is consistent with `StopReason` documentation: queue exhausted means no further events were scheduled before the horizon.

**Final virtual time:** Elapsed from start ≈ **28.0 days** for all runs (e.g. N=10: `final_world_ms=1702419200010` → ~28.0000001 days). Slight **N-dependent** sub-millisecond differences in `final_world_ms` come from equal-timestamp tie-breaking across agent counts, not from different configured durations.

**Horizon vs activity:** With the default 1-hour action interval and visitor-free mix, **attempted commands per agent** is **673** for every replicate and every N (zero variance). Total attempted scales linearly with N (673 × N). Weekly snapshot cumulative command means at day 28 match N × 6720 (= 673×N − 10), reflecting commands recorded at the last **interval** observation before the final handful of gameplay events.

---

## 5. Summary table — population sizes (terminal metrics)

Statistics are **across 20 replicates** per condition (`NumericSummary`: sample mean, sample σ, min, max). Unless noted, values are **population totals per run** (sum over agents).

| Metric | Unit | Aggregation | N=10 mean (σ) [min, max] | N=25 mean (σ) [min, max] | N=50 mean (σ) [min, max] |
|--------|------|-------------|---------------------------|---------------------------|---------------------------|
| Total attempted commands | count / run | population total | 6730 (0) [6730, 6730] | 16825 (0) [16825, 16825] | 33650 (0) [33650, 33650] |
| Attempted commands per agent | cmd / agent / run | mean of per-run rate | 673 (0) [673, 673] | 673 (0) [673, 673] | 673 (0) [673, 673] |
| Total successful commands | count / run | population total | 6730 (0) | 16825 (0) | 33650 (0) |
| Unique locations visited | places / run | cumulative discovery | 1034.25 (153.72) [815, 1330] | 1666.45 (153.33) [1424, 1921] | 2268.20 (180.02) [1914, 2633] |
| Unique locations per agent | places / agent / run | mean of per-run rate | 103.43 (15.37) [81.5, 133] | 66.66 (6.13) [56.96, 76.84] | 45.36 (3.60) [38.28, 52.66] |
| Unique regions visited | regions / run | cumulative | 13.0 (2.05) [9, 17] | 19.0 (2.00) [15, 23] | 23.0 (2.91) [18, 30] |
| Successful exploration moves | count / run | population total | 5127.5 (27.86) [5054, 5164] | 12721.5 (52.65) [12631, 12812] | 25463.5 (88.26) [25297, 25630] |
| Successful inspections | count / run | population total | 673.5 (21.58) [639, 723] | 1731.0 (34.82) [1674, 1788] | 3374.0 (61.20) [3267, 3481] |
| Repeat visits | count / run | population total | 4879.5 (95.68) [4732, 5020] | 12244.5 (113.70) [12015, 12474] | 24697.5 (181.00) [24375, 25020] |
| Home returns | count / run | population total | 0 (0) | 0 (0) | 0 (0) |
| Active home claims | count / run | end state | 0 (0) | 0 (0) | 0 (0) |
| Social attempts | count / run | population total | 13.0 (4.59) [4, 21] | 48.0 (11.31) [23, 68] | 210.0 (37.30) [158, 320] |
| Social successes | count / run | population total | 13.0 (4.59) [4, 21] | 48.0 (11.31) [23, 68] | 210.0 (37.30) [158, 320] |
| Inactive scheduler slots | count / run | population total | 0 (0) | 0 (0) | 0 (0) |
| Unique locations occupied | places / run | end-of-run | 10 (0) [10, 10] | 25 (0) [25, 25] | 49.95 (0.59) [48, 50] |
| Max agents sharing one location | agents | end-of-run max | 1 (0) [1, 1] | 1.1 (0.31) [1, 2] | 1.4 (0.50) [1, 2] |
| Hub agent share | fraction | end-of-run | 0 (0) | 0 (0) | 0 (0) |
| Deep Squalor agent share | fraction | end-of-run | 0 (0) | 0 (0.012) [0, 0.04] | 0.002 (0.006) [0, 0.02] |

**Success rates:** Exploration, inspect, and social success rates are **1.0** for every replicate (no rejections recorded in this baseline).

---

## 6. Per-agent vs population-wide activity

**Observation:** Per-agent command throughput is **flat** across N (673 attempts/agent). Population **totals** scale **linearly** with N (factor 2.5 and 5.0 from 10→25 and 10→50 for totals, as in JSON `comparisons`).

**Discovery:** Population **unique locations visited** **increase** with N (+61% mean 10→25, +119% 10→50), but **unique locations per agent decrease** (−36% 10→25, −56% 10→50). Larger populations explore **more places in aggregate** but **fewer places per agent** under this mix and scheduler.

**Social:** Social attempts scale roughly with social-agent count (1 / 2 / 5 agents) and co-presence opportunities; mean social attempts per social agent ≈ 13 (N=10), 24 (N=25), 42 (N=50) over 28 days — descriptive only, not a significance test.

---

## 7. Spatial behaviour

- **End-of-run occupancy:** Unique locations occupied ≈ N for N=10 and N=25; N=50 mean 49.95 with occasional 48 (one replicate left two agents sharing a location).
- **Co-location:** Max agents at one place remains low (mean 1.0–1.4; max 2). No hub concentration (`hub_agent_share` = 0).
- **Deep Squalor:** Negligible occupancy share at N=25/50 (max 4% on a single replicate); zero for N=10.
- **Overlap vs exploration:** Rising N increases **total** unique places visited but **reduces per-agent** uniqueness — consistent with partial overlap of individual trajectories rather than N independent full maps.

Command acceptance here equals success (100%); **unique-location counts remain a proxy**, not proof of “meaningful” discovery.

---

## 8. Longitudinal behaviour (condition-specific `snapshot_series`)

Five points per condition: `initial` at day 0, then `interval` at days 7, 14, 21, 28 (`elapsed_virtual_ms` / 86400000). No separate `terminal` row in the series for this run; day-28 interval aligns with last grid sample before run end.

### Cumulative unique locations visited (mean across replicates)

```
day:     0      7      14     21     28
N=10:    0    289    557    804   1033
N=25:    0    464    888   1293   1664
N=50:    0    601   1160   1726   2265
```

(Values rounded from JSON means; σ at day 28 ≈ 153 / 153 / 180 for N=10/25/50.)

### Cumulative attempted commands (mean)

At each weekly boundary, means are **deterministic** across replicates: N × (1680 × week_index) at days 7–28 (e.g. N=10 → 1680, 3360, 5040, 6720 at days 7–28).

### Current unique locations occupied (mean)

Rises from **1** at initial snapshot toward **≈ N** by day 28 (N=10: 9.8→10.0 by week 1; N=50: ~48–49.5 throughout weeks 1–4).

### Between-replicate variation over time

**Cumulative unique locations** σ grows with horizon (e.g. N=10: 0 → 48 → 85 → 123 → 153). **Command counts** at interval snapshots show **zero** σ (fully determined given N and schedule).

**Plateau:** Per-agent command rate is fixed; **discovery** means continue to climb through day 28 without flattening in this run.

---

## 9. Variability, outliers, and limitations

| Topic | Finding |
|-------|---------|
| Contributing replicates | n = 20 for all reported aggregates |
| σ definition | Sample standard deviation across replicates (`stats` module; undefined for n=1) |
| Outliers (unique locations visited) | N=10: rep 7 low (815), rep 11 high (1330); N=50: rep 15 low (1914), rep 7 high (2633). All `queue_exhausted`, same profile allocation, same per-agent command count |
| N vs variability | Differences in mean unique locations between N=10 and N=50 (~1200 places) exceed typical between-seed σ (~150–180) but **no inferential tests** were run |
| Home / visitor behaviour | Not exercised (zero homebody/visitor weight) |
| Termination label | `queue_exhausted` despite full 28-day window — document when comparing to runs that hit `horizon_reached` early |
| External validity | Synthetic command-path simulation only |

**Supported:** Larger N increases aggregate discovery and social volume; per-agent discovery and co-location patterns above are directly measured.

**Uncertain / tentative:** Whether overlap or diminishing per-agent returns dominate in other mixes; whether discovery would plateau on longer horizons; causal claims about “world feel” or realism.

---

## 10. Experiment tooling validation (pre-run)

| Step | Outcome |
|------|---------|
| `docs/simulation.md` + CLI | `experiment` subcommand; global weights/days/snapshot flags; `--max-actions 0` → unlimited |
| Condition-specific snapshots | `conditions[].snapshot_series` only (Phase 2.3.1) |
| `cargo fmt --check` | Pass |
| `cargo test -p coffee-sim` | **84** tests passed (6 unit + 23 + 20 + 13 + 22 integration) |
| `cargo test --workspace` | All crates passed (includes **180** `coffee-world` tests among others) |
| Smoke run | N=10, 2 replicates, 7 days — JSON schema validated |
| Simulator changes this phase | **None** (no defects blocking measurement) |

---

## 11. Observations worth testing next

1. **Profile contrast at fixed N=25:** exploration-heavy (this baseline) vs `--mixes home` with the **same 20-seed schedule** — home returns, claims, hub/squalor occupancy, and per-agent discovery should diverge if profiles matter.
2. **Social encounter rate:** with only 2–5 social agents, social commands are sparse at N=10; a **social-weighted mix** at matched N would test co-presence frequency.
3. **Discovery saturation:** extend virtual days or reduce action interval to see if cumulative unique locations plateau while repeat visits dominate.
4. **Horizon stop reason:** compare runs where the event queue ends before vs at horizon when visitor offline gaps are enabled.

---

## 12. Recommended next experiment

**Run a matched comparison: exploration-heavy vs home-oriented at N=25 only** (20 replicates, same `base-seed 1000`, 28 days, weekly snapshots):

```bash
cargo run --release -p coffee-sim -- experiment \
  --sizes 25 \
  --replicates 20 \
  --base-seed 1000 \
  --days 28 \
  --mixes exploration,home \
  --snapshot-interval-ms 604800000 \
  --json > phase-2.5-explore-vs-home-N25.json
```

This holds population size and seed schedule constant while changing only the behavioural mix. It directly addresses whether the current world responds differently to home-oriented agents (claims, returns, occupancy concentration) — a gap in this baseline where home metrics were uniformly zero.

---

## 13. Reproduction checklist

1. Checkout commit `4dc6512eabd47101ec79b655f016d85d2db78515`.
2. `cargo run --release -p coffee-sim -- experiment …` as in §2.
3. Confirm `total_runs_completed == 60` and three conditions with 20 replicates each.
4. Confirm identical `seeds_used` and per-index `seed` across conditions.
5. Analyze `conditions[].aggregate` and `conditions[].snapshot_series`; do not treat missing snapshot fields as zero.

---

*Report generated from measured output on 2026-10-13. Values are descriptive statistics only; no significance testing.*
