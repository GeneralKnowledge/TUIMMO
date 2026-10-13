# Phase 2.6 — Monolithic profiles at N=25 (wanderer vs homebody vs visitor)

Follow-up to [Phase 2.5](./phase-2.5-explore-vs-home.md): isolate **single-profile** populations at matched size and seed schedule, without preset mix confounding.

## 1. Research questions

1. How much of Phase 2.5’s home vs exploration gap comes from **occasional-visitor inactive slots** vs **homebody behaviour**?
2. At equal command volume (wanderer vs homebody), how do **discovery**, **home claims/returns**, and **occupancy** differ?
3. What does a **100% visitor** population do to aggregate activity and spatial metrics over 28 virtual days?

## 2. Git commit and command

| Field | Value |
|--------|--------|
| Experiment tooling | Adds CLI presets `homebody` and `visitor` (`uniform` = monolithic wanderer) |
| **Reproduce at commit** | `60e14ab` (presets + report; rerun command in §2 after checkout) |
| **First run note** | JSON was produced from a release build with these presets before this commit was recorded |
| Parent doc baseline | `9c09efa` (Phase 2.5 report on `main`) |

**Command (60 runs, one JSON):**

```bash
cargo run --release -p coffee-sim -- experiment \
  --sizes 25 \
  --replicates 20 \
  --base-seed 1000 \
  --days 28 \
  --mixes uniform,homebody,visitor \
  --snapshot-interval-ms 604800000 \
  --json > phase-2.6-monolithic-N25.json
```

**Settings:** 28 virtual days; action interval 1 h; `max_actions` unlimited; weekly snapshots; seeds same 20 derived from base `1000` as prior phases.

**Allocations (verified in JSON):**

| Condition | Allocated profiles |
|-----------|-------------------|
| `N=25:uniform` | 25 wanderer |
| `N=25:homebody` | 25 homebody |
| `N=25:visitor` | 25 occasional visitor |

Raw JSON (~size similar to Phase 2.5) — regenerate locally; not checked in.

## 3. Validation

| Check | Result |
|-------|--------|
| Conditions | 3 |
| Replicates / condition | 20 |
| Total runs | **60 / 60** |
| Same seeds per replicate index | Verified |
| `max_actions` | `null` |
| Stop reasons | `queue_exhausted` (60/60) |
| Snapshot series | Per condition, `replicate_count` 20 |

## 4. Terminal metrics (mean, σ across 20 replicates)

| Metric | Wanderer (`uniform`) | Homebody | Visitor |
|--------|----------------------|----------|---------|
| Attempted commands / agent | 673.0 (0) | 673.0 (0) | 49.6 (0.60) |
| Total attempted / run | 16825 (0) | 16825 (0) | 1239 (15) |
| Unique locations / run | 1825.9 (154.5) | 1724.0 (230.2) | 120.9 (17.5) |
| Unique locations / agent | 73.0 (6.2) | 69.0 (9.2) | 4.8 (0.7) |
| Unique regions / run | 19.6 (2.4) | 18.4 (3.1) | 3.1 (0.8) |
| Successful exploration moves | 14835 (33) | 14246 (43) | 783 (16) |
| Successful inspections | 1648 (27) | 108 (22) | 103 (8) |
| Repeat visits | 11685 (138) | 11693 (162) | 761 (21) |
| Home returns / run | 0 | **55.5 (10.0)** | 0 |
| Active home claims | 0 | **25 (0)** | 0 |
| Inactive slots / run | 0 | 0 | **330.8 (2.4)** |
| Unique locations occupied (end) | 24.9 | 24.9 | 23.1 |
| Max co-location | 1.1 | 1.1 | 2.1 |
| Social attempts | 0 | 0 | 0 |

## 5. Interpretation (measurements vs speculation)

**Measured**

- **Wanderer vs homebody** share the same **673 commands/agent** and nearly identical repeat-visit totals; homebody adds **25 claims** and **~56 home returns/run** with slightly **lower** unique locations (−6% mean per agent, not adjusted for inferential testing).
- **Visitor** alone explains Phase 2.5’s activity drop: **~330 inactive slots/run** and **~50 commands/agent**, with **~121** aggregate unique locations over 28 days.
- Monolithic wanderer **exceeds** Phase 2.4/2.5 exploration **unique locations** (~1826 vs ~1666) because all 25 agents wander; mix presets included investigators/social agents with different command budgets.

**Paired (same seed, visitor − wanderer unique locations):** mean **−1705** (σ ≈ 150).

**Uncertain:** Whether homebody “stays home” in a human sense; metrics only show returns/claims and similar spread occupancy to wanderers at run end.

## 6. Longitudinal — cumulative unique locations (mean)

```
day:      0    7   14   21   28
wanderer: 0  517  958 1402 1823
homebody: 0  497  944 1340 1721
visitor:  0   32   61   88  120
```

Cumulative attempted commands at day 28 (interval snapshot): wanderer/homebody **16800**; visitor **1232** (terminal mean attempted **1239**).

## 7. Tooling change (minimal)

`coffee-sim experiment --mixes` now accepts **`homebody`** and **`visitor`** monolithic presets (see `docs/simulation.md` preset table). Enables one reproducible 60-run matrix with labelled conditions.

## 8. Tests run

| Command | Result |
|---------|--------|
| `cargo fmt --check` | Pass |
| `cargo test -p coffee-sim` | 84 passed |
| `cargo test --workspace` | Pass |

## 9. Reproduction

1. Build/run at commit containing preset + report changes.
2. Execute command in §2; confirm 60 runs and allocations above.
3. Compare wanderer totals to Phase 2.5 exploration only after noting profile mix difference.

## 10. Suggested next step

**Two-profile ablation at N=25:** e.g. 50/50 wanderer/homebody with same seeds, to see interaction/co-location and home returns when explorers and homebodies share a world — without visitor-driven inactivity.

---

*Descriptive statistics from JSON captured 2026-10-13; not inferential.*
