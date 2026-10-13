# Phase 3.0 — Home experience audit & vertical slice

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `c05e78a8eb9835a845a3d3913397469fa7b69d35` |
| **Resulting commit** | `dfbd9a5` (branch `cursor/phase-3-0-home-experience-e526`) |

## 1. Audit summary

### A. Verified existing capabilities

| Capability | Player action | System effect | Player sees | Verification |
|------------|---------------|---------------|-------------|--------------|
| Claim home | `home claim` on eligible Squalor tile | Inserts active `personal_homes` row, region ensured | Private “You make a home at …”; `look` adds home line | `claim_lists_and_rejects_duplicates_and_protected_places` |
| Name home | `home name …` / `clear` | Updates `public_name` | Overlay + public `look` name | `naming_persists_renames_clears_and_stays_public_only` |
| Move / abandon | `home move confirm`, `home abandon confirm` | Abandon old row, claim new or none | Private system lines; abandoned lifecycle on old tile | `move_requires_confirm_and_is_atomic`, `abandon_requires_confirm…` |
| Habitation stages | Time away + returns + exploration | `meaningful_returns`, claim age; read-time traits | `home` Condition + blurb; `look` detail by stage | `habitation_advances_by_time_returns_and_exploration` |
| Returns counting | Leave home ≥10 min, arrive via movement | Increments `meaningful_returns`, persists | **Before slice:** no arrival feedback; counter only in DB | `note_home_travel` tests |
| Privacy | Claim at tile with journal/waypoints | No copy to home row | Others see generic/distinctive public prose only | `private_waypoints_and_journals_stay_private…` |
| Persistence | Restart / reload | SQLite `personal_homes` + habitation columns | Same home after reconnect | `personal_homes_survive_reopen…`, `acceptance_lifecycle…` |
| Abandoned places | Other player `look` | Row kept with `abandoned_at` | Staged “recently” / “forgotten” / absorbed | `lifecycle_presentation…`, store tests |

### B. Verified experience gaps (confirmed)

1. **Meaningful returns were silent** — counter and habitation advanced, but arrival only showed generic “You step into …”.
2. **`home` status hid return progress** — Condition text did not say how many returns had counted or what to do next.
3. **`public_description` column unused** — no player command (deferred; not selected for this slice).
4. **Guide pages** omit move/abandon/habitation (help text in `home help` is richer than explore guide).
5. **Simulator** does not exercise naming/habitation feedback (acceptable for Phase 3.0 scope).

### C. Chosen improvement

**Make meaningful returns legible:** private arrival acknowledgement on counted return + explicit return progress on `home`.

**Why this slice:** Phase 2.6 proved homebodies claim and return; the weakest link in the player journey is **feedback** when those returns matter for settling. No new mechanics, counters, or economy — only surfacing existing habitation rules through normal movement and `home`.

## 2. Implementation

| File | Purpose |
|------|---------|
| `crates/coffee-world/src/home.rs` | `meaningful_return_message`, `settlement_progress_line`; status overlay includes progress |
| `crates/coffee-world/src/state/home_cmds.rs` | `HomeTravelNote`; return message when count increments; help line; test |
| `crates/coffee-world/src/state/movement_cmds.rs` | Emit owner message on meaningful return arrival |
| `docs/game-design.md` | Document arrival line + return count on `home` |

## 3. Before / after player transcript

**Before (representative — return counted, no home-specific line):**

```
> west
You step into …
> east
You step into …
> home
Condition: Settling in
Recently claimed. Time and returns from elsewhere will settle it.
```

**After (from test `meaningful_return_notifies_owner_on_arrival`):**

```
> home claim
You make a home at …
> west
You step into …
> east
You step into …
You return home after a real stretch away. The place is beginning to feel like yours.
> home
Counted returns from expeditions: 1 (three returns deepen the settled feeling).
Condition: Settling in
…
```

## 4. Tests and validation

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` | Pass |
| `cargo test -p coffee-world` | **181** passed |
| `cargo clippy -p coffee-world --all-targets -- -D warnings` | Pass |
| `cargo test --workspace` | Pass (run at merge) |
| `cargo clippy --workspace --all-targets -- -D warnings` | (run at merge if required by CI) |

New test: `meaningful_return_notifies_owner_on_arrival` (claim → west → wait → east → feed + `home` overlay).

## 5. Privacy, persistence, lifecycle

- Return messages are **Audience::Player** only (same as claim/name).
- Habitation ops still use `PersistOp::HomeHabitation`; no schema change.
- Move/abandon/claim flows unchanged; no duplicate state.

## 6. Simulator

No simulator changes. Existing metrics (`home_returns`, `home_claims_active`) still observe the same behaviour; arrival text is not simulated in JSON reports.

## 7. Remaining limitations

- `public_description` still unused.
- Explore guide still brief vs `home help`.
- Habitation on live server uses wall clock (documented elsewhere).
- No new cross-player home interaction beyond existing public `look`.

## 8. Recommended next phase

**Player experience:** Two-profile shared world at one tile density — e.g. guided onboarding that walks through claim → name → first counted return (in-game hints tied to `home help`).

**Simulator (later):** Mixed wanderer/homebody at fixed N after onboarding copy is stable — co-location only, not a substitute for TUI legibility.
