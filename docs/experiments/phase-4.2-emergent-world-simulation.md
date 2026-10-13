# Phase 4.2 — Emergent world simulation & persistent stories

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `75b7834` (main after Phase 4.1 merge) |
| **Resulting commit** | (this branch) |

## Motivation

Phase 4.1 connected charting, corroboration, and multi-subject theories, but:

- Charting hints repeated on every `inspect ground`.
- Population simulation reported feature counts, not cross-operator stories.
- Agents rarely posted theories, read signs, or reacted to earlier traces.

This milestone wires one-time charting hints, extends agent behaviour and memory from **player-visible** command output, and adds interaction metrics plus a deterministic acceptance path for sign → inspect → theory.

## Fix A — One-time charting hints

- [`append_charting_once`](../../crates/coffee-world/src/state/inspect_cmds.rs) sets operator flag `chart-hint:{world}:{epoch}:{origin_x}:{origin_y}` via `PersistOp::Flag` on first eligible `inspect ground`.
- Corroboration and theory summary lines are unchanged and still appear on every relevant inspect.
- Tests: repeat inspect uses `finding` panel (feed retains history); [`charting_and_theories_survive_reopen`](../../crates/coffee-world/src/store/store_tests.rs) verifies flag persistence.

## Fix B — Theories in exploration

No second theory system. Inspect still adds a single summary line; agents and players use `theory list <subject>`. Simulator memory records subjects from inspect/overlay text using wilderness **place keys**, not display names.

## Simulation architecture

| Piece | Role |
| --- | --- |
| [`SimulationRunner`](../../crates/coffee-sim/src/runner.rs) | Shared `WorldState`, virtual clock, priority scheduler |
| [`profile::choose_action`](../../crates/coffee-sim/src/profile.rs) | Weighted commands per profile (incl. new **follower**) |
| [`feed_memory`](../../crates/coffee-sim/src/feed_memory.rs) | Parse feed/finding/look into bounded agent memory |
| [`interaction`](../../crates/coffee-sim/src/interaction.rs) | Trace creation, encounter pairs, connected sequences, transcript |
| [`execute_line`](../../crates/coffee-sim/src/execute.rs) | Same `WorldState::apply` as live play |

Profiles (weights via CLI or `PopulationMix::story_emergence()`):

- **Wanderer** — explore, inspect ground, occasional `sign`.
- **Investigator** — inspect mark, `journal record`, occasional `theory post … survey-mark`.
- **Homebody** — claim / return home.
- **Follower** — `inspect sign`, `theory list <subject>`, competing `theory post` when hints exist.
- **Social player / occasional visitor** — unchanged from Phase 3.x sim work.

Agents do **not** read private journals or global DB rows; only parsed public output.

## Metric definitions

| Metric | Meaning |
| --- | --- |
| `public_traces_created` | Signs or theory posts recorded by the tracker during the run |
| `traces_encountered_by_later_operators` | Unique (trace, agent) pairs where a **different** agent saw the trace at the same place key via look/inspect memory |
| `theory_subjects_multi_author` | Subjects with ≥2 distinct sim agent authors (from world state at end) |
| `connected_sequences` | Encounter followed by a meaningful command (inspect/sign/theory/journal record) |
| `trace_encounter_rate` | encounters / traces created (`None` if no traces) |

Materialized regions and theory entry counts are **not** proof of shared story; the interaction block is documented separately in reports.

## Deterministic acceptance vs population runs

**Acceptance** ([`cross_operator_acceptance.rs`](../../crates/coffee-sim/tests/cross_operator_acceptance.rs)): Ada leaves a sign; Bob walks the same path, `inspect sign`, posts a theory — one connected sequence. This proves the gameplay loop and metric wiring; it is not emergent scheduling.

**Population** (example):

```bash
cargo run -p coffee-sim -- --players 6 --wanderer-weight 25 --investigator-weight 25 \
  --homebody-weight 15 --follower-weight 35 --days 5 --max-actions 500 --seed 4242
```

Observed on seed `4242` (500 actions, 5 virtual days): 2 public traces, **0** cross-operator encounters, 2 theory entries on 1 subject, 0 multi-author subjects. Agents explored widely but rarely co-located on signed tiles — reported honestly, not as success.

Short CI-style run: [`story_emergence.rs`](../../crates/coffee-sim/tests/story_emergence.rs) (`seed 4242`, 400 actions, 3 days).

## Validation

```bash
cargo fmt --all --check
cargo test -p coffee-world
cargo test -p coffee-sim
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Remaining gaps

- Population mixes still produce many isolated signs with low encounter rates unless agents converge geographically.
- Followers do not pathfind toward known signs without visiting the tile.
- No LLM agents; narratives in reports come only from logged commands.
- Charting hint is per birth cell, not a full movement audit.

## Recommended next step

Bias navigation toward **remembered trace locations** (still without omniscient DB reads): when memory records a sign at a place key, increase cardinal weight toward revisiting that region, then re-measure encounter rates under fixed seeds.
