# Phase 4.1 — Persistent discovery, shared theories & world history

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `63a87b6f872e671b7241daecfdddb018af922ad6` (main after Phase 4.0) |
| **Resulting commit** | `00d06db` |

## Problem

Phase 4.0 connected independent private field notes through corroboration counts, but operators still could not see who first opened a birth cell, and shared theories remained effectively scoped to the delisted-floor mystery. Exploration did not yet read as a accumulating shared past.

## Design and data flow

```
Movement → ensure_region → entered row (actor_id = first charter)
Inspect ground → charting_inspect_line (if charter ≠ you)
Inspect feature → corroboration (field rows) + theories_summary_line (shared_theories.topic)
Theory post → subject from location, explicit token, or delisted-floor default
```

No new tables. Reused:

- `region_discoveries` kind `entered` for first charting ([`entered_for_region`](../../crates/coffee-world/src/geography/records.rs))
- `shared_theories.topic` as theory **subject** (same string family as journal `content_key` or place keys)
- Phase 4.0 corroboration helpers on inspect

New module [`history.rs`](../../crates/coffee-world/src/history.rs): subject validation, optional prefix parsing, charting and theory summary lines on `WorldState`.

## Player-visible changes

1. **`inspect ground`** in the Squalor may add: *This district was first charted by &lt;name&gt;.* Only when another operator materialized that birth cell. Not on every step.
2. **`theory list`** without args lists all subjects with entries; **`theory list &lt;subject&gt;`** filters.
3. **`theory post`** accepts `about &lt;subject&gt;`, a leading subject token (`survey-mark …`), infers subject at authored tiles, or defaults to `delisted-floor` in the Hub (legacy).
4. **`theory read &lt;id&gt;`** is global by id; overlay shows **Subject:** line.
5. **`inspect`** at authored features may append public theory counts and `theory list &lt;subject&gt;` hint (alongside corroboration when applicable).

Signs and trails unchanged. Private journals and waypoints never appear in history or theory overlays.

## Privacy

- Charting names the **first charter** only (public operator display name from persisted account).
- Corroboration remains a count only (Phase 4.0).
- Theory bodies are intentionally public; journal text is never copied into theory posts automatically.

## Simulator

[`WorldOutcomeSnapshot`](../../crates/coffee-world/src/sim.rs) adds:

- `theory_subjects` — distinct `topic` values with ≥1 entry
- `theory_entries` — total theory rows

Existing `corroborated_field_keys` unchanged.

**Limitation:** Default sim agents still move and journal lightly; they do not reliably `inspect ground`, post place-scoped theories, or revisit prior charting. Metrics report persisted end-state, not rich emergent narrative unless profiles are tuned.

## Two-operator transcript (reproducible)

Both operators start at the Coffee Shop. Ada charts a new birth cell west of the gate and posts a theory. Bob crosses into that cell, reads history on the ground, and reads the theory.

```text
# Ada
> go up
> outskirts
> west          # repeat until a new birth cell opens (9× west from gate tile in tests)
> inspect ground
(Ada is first charter — no “first charted by …” for herself)
> theory post theory survey-mark The relay crew closed the survey here.

# Bob (same path depth)
> go up
> outskirts
> west          # same count as Ada
(move feed must NOT contain “first charted”)
> west          # enter Ada’s birth cell
> inspect ground
… This district was first charted by Ada.
> theory list survey-mark
> theory read 1
(overlay shows Subject: survey-mark and Ada’s text)
```

After server restart, SQLite reload preserves `entered`, theories, and the same inspect/read paths ([`charting_and_theories_survive_reopen`](../../crates/coffee-world/src/store/store_tests.rs)).

## Tests added

| Scenario | Test |
| --- | --- |
| Prior charting on inspect, not every step | `second_operator_sees_first_charting_on_inspect_ground_not_every_step` |
| No fabricated prior explorers | `first_charter_inspect_ground_does_not_fabricate_prior_explorers` |
| Public theories on inspect, journal private | `inspect_surfaces_public_theories_without_leaking_private_journal` |
| Generalized subjects / competing views | `explicit_subject_and_competing_theories_coexist` |
| Delisted-floor legacy | `an_observation_and_a_theory_are_readable_by_name` |
| Persistence | `charting_and_theories_survive_reopen`, `field_discovery_corroboration_survives_reopen` |
| Invalid input | `invalid_subject_and_missing_entry_are_rejected` |

## Remaining limitations

- Charting line can repeat on every `inspect ground` (no per-operator `chart-hint:` flag wired yet).
- Theory inference from location does not cover every wilderness landmark without an authored `content_key`.
- Sim agents do not yet optimize for history/theory metrics.

## Recommended next milestone

Wire optional one-time charting hints per operator per birth cell, and extend simulation profiles with weighted `inspect ground` / `theory post about …` at authored tiles so reports can compare runs where operators actually leave interpretable traces.

## Validation

```bash
cargo fmt --all --check
cargo test -p coffee-world
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Record exact counts in the PR body when CI completes.
