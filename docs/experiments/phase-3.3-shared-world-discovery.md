# Phase 3.3 — Shared-world discovery vertical slice

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `b263881243123e66694a7c80945862165f184c8b` |
| **Resulting commit** | `e0a066b` |

## Baseline audit

### Existing public trace systems

| Mechanism | Author command | Persistence | Visitor encounter | Notes |
|-----------|----------------|-------------|-------------------|-------|
| **Sign** | `sign <text>` | `world_structures` (`kind=sign`) | `look` prose + map `S` | Full text in look line |
| **Trail post** | `trail <name>` | `world_structures` (`kind=trail`) | `look` prose + map `T` | Name is the public mark |
| **Theory** | `theory post …` | `shared_theories` | `theory list` (global) | Not tied to a place |
| **Noticeboard** | `board post` | capped notices | `board` in Coffee Shop | Hub-only, expires |
| **Marker** | `mark <name>` | structure row | `look` | Personal name tag |

Signs and trail posts already satisfy persistence, offline authors, and place-bound discovery via **`look`**.

### Verified gap

**Presentation / inspect affordance:** `look` already quotes another player's sign, but **`inspect sign` and `inspect trail` did not work** as targets — only the sign's exact text (object keyword) matched. New explorers who use `inspect` habitually (like Hub objects) got *"You see nothing called 'sign'."* despite visible prose. That broke the "evidence of another person" moment for a common command path.

No new trace type, schema, or simulator work was required.

## Selected change

1. **`look` objects list** — generic keywords `sign` and `trail` when those structures are on the tile (`geography/travel.rs`).
2. **`inspect sign` / `inspect trail`** — dedicated public read prose matching `sign read` tone (`inspect_cmds.rs`).

## Player journey

**Player A (ada):** Leaves the Hub, reaches walkable Squalor (tile 8,9 north of gate), runs `sign Relay gap worth a look`. Optional private journal/waypoint at other places stays private. Ada disconnects.

**Player B (bob):** Independently reaches the same tile, **`look`** shows *A sign reads "Relay gap worth a look". ada left it.*, **`inspect sign`** repeats the message in the feed. No friendship or co-presence required.

**World reload:** Sign row survives `Store` reopen; bob still reads it (`public_sign_survives_reopen_for_a_late_discoverer`).

## Privacy guarantees

- Bob sees only the sign text ada published.
- Bob cannot `journal read` ada's private field note (`No journal entry matches`).
- Bob's `waypoint list` does not show ada's private waypoint note.
- Regression: `public_sign_discovered_by_another_operator_without_private_leaks`.

## Command transcript (from tests)

```
# Ada
> go up
> journal record
> outskirts
> north
> sign Relay gap worth a look
> waypoint add camp-watch
> waypoint note camp-watch Ada-only ridge tally
(disconnect)

# Bob (same tile, independent path)
> look
… A sign reads "Relay gap worth a look". ada left it.
> inspect sign
Sign by ada: "Relay gap worth a look"
> journal read survey-scratch
No journal entry matches 'survey-scratch'.
```

**Negative:** bare tile `inspect sign` → *You see nothing called 'sign'.*

## Files

| File | Change |
|------|--------|
| `crates/coffee-world/src/geography/travel.rs` | `sign` / `trail` inspect keywords in scene objects |
| `crates/coffee-world/src/state/inspect_cmds.rs` | `inspect_public_trace_at` |
| `crates/coffee-world/src/state/world_rules_tests.rs` | Two-player discovery + privacy tests |
| `crates/coffee-world/src/store/store_tests.rs` | Reopen persistence test |
| `docs/game-design.md` | Homesteads/traces note |
| `docs/experiments/phase-3.3-shared-world-discovery.md` | This note |

## Validation

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` | Pass |
| `cargo test -p coffee-world public_sign` | Pass |
| `cargo test -p coffee-world inspect_sign_requires` | Pass |
| `cargo test -p coffee-world` | **188** passed |
| `cargo test --workspace` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |

## Limitations

- Discovery still requires reaching the tile (no global ping).
- Trail posts expose only the chosen name, not long prose.
- Theories remain discoverable via `theory list`, not wilderness exploration.

## Next milestone

**Phase 3.4 (player experience):** Optional one-time hint when `look` first shows another operator's sign or trail (mirroring `hint-inspect` / `hint-home-claim`) — only if playtests show players still miss public traces.
