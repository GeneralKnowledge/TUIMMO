# Phase 3.5 — Line 8 thread and prose diversity

## Commits

| | SHA |
|---|-----|
| **Starting commit** | `22d793399991105b2ee5544180f3cf01f1366a35` (Phase 3.4 on main) |
| **Resulting commit** | `c259b3f` |

## Baseline weakness

- **Subway:** Mechanics and station fiction existed, but surface metro landings used generic two-line prose; timetables and stations did not share vocabulary. `StorySite` / traces are suppressed on metro tiles in `travel.rs`, so entrance-specific authored lines were required.
- **Flavor:** ~86% of tiles use `ordinary_prose` with a small fixed table — district titles varied, body copy repeated.

## Line 8 environmental thread

**Anomaly:** Ash Plaza surface landing — peeled map stencil, Line 8 west (`subway::entrance_landing_prose`).

**Corroboration:** Ash `inspect diagram`; civic `inspect timetable` trace; Ward West `inspect pillar` (SECTOR 14-W struck through).

**Unresolved conclusion:** Ward West `inspect platform` — maintained edge paint; who kept the line alive and why the map was taken whole is explicitly not written.

## Prose diversity

`UrbanContext::ordinary_prose(terrain, abs_x, abs_y)` selects among 2–3 variants per bucket via `mix_coords` (stable per tile).

## Player transcript (command path)

```
> outskirts
> west (×6 to Ash entrance)
(look — Line 8 stencil / peeled map on landing)
> metro
> inspect diagram
… LINE 8 WEST … peeled away whole …
> west
> inspect pillar
… WARD WEST … SECTOR 14-W … timetables …
> inspect platform
… not written on the platform …
> east
> inspect diagram
(same evidence — stable revisit)
```

Civic timetable corroboration: find a civic-band tile with `inspect timetable` in the origin region (deterministic trace overlay).

## Inference

**Can infer:** Official closure paperwork disagrees with physical use; Line 8 pairing Ash Plaza ↔ Ward West persisted on timetables and stencils after the wall map vanished.

**Unknown:** Who used the corridor/stations afterward; whether maintenance was authorized.

## Tests

- `line_8_environmental_thread_links_surface_station_and_timetable`
- `civic_timetable_inspect_references_line_8_and_missing_map`
- `ordinary_prose_varies_by_coordinate_in_a_sample`
- Regressions: existing subway hop, Ward Clinic, public sign privacy

## Validation

| Command | Result |
|---------|--------|
| `cargo fmt --all --check` | Pass |
| `cargo test -p coffee-world` | Pass (192 tests) |
| `cargo test --workspace` | Pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | Pass |

## Limitations

- Still two stations only; no third stop.
- Timetable corroboration requires finding a civic trace tile (not marked on map).
- Metro tiles still exclude random `trace_at` overlays (by design).

## Next milestone

Optional fringe/deep `StorySite` thread using the same patterns doc; or cross-link Line 8 with Ward Clinic only in player docs, not mechanics.

## Related

- [emergent-story-patterns.md](./emergent-story-patterns.md)
