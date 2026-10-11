# The Coffee Shop

## Identity

- **ID:** `coffee-shop`
- **Display name:** The Coffee Shop
- **Region:** Undernet
- **Type:** Social hub, ground floor of a small shop
- **Narrative purpose:** Neutral room where operators meet, speak, and start out. The return point between trips upstairs or into the alley.
- **Access:** Public. New operators are created here. The game does not lock the doors.

## Player experience

**Implemented.** Steam, cheap ceramic, and a row of terminals that still boot. The windows look onto wet neon. A stair behind the counter goes up. Other operators in the room are visible and hear `say`. The job board offers nothing you can take.

**Interpretation.** The room should feel staffed a minute ago: cups out, a lamp on, laminate warm from elbows. It is improvised, not a themed café. Players stay because other people are here and because the alley is the only way onward.

Memorable pieces: the carved cup in the counter, the paper job board, the stair that smells like coffee on the way back from the roof.

## Spatial layout

**Interpretation**, chosen so the implemented exits stay true. The game does not store metres or wall faces.

- About 8 m wide and 6 m deep. One room. Ceiling low enough to feel like a shop, not a hall.
- **North:** dirty glass onto the street. Not an exit. Neon is outside.
- **East:** door, keyword `alley`, to `neon-alley`.
- **South:** counter. Stair behind it, keyword `up`, to `roof-array`.
- **West:** solid wall. No service door.
- **Landmarks:** counter, job board, the bootable terminal row (scenery, not an object).
- **Furniture:** a few small tables and mismatched chairs. Not inspectable.
- **Hidden areas:** none.

Sightline: from the alley door you see the counter and the windows. The stair is partly behind the counter, which matches "a stair behind the counter."

## Existing game behaviour

| Keyword | Kind | Destination or result |
| --- | --- | --- |
| `alley` | Exit | `neon-alley` |
| `up` | Exit | `roof-array` |

Return exits: from the alley, `shop`; from the roof, `down`.

| Keyword | Aliases | Name | Grant |
| --- | --- | --- | --- |
| `counter` | `bar` | counter | none |
| `board` | `jobs` | job board | none |

Counter copy: scarred laminate, a tiny cup carved in the corner. Board copy: faded cards for courier work and lost keys. None of them are for you, yet.

New characters carry a loyalty chip. It is not in this room. It opens nothing.

People here share local chat and see each other arrive and leave. Position is saved on move.

## Visual continuity

- Amber practical light, ceramic, laminate, wet glass, teal night outside. Mint only on a screen that is actually on.
- The carved cup and the paper board must stay. They are implemented objects.
- The east door aligns with the west end of Neon Alley. The stair aligns with the roof hatch.
- Do not add a street door, a basement, a back office exit, or a path from the windows to the roof.
- Do not depict a working kitchen pass or a staff-only lock. Neither exists.

## Images

- `concept.png` — atmosphere, from inside, looking toward the windows and the counter stair.
- `floorplan.png` — top-down plan using the style-guide marks. No words on the drawing.

Legend for the floorplan:

| Mark | Meaning |
| --- | --- |
| North double line | Windows, not an exit |
| East triangle | `go alley` |
| South stair ticks behind the counter block | `go up` |
| Circle on the counter | `inspect counter` |
| Circle on the east wall | `inspect board` |
| Small rectangles under the windows | Terminal row, scenery only |

## Unresolved questions

- Which compass face is "east" is interpretation. The code only has `alley` and `up`.
- The job board's cards are not items. Making one of them a lead would be a new mechanic.
- Whether anyone works the counter is unstated. No NPC is implemented. Art should not depend on a barista.
- `concept.png` puts the stair on the left wall. This document and `floorplan.png` put it behind the counter, south. The floorplan is the spatial reference. The south triangle on that plan is `go up`, not a third door.
- Tables on the floorplan are scenery. They are not inspectable objects.
