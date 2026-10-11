# Coffee.shop visual style guide

This guide is for concept art, floorplans, and later locations. The playable world is still the terminal. Art describes what the rooms look like. `crates/coffee-world/src/undernet.rs` decides what is true.

The Undernet is a lived-in underground of one city, not a neon catalogue. People who know machines have taken over rooms that already existed: a coffee shop, a dead trading floor, a cooling duct, a relay hut. Hardware is old because it was left behind, then kept because it still boots.

## Palette

The terminal theme and the physical world share one family. UI colours are implemented in `crates/coffee-server/src/theme.rs`.

| Role | Hex | Use |
| --- | --- | --- |
| Amber | `#FFC457` | Practical lamps, CRT phosphor, coffee heat, the shop sign |
| Paper | `#E2DCCE` | Ceramic, paper, laminate highlights, floorplan ground |
| Warm local | `#ECE2CE` | Close surfaces in social rooms |
| Teal shadow | `#40707A` | Wet concrete, window glass at night, cable jackets |
| Mint screen | `#78DCCC` | One healthy display in a room, not a wash of cyan |
| Global cyan | `#7DD6E6` | Distant neon, rain on a sign, the rare clean light |
| Mute | `#787C84` | Dust, dead metal, unpowered plastic |
| Warn | `#FF965A` | A single fault lamp, a hot grate, hazard tape |

Most of a frame should be concrete, brown-black cable, dirty white, and shadow. Amber and mint are accents. If a picture is more than about a third saturated neon, it has left the setting.

Public rooms may be warmer. Abandoned and restricted rooms drop the amber and keep one failing source: a terminal, a status LED, or light leaking under a door.

## Materials and light

- Concrete, scarred laminate, cheap ceramic, foil, tape, rubber cable, painted steel, dirty glass.
- Rain and condensate are common at street level and in the tunnels. The archive is the dry exception.
- Light is practical: a lamp someone switched on, a screen left awake, a sign buzzing outside. Avoid even studio lighting and floating holograms.
- Glass is grimy. Metal is fingerprinted. Repairs are visible: tape, zip ties, a replaced foot on a chair.
- Technology ages in place. CRTs sit next to a later flat panel because nobody threw the CRT out. Racks are open. Labels are marker, not a corporate font system.

## Motifs that recur

Use these so rooms belong together. Do not put all of them in every room.

- Cable routed by people, not by a cable tray catalogue: bundles, hooks, a run along a skirting board.
- Paper that outlived the system it described: job cards, a torn poster, a crossed-out floor list.
- One object that is still warm or still lit, surrounded by gear that is not.
- A human trace at hand scale: a carved mark, a folding chair, a cup, a foil awning, a taped mast.
- Street neon is outside or down a passage. Interior neon is a spill, not the architecture.

## How spaces differ

| Class | Light | Clutter | Who it is for |
| --- | --- | --- | --- |
| Public | Warm, mixed, a bit of street spill | Cups, paper, chairs, signs of a shift | Anyone who knows the door |
| Private / staff | Task light, narrower | Tools, panels, handwritten lists | People who work the building |
| Abandoned | One surviving source, ash and dust | Furniture shoved aside | Whoever comes down anyway |
| Restricted | Low, dry, even | Almost no personal mess | The lock, not a crowd |
| Hidden | Not shown until the game reveals it | A hatch, a gap, a plate | Operators who found the key |

The Coffee Shop is public. The service corridor is staff space that players can already walk. The abandoned exchange is abandoned. The Black Archive is restricted and, until a cipher plate is carried, hidden from the exit list. The Old Relay is exploration: neglected, but not empty of function.

## Neighbourhoods

The style guide is one city, not one room repeated.

- **Shop block.** Intimate, damp, amber, ceramic, paper. Street visible through glass. No campus, no atrium.
- **Exchange block.** Larger floor plates, dead finance, half the lights off, rumours instead of trade. Foil and racks appear at the market.
- **Underneath.** Round concrete, fan hum, condensate. Colour drains toward teal and grey.
- **Relay and archive.** Obsolete telecom. Open racks, a dark dish, then a dry quiet room. The archive should feel kept, not ruined.

Do not invent a second city, a spaceport, or a chrome megastructure. Distance beyond a room is "a few blocks" only where the room text already says so.

## Signage and type

- Prefer physical signs: painted board, marker on metal, a buzzing storefront tube seen through rain.
- Generated images should not carry essential lore as rendered text. Models misspell words, and a wrong sign becomes false canon.
- In-game readable text already exists in descriptions (`RELAY UNSTAFFED`, `CASH ONLY, THEN LEAVE`, the carved cup). Art may suggest paper and tubes without legible sentences.
- Floorplans are drawings, not posters. Put the legend in the location reference, not in the image.

## Floorplan convention

Top-down line drawing. Warm paper ground (`#E2DCCE`), black or dark teal walls, no perspective, no people, no words.

| Mark | Meaning |
| --- | --- |
| Thick line | Wall |
| Thin double line | Window, not an exit |
| Gap with a small filled triangle | Implemented exit. The triangle points out |
| Dashed wall or dashed hatch | Hidden or flag-gated opening |
| Rectangle | Fixed furniture or rack |
| Circle | Implemented inspectable object |
| Stair or grate ticks | Vertical link (`up` / `down`) |

A circle is only for an `ObjectDef` keyword. Scenery mentioned in the room sentence (the shop's terminal row, chairs, gutters) may be drawn, but it is not a circle and it is not a new command.

Do not add a door because the composition wants one. Exits are listed in the location reference under existing behaviour.

## Continuity rules for new art

1. Read the location `reference.md` and the exit list before generating.
2. Match palette and materials to this guide and to the neighbouring room.
3. Show only implemented exits. A view through a window is not a door.
4. Hidden hatches stay unmarked in public-facing concept art, or appear as an ordinary panel, until the reference says the player can see them.
5. If a picture contradicts the graph, the picture is wrong. Record it in the location's unresolved questions and correct the image or the note.
6. Proposed layout (metres, which wall faces the alley) is labelled interpretation. It must not contradict an implemented keyword (`up`, `down`, `alley`, `floor`).

## What this guide refuses

Glossy holograms, flying cars as the subject, endless rain-soaked kanji, identical magenta corridors, pristine server cathedrals, and crowds posed for a poster. One room, one job, evidence that someone was here an hour ago.
