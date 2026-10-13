# Phase 3.1 — Persistent homes & shared-world traces

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `ab62609a4fc86d240f7b2f74a6a35e39b893be5a` |
| **Resulting commit** | (branch tip after merge) |

## Previous phase verified (3.0 on main)

Implemented and tested:

- Private arrival line when a **meaningful return** counts (`HomeTravelNote`, movement emit).
- **`home` overlay** shows counted returns toward settling.
- Tests: `meaningful_return_notifies_owner_on_arrival`, 181 `coffee-world` tests at merge.

Not done in 3.0: abandoned trace depth, visitor-facing history, `public_description` command.

## Verified gap (this phase)

**Persistent world / visitor:** Abandoned homes used one generic line per stage regardless of whether the occupant had **just claimed** or **returned many times**. Persisted `meaningful_returns` and claim tenure were ignored in `look` prose — the world forgot how lived-in a place had been.

## Selected slice

**Derive abandoned-home `look` text from durable row data** (`claimed_at`, `abandoned_at`, `meaningful_returns`, public name only). Three trace tiers (faint / moderate / deep) for **recently abandoned** and **forgotten** stages. No migration, no private discovery content.

### Acceptance criteria

- Visitor `look` differs for short-lived vs return-rich abandoned homes.
- No journal/waypoint/private keys in abandoned lines.
- Active homes unchanged; lifecycle stages and absorption unchanged.
- Store reload preserves behaviour (fields already persisted).

## Before / after (visitor `look`, recently abandoned)

**Before (all recent abandons):**

> This place was inhabited until recently. Signs of a kept room remain among the Squalor's dust.

**After — faint claim (0 returns, short tenure):**

> This place was left behind recently. A corner had been cleared, but it never quite settled into a kept room.

**After — deep history (≥3 returns, from test `abandoned_home_trace_reflects_persisted_returns_for_visitors`):**

> This place was inhabited until recently once called "Dust Shelf". It had been kept — dust held back, a path worn in from returns.

## Files

| File | Change |
|------|--------|
| `crates/coffee-world/src/home.rs` | `LivedInTrace`, abandoned look helpers |
| `crates/coffee-world/src/state/home_cmds.rs` | Integration test; lifecycle assert flexibility |
| `docs/game-design.md` | Abandoned trace note |

## Validation

Run at merge: `cargo fmt --all --check`, `cargo test -p coffee-world`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`.

## Simulator

No changes. Future experiment idea: mixed wanderer/homebody co-location — documented only.

## Next step

**Player experience:** First-time `home claim` hint on eligible tiles (discoverability without a new mechanic). **Not** simulator work until TUI onboarding is clearer.
