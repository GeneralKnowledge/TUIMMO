# Phase 2.5 — Matched exploration vs home-oriented (N=25)

## 1. Research questions and pre-run hypotheses

**Primary question:** Under the same population size (25), virtual duration (28 days), and replicate seed schedule, how differently do **exploration-heavy** and **home-oriented** presets use the existing world?

### A. Home behaviour (questions)

- Do home-oriented runs produce **active personal home claims**?
- Are homes **revisited**, or only claimed?
- Do home-oriented agents **stay near home** or keep exploring?
- Are home metrics **consistent across seeds**?

*Hypotheses ( falsifiable, not assumed ):*
- Home preset should yield **non-zero** `home_claims_active` and `home_returns` where Phase 2.4 exploration runs showed zeros.
- Returns may still be **sparse** relative to total commands if exploration commands dominate homebody time.

### B. Exploration (questions)

- Differences in **unique locations** (total and per agent)?
- Differences in **exploration moves**, **inspections**, **repeat visits**?
- Are totals explained by **lower attempted commands** (inactive visitor slots) rather than spatial preference alone?

*Hypothesis:* Home-oriented runs may show **lower aggregate discovery** partly because **occasional visitors** contribute inactive scheduler slots and fewer attempts per agent.

### C. Social behaviour (questions)

- Social attempts and successes in each preset?
- Enough **co-presence** for social commands?

*Hypothesis:* Both presets include a small social share; exploration has **2** social agents vs home **1**, so exploration may show higher social **counts** without implying better “social design.”

### D. Spatial occupancy (questions)

- End-of-run **unique occupied locations**, **max co-location**, **hub** and **deep-Squalor** shares?

*Hypothesis:* End-of-run occupancy remains **dispersed** (low hub share) in both conditions; differences in co-location may be modest.

### E. Longitudinal behaviour (questions)

- Weekly progression of **cumulative unique locations** and **attempted commands**?
- Home-specific snapshot metrics?

*Hypothesis:* Cumulative commands in home condition **lag** exploration at each week because of offline/inactive visitor time. Snapshot series **do not** currently export home claims or returns (terminal metrics only).

---

## 2. Git commit and command

| Field | Value |
|--------|--------|
| Repository | `https://github.com/GeneralKnowledge/TUIMMO` |
| **Experiment commit** | `d552095b6958737e66d956cbc352d335fd040d39` |
| Prior reference | Phase 2.4 report at same tip; baseline *measurements* at `4dc6512` |

No simulator code changed between Phase 2.4 merge and this run (`git log 4dc6512..d552095` is documentation-only).

**Exact command:**

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

**Effective settings (JSON `experiment` + CLI defaults):**

| Setting | Value |
|---------|--------|
| Virtual duration | 28.0 days |
| Virtual start | `1700000000000` |
| Action interval | `3600000` ms (1 virtual hour) |
| Max actions | omitted → unlimited (`null`) |
| Snapshot interval | `604800000` ms (7 days) |
| Snapshot max | `128` |

**Raw JSON:** Generated locally as `phase-2.5-explore-vs-home-N25.json` (~76 KiB). Not checked into the repo; reproduce with the command above.

---

## 3. Preset weights and allocated profiles (N=25)

Verified from `PresetMix` → `PopulationMix` in `crates/coffee-sim/src/main.rs` and JSON `population` / `allocated_profile_counts`.

| Preset | W / I / H / S / V weights | Allocated counts (W / I / H / S / V) |
|--------|---------------------------|--------------------------------------|
| **exploration** | 60 / 30 / 0 / 10 / 0 | 15 / 8 / 0 / 2 / 0 |
| **home** | 10 / 0 / 50 / 5 / 35 | 3 / 0 / 12 / 1 / 9 |

Weights match `docs/simulation.md` preset table. Allocation uses largest-remainder (`PopulationMix::allocate`).

**Derived seeds (`base_seed=1000`, 20 replicates):** same list as Phase 2.4 (`seeds_used` in JSON); see Phase 2.4 report §3 or `crates/coffee-sim/src/seeds.rs`.

---

## 4. Experiment validation

| Check | Result |
|-------|--------|
| Conditions | 2: `N=25:exploration`, `N=25:home` |
| Agents per condition | 25 / 25 |
| Replicates per condition | 20 / 20 |
| Total runs completed | **40 / 40** |
| Same seed per replicate index | Verified (20/20 indices match) |
| Independent worlds | Yes (standard multi-seed runner) |
| `max_actions` | `null` |
| Snapshot series | Under each `conditions[i].snapshot_series` only |
| Snapshot `replicate_count` | 20 at every point |
| Exploration N=25 vs Phase 2.4 | Terminal means match prior baseline (e.g. total attempted 16825, unique locations 1666.45) |

### Termination

| Condition | Stop reason (all 20 replicates) | Mean `final_world_ms` (≈ elapsed) |
|-----------|----------------------------------|-----------------------------------|
| exploration | `queue_exhausted` | `1702419200025` (~28.0 d) |
| home | `queue_exhausted` | `1702419200019` (~28.0 d) |

