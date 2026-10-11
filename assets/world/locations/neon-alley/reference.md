# Neon Alley

## Identity

- **ID:** `neon-alley`
- **Display name:** Neon Alley
- **Region:** Undernet
- **Type:** Service alley, street level
- **Narrative purpose:** The step from the shop into the rest of the block. It is the only outdoor walk in the game.
- **Access:** Public. No flag.

## Player experience

**Implemented.** A service alley between the shop and the exchange. Signage buzzes. Rain collects in a gutter that smells like hot dust. A torn poster is the only inspectable thing.

**Interpretation.** Narrow, wet, and short enough to see both doors. The neon is a sign at the exchange end and a spill on the brick, not a canyon of advertisements. Players pass through, but the poster and the grate give them a reason to stop.

Memorable pieces: the gutter, the poster whose date is still bright, the grate that breathes warm air.

## Spatial layout

**Interpretation.**

- About 4 m wide and 20 m long. Open to the sky. No roof exit.
- **West end:** door, keyword `shop`, to `coffee-shop`. This is the shop's east door.
- **East end:** opening, keyword `exchange`, to `exchange`.
- **Floor, toward the east half:** grate and a steep stair, keyword `down`, to `cooling-tunnels`.
- **North wall:** the poster.
- **South edge:** gutter. Not an object and not an exit.
- **Hidden areas:** none. The grate is visible. It is the `down` exit, not a secret.

The alley does not connect to the roof, the market, or the service corridor. Those are reached through the exchange or the shop.

## Existing game behaviour

| Keyword | Kind | Destination or result |
| --- | --- | --- |
| `shop` | Exit | `coffee-shop` |
| `exchange` | Exit | `exchange` |
| `down` | Exit | `cooling-tunnels` |

| Keyword | Aliases | Name | Grant |
| --- | --- | --- | --- |
| `poster` | `wall` | poster | none |

Poster copy: a torn poster for a night market that closed three years ago. The date is still bright. There is no night market location.

`inspect wall` hits the poster because `wall` is an alias. It does not describe the architecture.

## Visual continuity

- Wet concrete, brick or stained render, one buzzing sign, teal puddles, a little amber leaking from the shop door.
- The poster is paper and torn. Do not design a second readable billboard.
- West door must read as the shop. East end must read as the exchange front, not as the relay.
- The grate is in the ground, not a manhole in a main road. This is a service alley.
- Do not draw the cooling tunnel as visible beyond a dark opening. The tunnel is another location.
- Do not add a fire escape to the roof. The roof is only `up` from inside the shop.

## Images

- `concept.png` — standing in the alley, shop door behind the light at one end, exchange sign at the other, grate in the ground.
- `floorplan.png` — long rectangle, no words.

Legend:

| Mark | Meaning |
| --- | --- |
| West triangle | `go shop` |
| East triangle | `go exchange` |
| Grate ticks in the floor | `go down` |
| Circle on the north wall | `inspect poster` |
| South channel | Gutter, scenery |

## Unresolved questions

- "Between the shop and the exchange" is the whole implemented geography of the alley. Exact storefront names are not canon.
- The night market on the poster is history inside one sentence, not a room.
- A longer street with more doors would be a new location. Do not paint extra entrances.
- `concept.png` shows a barred window on the shop-side wall. That window is scenery. It is not an exit, and it does not open into the shop or the roof.
- The south strip on `floorplan.png` is the gutter named in the legend, not a window and not a door.
