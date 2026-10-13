# Phase 3.2 — Discoverable personal homes

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `89830ac484e5f1b7513e9cd77d32ac7d628aac46` |
| **Resulting commit** | `b42e05f` |

## Verified gap

Phases 3.0–3.1 improved **return feedback** and **abandoned-home traces**. A player could still walk eligible Squalor tiles without learning that **`home claim`** exists: `home` and `help home` explain the command, but only after the player already knows to ask.

## Selected solution

**One private system line on first arrival at claim-eligible ground**, using the same pattern as `hint-inspect`:

- Flag `hint-home-claim` via existing `PersistOp::Flag` / `operator_flags` (no migration).
- Hook: `move_through` after a successful step, only when `home_claim_eligible` (no active home + `validate_home_destination` succeeds).
- Does not run on `look`, so repeated inspection does not spam the feed.
- Does not block movement or require confirmation.

**Why this fits:** Surfaces an existing mechanic at the moment exploration makes it relevant, without quests, map markers, or new progression.

## Acceptance criteria

| Criterion | Verification |
|-----------|----------------|
| Eligible ground shows hint once | `home_claim_hint_on_first_eligible_arrival_only` |
| Ineligible ground (e.g. gate step) | `home_claim_hint_absent_when_ineligible_or_home_exists` |
| Existing home — no second-home prompt | same test |
| Optionality — no auto-claim | hint test asserts no home until `home claim` |
| Normal play continues | movement + claim in hint test |
| Noise control | re-entering same tile does not repeat line |
| Persistence | same `operator_flags` path as `hint-inspect`; flag set in transition ops |
| Regression | full `coffee-world` + workspace tests |

## Before / after player transcript

**Before:** Arriving on eligible plaza tile only showed movement prose (no mention of `home claim`).

**After (actual output from command-path harness, `pair()` test world — movement from tile 6,8 east to survey tile 7,8):**

```
> east
You step into open street — civic, plaza.
The ground here could become a home, if you wanted one. `home claim` would mark this place as yours — optional, and not a cabin.
> west
> east
(one hint total — no repeat)
> home claim
You make a home at Squalor, civic quarter, plaza, near the Hub, birth (0, 0), tile 7,8.
> home
Your home
Place: Squalor, civic quarter, plaza, near the Hub, birth (0, 0), tile 7,8
Condition: Unsettled
…
```

**Hint correctly absent:** stepping east from the survey tile onto the **Hub gate** tile (`home_claim_hint_absent_when_ineligible_or_home_exists`); also after the player already has an active home.

## Files

| File | Purpose |
|------|---------|
| `crates/coffee-world/src/state/guide.rs` | `HINT_HOME_CLAIM`, `attach_home_claim_hint` |
| `crates/coffee-world/src/state/home_cmds.rs` | `home_claim_eligible`; hint tests |
| `crates/coffee-world/src/state/movement_cmds.rs` | Call hint after arrival |
| `docs/game-design.md` | Document `hint-home-claim` |
| `docs/experiments/phase-3.2-home-discoverability.md` | This note |

## Validation

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` | Pass |
| `cargo test -p coffee-world home` | Pass (23 tests) |
| `cargo test -p coffee-world` | Pass (185 tests) |
| `cargo test --workspace` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |

## Privacy, persistence, lifecycle

- Hint is `Audience::Player` only (same channel as inspect hint).
- No change to claim/move/abandon, habitation counters, abandoned traces, or simulator.
- Flag is per-operator; abandoning a home does **not** clear `hint-home-claim` (same as `hint-inspect` — no re-tutorial spam).

## Known limitations

- If a player sees the hint but never claims, they are not reminded on later visits (by design).
- After `home abandon`, the hint does not reappear even if they are homeless again (would need flag removal — out of scope unless product asks).
- Hint fires on **arrival**, not on first `look` at a tile reached by other means (if any exist).

## Next milestone

**Phase 3.3 (player experience):** Optional polish on `help explore` / guide pages to mention personal homes alongside cabins and settlements — still without a forced tutorial or new mechanics.