As in Phase 2.4, **`queue_exhausted`** means the scheduler consumed all enqueued gameplay events within the horizon window; elapsed time still reaches ~28 virtual days.

---

## 5. Terminal metrics comparison

Statistics: **sample mean (σ) [min, max]** across 20 replicates. Exploration = **E**, Home = **H**. Δ = H − E; relative = Δ/E when E ≠ 0.

| Metric | Unit | Basis | E mean (σ) [min,max] | H mean (σ) [min,max] | Δ | Rel Δ |
|--------|------|-------|----------------------|----------------------|---|-------|
| Total attempted commands | /run | population | 16825 (0) [16825,16825] | 11209.75 (7.63) [11104,11175] | −5615.25 | −33.4% |
| Attempted commands / agent | /agent/run | per-run rate | 673 (0) [673,673] | 448.39 (0.31) [444.16,447] | −224.61 | −33.4% |
| Total successful commands | /run | population | 16825 (0) | 11150.80 (16.45) [11104,11175] | −5674.20 | −33.7% |
| Unique locations visited | /run | population cumulative | 1666.45 (153.33) [1424,1921] | 1431.65 (135.21) [1243,1507] | −234.80 | −14.1% |
| Unique locations / agent | /agent/run | per-run rate | 66.66 (6.13) [56.96,76.84] | 57.27 (5.41) [49.72,60.28] | −9.39 | −14.1% |
| Unique regions visited | /run | population | 18.75 (2.00) [15,23] | 17.65 (1.73) [14,21] | −1.10 | −5.9% |
| Successful exploration moves | /run | population | 12701.85 (52.65) | 9383.95 (58.37) | −3317.90 | −26.1% |
| Successful inspections | /run | population | 1737.45 (34.82) | 293.30 (22.99) | −1444.15 | −83.1% |
| Repeat visits | /run | population | 12278.00 (113.70) | 7799.75 (121.13) | −4478.25 | −36.5% |
| Home returns | /run | population | 0 (0) | 28.35 (7.62) [10,45] | +28.35 | n/a (E=0) |
| Active home claims | /run | end state | 0 (0) | 12.00 (0) [12,12] | +12.00 | n/a |
| Social attempts | /run | population | 47.70 (11.31) | 33.45 (20.88) | −14.25 | −29.9% |
| Social successes | /run | population | 47.70 (11.31) | 33.45 (20.88) | −14.25 | −29.9% |
| Inactive scheduler slots | /run | population | 0 (0) | 119.45 (1.36) [117,122] | +119.45 | n/a |
| Unique locations occupied | /run | end-of-run | 25.00 (0) | 24.65 (0.59) [23,25] | −0.35 | −1.4% |
| Max agents at one location | agents | end-of-run max | 1.10 (0.31) [1,2] | 1.50 (0.51) [1,3] | +0.40 | +36.4% |
| Hub agent share | fraction | end-of-run | 0 (0) | 0 (0) | 0 | n/a |
| Deep Squalor agent share | fraction | end-of-run | ~0 | ~0 | ~0 | — |

**Pooled inactive slot share:** E 0.0; H ≈ 0.0105 (2389 inactive slots / 226584 scheduler slots in aggregate).

**Success rates:** Exploration move success rate 1.0 in both conditions (pooled). Home runs show a small gap between attempted and successful totals (mean difference ≈ 59 commands/run, σ ≈ 16.5 on successful totals).

---

## 6. Per-agent activity and exploration

**Observation:** Home-oriented runs execute **~33% fewer** attempted commands per agent (448 vs 673), driven by **119 inactive slots per run** (occasional-visitor offline scheduling). This must be read alongside spatial metrics.

**Per-agent unique locations** fall **~14%** (paired mean Δ ≈ −9.39 places/agent, σ ≈ 7.99 across the 20 shared seeds).

**Descriptive ratio (not a simulator metric):** mean unique locations per attempted command ≈ **0.099** (E) vs **0.128** (H). Home runs visit **fewer unique places overall** but **slightly more unique places per attempt** on average — still synthetic and confounded by profile mix (no investigators in home preset).

**Exploration condition** replicates Phase 2.4 N=25 exactly → cross-check that mix comparison is isolated to preset change.

---

## 7. Home claim and return findings

**Measurements (home condition only):**

- **`home_claims_active`:** 12 on **every** replicate — matches 12 homebody agents; claims appear **stable** across seeds.
- **`home_returns`:** mean **28.35** (σ 7.62), range **[10, 45]** per run (population total, not per homebody).

**Interpretation (separate from measurement):**

- Home preset **does** exercise claim/return counters absent in exploration-heavy runs.
- Returns are **non-zero** but modest relative to thousands of exploration commands; homebodies **do not** appear home-bound exclusively.

**Exploration condition:** home returns and claims remain **0** (no homebody/visitor profiles).

---

## 8. Social and spatial occupancy

