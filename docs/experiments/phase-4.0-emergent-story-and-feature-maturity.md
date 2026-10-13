# Phase 4.0 — Emergent story and feature maturity

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `b263881243123e66694a7c80945862165f184c8b` |
| **Resulting commit** | `4ee5fdf8419ebb07ebceb44b2b6f0d50c0b8542a3` |

## Maturity table

| System | What works now | Incomplete | Consequence for the player | Recommended next step |
| --- | --- | --- | --- | --- |
| Squalor geography | Deterministic v1 regions, urban density overlays, landmarks, cardinal travel, `map`, metro seams | No player-facing “first charted this cell” line | Exploration feels solo even when someone else opened the birth cell first | **Disconnected:** one-time attribution when entering a region another operator materialized |
| Authored discoveries | Survey chain, Ward Clinic sites, delisted floor, landmarks, Black Archive gating | Mysteries do not react to other operators’ private journals (before this phase) | Parallel discoveries feel unrelated until someone posts `theory` | **Disconnected → addressed:** corroboration headcount on inspect / after record |
| Journal / waypoints / signs | Private journal, waypoints with hints, signs (replace/remove/read), trail posts | Trails are create-only; `sign read` duplicates look | Trails cannot be corrected; signs slightly redundant | **Shallow:** trail remove/replace parity with signs |
| Shared theories | Persistent board, one topic (`delisted-floor`), rate/cap limits | Single topic; no link to place or journal | Delisted mystery has a public layer; other mysteries do not | **Intentionally limited** for now; add topics per mystery or place-scoped boards |
| Personal homes | Claim, habitation, distinctive traits, abandoned traces, claim hint | No visitor-facing link between home and owner’s journal | Homes read as lived-in but not as story hubs | **Disconnected:** optional public home description (deferred in 3.1) |
| Homesteads / settlements / workshops | Cabins/workshops, access/share, settlement boards, claims | No settlement-wide remote chat (by design) | Groups share places, not a live channel | **Intentionally limited** |
| Social | say/chat/region, mail, friends, ignore, profiles | No global “presence at discovery” | Social layers are separate from exploration evidence | Connect corroboration to optional `theory` prompts later |
| coffee-sim | Profiles, compare/scale, snapshots, occupancy | Ground-only `inspect`; no pathfinding home | Metrics skew to movement volume; rare landmark corroboration in default mixes | Add named inspect weights only when measuring a specific mystery |
| Persistence / privacy | SQLite transactions, journal per-actor, theory/mail caps | Session tokens ephemeral | Restart-safe world; journals stay private | Keep corroboration aggregate-only (this phase) |

Gap tags used above: **Broken** (none found blocking this work), **Disconnected**, **Shallow**, **Missing**, **Intentionally limited**.

## Stale roadmap corrected

[`README.md`](../README.md) previously listed workshops/settlements and invite-only registration as future work; both are implemented (`0005_sharing_and_workshops.sql`, `0012_account_invites.sql`, `docs/game-design.md`). Next milestones now list player-impact gaps from this audit.

## Selected problem

Multiple operators can persist the same field-journal `content_key` in `region_discoveries` (`kind = field`), but until this phase nothing in `look` / `inspect` told a later visitor that independent documentation existed. The only public interpretation layer was voluntary `theory` on `delisted-floor`.

## Evidence

- [`journal_cmds.rs`](../../crates/coffee-world/src/state/journal_cmds.rs) tests already assert two rows for `survey-mark` with distinct row ids.
- [`ensure_region`](../../crates/coffee-world/src/state/mod.rs) writes `entered` rows with `actor_id`, but no command surfaces them ([`game-design.md`](../game-design.md) calls this shared history).
- Player signs and trail posts are fully public on `look`; private parallel notes were invisible across operators.

## Implementation

- [`GeographyState::field_note_recorder_count`](../../crates/coffee-world/src/geography/records.rs) — distinct recorders per `content_key`.
- [`corroboration_inspect_line`](../../crates/coffee-world/src/geography/field.rs) / [`corroboration_record_line`](../../crates/coffee-world/src/geography/field.rs) — copy only when count ≥ 2.
- [`inspect_cmds.rs`](../../crates/coffee-world/src/state/inspect_cmds.rs) — append on authored wilderness / static inspect targets that map to a journal key.
- [`journal_cmds.rs`](../../crates/coffee-world/src/state/journal_cmds.rs) — private feed line after successful record when count ≥ 2.
- **Sim:** [`WorldOutcomeSnapshot::corroborated_field_keys`](../../crates/coffee-world/src/sim.rs) and [`WorldReport.corroborated_field_keys`](../../crates/coffee-sim/src/report.rs) — count of keys with ≥ 2 recorders at end of run.

No migration, no generator fingerprint change.

## Deferred alternatives

1. **First-chart attribution on region entry** — Data exists (`entered` rows) but showing it once per operator needs new “seen” state or risks feed spam on every border step.
2. **Trail/sign parity or auto `theory` after `journal record`** — QoL or duplicates the theory board; does not connect existing silent multi-operator field rows to exploration.

## Verified command-path transcript

Harness: `pair()` test world, survey stone (tile west of Hub gate). Commands and outcomes match [`survey_mark_corroboration_without_leaking_journals`](../../crates/coffee-world/src/state/journal_cmds.rs).

```
> go up
> outskirts
> west
(Ada at survey stone)

> inspect mark
(Inspection prose; no corroboration line — only one recorder so far)

> journal record
Recorded in your journal as survey-mark. …
(No “not the only one” line yet)

(Bob repeats movement to the same tile.)

> journal record
Recorded in your journal as survey-mark. …
You are not the only one — 2 operators have recorded this discovery in a private journal.

> inspect mark
…
2 operators have kept a private field note for this discovery.

> journal read survey-scratch
No journal entry matches 'survey-scratch'.   (Bob cannot read Ada’s unrelated entries)
> journal read survey-mark
(Bob’s own entry only in overlay)
```

**Inference:** Others documented the same discovery; their wording and paths stay unknown. **Unknown:** Who they are, what they concluded, whether they posted `theory`.

## Persistence, determinism, privacy

- **Persistence:** [`field_discovery_corroboration_survives_reopen`](../../crates/coffee-world/src/store/store_tests.rs) — commit two `survey-mark` rows, reopen SQLite, `inspect mark` still shows count 2.
- **Determinism:** Count is derived from loaded `discoveries` rows only; no `Date.now()` in queries.
- **Privacy:** No journal title, note, or author name in corroboration lines; `journal read` remains per-operator.

## Simulation metrics

| Metric | Definition | Limitation |
| --- | --- | --- |
| `corroborated_field_keys` | At run end, number of `content_key` values with ≥ 2 distinct field recorders in `GeographyState` | Does not measure fun or SSH capacity; default sim rarely performs named landmark inspect, so this stays low unless investigators stack on the same authored tile |

## Tests and validation

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| `cargo test -p coffee-world corroboration` | 3 passed |
| `cargo test -p coffee-world` | 188 passed |
| `cargo test -p coffee-sim` | 22 passed |
| `cargo test --workspace` | All crates passed (integration tests included) |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |

## Remaining high-priority gaps (player impact)

1. Show who first materialized a birth cell (`entered` rows) — one line, once per operator per region.
2. Extend shared theories to other authored mysteries (or place-linked topics).
3. Deeper sim actions (`inspect <object>`) when measuring mystery engagement.
4. Trail post maintenance (remove/replace) for long-lived homestead tiles.
