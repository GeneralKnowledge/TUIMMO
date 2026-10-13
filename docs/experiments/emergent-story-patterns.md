# Emergent story patterns in TUIMMO

This note maps common environmental-narrative techniques to **mechanisms already in the codebase**. It is not a quest design doc and does not introduce new systems.

## Principles

1. **Evidence, not exposition** — Players infer from inspectable props; no NPC lecture or quest log.
2. **Shared vocabulary** — Proper nouns and facts repeat across independent sites (Line 8, Ward West, B-4) so discovery order stays flexible.
3. **Diegetic uncertainty** — When inference stops, say so in-world (“not written on the scaffold”).
4. **Separate voices** — System-authored overlays (`StorySite`, `TraceKind`, stations) vs player structures (`sign`, `trail`) vs private notes (`journal`, `waypoint`).
5. **Determinism** — Overlays are pure functions of coordinates (and optional operator flags for depth); revisiting must not rewrite history.
6. **No mechanical reward** — Threads do not grant items, open doors by themselves, or block exploration.

## Pattern → implementation

| Pattern | TUIMMO mechanism | Examples |
|---------|------------------|----------|
| Forensic clue graph | Authored inspect text + cross-links | Line 8 (metro + timetable), delisted floor (panel + terminal) |
| Fixed authored sites | `StorySite` in `density.rs` | Ward Clinic directory / clinic / corridor |
| Scattered texture | `TraceKind` via `trace_at` | Timetables, notices, mailboxes |
| Hub / Undernet chains | `undernet` objects + flags | Survey stone, archive cipher |
| Flag-gated depth | `operator_flags`, items | `survey-mark`, `read-terminal` |
| Player public traces | `world_structures` sign/trail | Phase 3.3 discovery |
| Private knowledge | `region_discoveries` field rows | `journal record` |
| Shared beliefs | `shared_theories` | `delisted-floor` topic only |
| Variety without RNG visits | `prose_variant` + `mix_coords` | Ordinary Squalor look lines |

## Anti-patterns (avoid)

- Global “new clue” notifications or waypoints to the next beat.
- Publishing private journal text into public inspect lines.
- Generator or fingerprint changes for a short authored thread.
- LLM-generated clue text at runtime.

## Related player docs

- [game-design.md](../game-design.md) — survey, Ward Clinic, Line 8, delisted floor sections
- [setting-and-subway.md](../setting-and-subway.md) — subway data model
- [world-architecture.md](../world-architecture.md) — urban density overlays