**Social:** E mean 47.7 social attempts vs H 33.5 (−30%). Exploration allocates **2** social players vs **1** in home; both show **100% social success rate** in aggregates. Count differences reflect **mix and activity level**, not evidence of richer social systems.

**Spatial (end-of-run):**

- Both conditions spread agents across **~25** occupied locations (H occasionally 23–24).
- **Max co-location** slightly higher in H (mean 1.5, max 3 vs E max 2).
- **Hub share** and **deep-Squalor share** ≈ 0 in both — consistent with Phase 2.4; **not** evidence that agents never visit hub during the run.

---

## 9. Longitudinal behaviour (condition-specific snapshots)

Available fields per point: **cumulative unique locations visited**, **cumulative attempted commands**, **current unique locations occupied**. **Home returns/claims are not in snapshot series** (terminal metrics only).

### Cumulative unique locations visited (mean)

```
day:          0     7    14    21    28
exploration:   0   464   888  1293  1664
home:          0   408   754  1089  1430
```

### Cumulative attempted commands (mean)

```
day:          0     7    14    21    28
exploration:   0  4200  8400 12600 16800
home:          0  2834  5617  8400 11191
```

(Week-28 interval snapshots precede the final few gameplay events; terminal attempted mean is 16825 / 11209.75.)

### Current unique locations occupied (mean)

```
day:          0     7    14    21    28
exploration: 1.0  24.6  24.7  24.6  24.9
home:        1.0  22.9  23.7  24.4  24.4
```

**Observation:** Discovery and command curves **run parallel** but home **lags** in commands from week 1 onward; cumulative unique locations gap **widens** through day 28. Occupancy converges toward N by week 4 in both.

---

## 10. Between-seed variability and outliers

| Metric | E σ | H σ | Notes |
|--------|-----|-----|-------|
| Unique locations visited | 153 | 135 | Substantial seed variance in both |
| Home returns | — | 7.62 | Replicate 13 low (10); replicate 9 high (45) |
| Total attempted (home) | — | 7.63 | Narrow band (11104–11175) |

**Paired analysis (same seed index, H − E):**

| Metric | Paired mean Δ | Paired σ |
|--------|---------------|----------|
| Unique locations visited | −234.8 | 199.7 |
| Total attempted | −5615.25 | 7.44 |
| Home returns | +28.35 | 7.43 |
| Home claims active | +12.0 | 0.0 |

Identical seeds **do not** imply identical trajectories after profile assignment differs.

---

## 11. Observations vs interpretations

| Measured observation | Tentative interpretation (uncertain) |
|----------------------|--------------------------------------|
| H has 12 active claims every run | Homebody claim path is reachable in current world |
| H has 10–45 home returns/run | Returns occur but are not the dominant activity |
| H has ~33% fewer attempts/agent | Occasional-visitor inactive slots reduce command volume |
| H has ~83% fewer inspections | No investigator profile in home preset (mix composition effect) |
| Lower aggregate unique locations in H | Partly lower activity; per-attempt discovery slightly higher descriptively |
| Social attempts lower in H | Fewer social agents and fewer active command opportunities |

Do **not** infer real-player psychology, SSH capacity, or statistical significance from these descriptive summaries.

---

## 12. Limitations

- Synthetic agents via offline command path only; no network/TUI load.
- **Mix confounding:** home preset removes investigators and adds visitors/homebodies — not a single-profile ablation.
- Longitudinal snapshots omit home and social series fields.
- All runs **`queue_exhausted`** at ~28 d; label differs from `horizon_reached` semantics (see Phase 2.4).
- No inferential hypothesis tests in this phase.

---

## 13. Tooling validation (pre-run)

| Command | Result |
|---------|--------|
| `cargo fmt --check` | Pass |
| `cargo test -p coffee-sim` | **84** passed |
| `cargo test --workspace` | All crates passed (incl. **180** `coffee-world` tests) |

No simulator code changes were required for this experiment.

---

## 14. Reproduction checklist

1. Checkout `d552095b6958737e66d956cbc352d335fd040d39`.
2. Run the command in §2; confirm `total_runs_completed == 40`.
3. Confirm conditions `N=25:exploration` and `N=25:home`, each with 20 replicates.
4. Verify identical `seed` per replicate index across conditions.
5. Compare exploration aggregates to Phase 2.4 N=25 row for regression sanity.
6. Analyze terminal `aggregate` and `snapshot_series`; do not treat missing snapshot fields as zero.

---

## 15. Recommended next experiment

**Monolithic profile comparison at N=25** (20 seeds, 28 days, weekly snapshots): three conditions — 100% homebody vs 100% wanderer vs 100% occasional visitor — using `PopulationMix::monolithic` via explicit weights (e.g. `--homebody-weight 100` with other weights 0) or library-only conditions if CLI lacks monolithic flags.

**Why:** Phase 2.5 confounds preset differences (visitors, investigators, social counts). Single-profile runs would separate **inactive-slot effects**, **home return behaviour**, and **exploration breadth** without mix composition confounding.

---

*Report from measured JSON on 2026-10-13. Descriptive statistics only.*
