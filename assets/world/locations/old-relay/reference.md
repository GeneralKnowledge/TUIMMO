# The Old Relay

## Identity

- **ID:** `old-relay`
- **Display name:** The Old Relay
- **Region:** Undernet
- **Type:** Derelict communications room
- **Narrative purpose:** The technical find. A loose cipher plate opens the Black Archive. A separate floor passage leads back toward the abandoned exchange.
- **Access:** Public to enter. The archive hatch is restricted.

## Player experience

**Implemented.** Racks stand open. One relay sled is still seated, its cipher plate latched but loose. A dead dish has a dark status lamp and a cable that runs into the floor. Exits before the plate: `tunnels` and `floor`. After `inspect relay`: those two, plus `archive`.

The room should feel neglected and still partly alive: one warm plate, everything else dark. It is not a ruin you cannot use, and it is not a staffed NOC.

Memorable pieces: the warm plate, the dark dish, the fact that `floor` and the sealed hatch are different holes.

## Spatial layout

**Interpretation.** Room size about 10 m by 8 m. The relay is not inside the coffee shop. Roof copy places it a few rooftops away; the walk is the tunnel or the long way through the exchange basement. No distance is simulated.

- **West:** door, keyword `tunnels`, to `cooling-tunnels`.
- **Floor, south side:** an open passage, keyword `floor`, to `abandoned-exchange`. Always available. This is not locked and not hidden.
- **East rack row:** the relay sled, keyword `relay`. Taking the plate does not remove the sled from the room. A later inspect says the housing is empty.
- **North or corner:** the dead dish, keyword `dish`. The cable dropping into the floor is scenery on that object. It is not itself the `floor` exit, but the art may show the cable and the passage near each other.
- **Sealed hatch:** keyword `archive`, to `black-archive`. Off the exit list until the flag `cipher-plate` is set. Guessing the word before that reports the seal. After the plate, it is a normal exit. The return from the archive is `relay`.

Do not combine the floor passage and the archive hatch into one opening.

Elevation is ambiguous: `floor` is not paired with `up`. A short drop or a slope is allowed. A shared flat floor with the abandoned exchange is not required.

## Existing game behaviour

| Keyword | Kind | Destination or result |
| --- | --- | --- |
| `tunnels` | Exit | `cooling-tunnels` |
| `floor` | Exit | `abandoned-exchange` |
| `archive` | Exit, flag `cipher-plate` | `black-archive` |

Without the flag, `go archive` fails with: "The way to the Black Archive is sealed. The lock wants a cipher plate." The exit is omitted from `look`. The in-game map shows `(sealed)` in that slot until the flag is set.

| Keyword | Aliases | Name | Grant |
| --- | --- | --- | --- |
| `relay` | `sled`, `plate` | relay sled | flag `cipher-plate`, item `cipher-plate` |
| `dish` | `antenna` | dead dish | none |

First inspect of the sled: "The plate comes free in your hand. The sealed hatch in this room will answer it." Repeat inspect: the housing is empty and you already carry the plate.

Plate item detail: "A warm metal plate etched with a route cipher."

Dish copy: the status lamp is dark and the cable runs down into the floor. No grant.

The roof antenna pointing here is flavor on `roof-array`. It is not an exit.

## Visual continuity

- Open steel racks, dust, one warm metal plate, a dark dish or feed horn, concrete, almost no neon. Amber is a small warm point on the plate or a work lamp, not a shop interior.
- Teal and mute grey dominate. The mint accent is absent unless a single LED is clearly dead-dark instead.
- The tunnel door should feel like the end of the cooling tunnels: round concrete remembered, not a street.
- The archive hatch reads as a locked panel in the room. Before the plate it can look like equipment. It must not be drawn as the mouth of the floor passage.
- Do not show a staffed console, a map table of the whole city, or a door onto a roof.

## Images

- `concept.png` — interior, open racks, sled with a loose warm plate, dark dish, tunnel door and a separate floor opening. The archive hatch may appear as an unmarked panel.
- `floorplan.png` — top-down, no words.

Legend:

| Mark | Meaning |
| --- | --- |
| West triangle | `go tunnels` |
| South gap | `go floor` |
| Dashed hatch on the east wall | `go archive`, hidden until the plate |
| Circle in the east racks | `inspect relay` |
| Capped circle, north | Dish landmark. Not an object by itself |
| Small circle beside that dish | `inspect dish` |

## Unresolved questions

- Whether `floor` is a hatch, a stair, or a slope is not in the code. The keyword is only `floor`.
- The plate's "route cipher" is not a puzzle beyond carrying the flag. No combination lock is implemented.
- The archive stacks mention a gap that matches the plate. That sentence is static and is documented on the archive, not as a relay interaction.
- A line-of-sight from the roof is text only. Concept art of the relay interior must not include the coffee shop.
- `concept.png` puts the closed hatch on the same wall as the open tunnel door, and the floor opening in the middle of the room. `floorplan.png` puts `tunnels` on the west, `floor` on the south, and the dashed `archive` hatch on the east. The floorplan is the spatial reference. Both pictures keep three separate openings. Do not merge the floor hole with the hatch, and do not add a fourth exit to reconcile the two drawings.
