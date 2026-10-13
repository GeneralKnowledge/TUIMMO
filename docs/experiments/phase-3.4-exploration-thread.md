# Phase 3.4 — Exploration has a point (Ward Clinic thread)

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `647be2157ae5dfca3942791315bb2d59aac77da5` |
| **Resulting commit** | `4f8f51b` |

## Baseline playthrough and weakness

**Verified on main:** The near-Hub **Ward Clinic** environmental thread already existed in `geography/density.rs` (directory `(6,10)`, clinic `(5,12)`, corridor `(6,12)`) with command-path coverage in `ward_clinic_story_is_discoverable_without_a_quest_marker`.

**Weakness:** The three sites read as disconnected set dressing. Each line was plausible, but a player could not reliably infer a **single paradox** — official closure versus evidence that the service corridor stayed in use and the seal was abandoned. The thread was also undocumented in `docs/game-design.md` (unlike the survey and delisted-floor chains), so design intent was invisible outside the code.

No new tiles, generator changes, or persistence schema were required.

## Mystery design

**Subject:** Ward Clinic near the Hub — closure paperwork vs physical traces.

| Stage | Site | Role |
|-------|------|------|
| Anomaly | Lobby directory | CLOSED stamp vs handwritten strip sending clerks to the entrance for records |
| Corroboration | Clinic door | Re-posted corridor warning on tape newer than the closure order |
| Unresolved conclusion | Service corridor | Half-finished seal, scuffed dust, truncated work order; who used the passage and why the seal stalled stays unknown |

**Player inference:** The clinic was closed on paper while staff (or someone) kept moving records and using the east corridor; the brick seal was started and abandoned.

**Deliberately unknown:** Identity of later users, whether use was authorised, and why the work order stopped mid-sentence.

## Player transcript (command path test)

Route: gate → west → north → west → north (directory) → north → north → west (clinic) → east (corridor).

```
> outskirts
> west
> north
> west
> north
(look — Ward Clinic directory, CLOSED vs handwritten records strip)
> inspect directory
… CLOSED in heavy ink … corridor job finishes … do not agree on whether the building is still in use.
> north
> north
> west
> inspect clinic
… tape looks newer … corridor kept seeing traffic.
> east
> inspect corridor
… scuff marks … seal never finished … not written on the scaffold.
```

No quest completion message. Optional `journal record` at each site.

## Determinism and privacy

- Story sites are pure functions of `(region 0,0, local x,y)`; prose is stable across revisits (tested by re-inspecting corridor after stepping away).
- Does not read or expose other operators' journals, waypoints, or signs.
- Phase 3.3 sign/trail privacy tests unchanged.

## Files

| File | Change |
|------|--------|
| `crates/coffee-world/src/geography/density.rs` | Look/inspect/journal notes; coherence unit test |
| `crates/coffee-world/src/state/journal_cmds.rs` | Revisit + scuff assertions on playthrough test |
| `docs/game-design.md` | Ward Clinic closure section |
| `docs/experiments/phase-3.4-exploration-thread.md` | This note |

## Validation

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` | Pass |
| `cargo test -p coffee-world ward_clinic` | Pass (2 tests) |
| `cargo test -p coffee-world` | **189** passed |
| `cargo test --workspace` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |

## Limitations

- Thread is localized to birth region `(0, 0)` near the Hub.
- Discovery order is not enforced; west-from-gate flood still blocks naive north routes.
- No new map legend beyond existing urban/story presentation.

## Next milestone

**Phase 3.5 (player experience):** A second compact environmental thread in the **fringe or deep** band using the same `StorySite` pattern — only if playtests show near-Hub clinic saturation; otherwise deepen delisted-floor / survey cross-links in presentation only.
