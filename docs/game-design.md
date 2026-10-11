# Coffee.shop — the Undernet

Ten rooms, one shared world. The Coffee Shop is the safe room where new operators appear.

```text
Outskirts (generated)
    |
Roof Array
    |
The Coffee Shop -- Neon Alley -- The Exchange -- Stack Market
                       |              |
                       |        Service Corridor
                       |              |
                 Cooling Tunnels -- The Abandoned Exchange
                       |              |
                       +-------- The Old Relay
                                      |
                               The Black Archive   (sealed)
```

`map` inside the city prints this graph with your room marked. The archive door is hidden until you are carrying a cipher plate.

From the Roof Array, `outskirts` steps through a break in the fencing onto generated land. `map` there draws the 16 by 16 region you are standing in. `city`, from the gate tile, returns to the roof. The ten rooms are unchanged. The land beyond them is described in [world architecture](world-architecture.md).

## Commands

| Command | Aliases | Effect |
| --- | --- | --- |
| `help` | `?` | List commands, or `help go` |
| `look` | `l` | Current room, exits, people, objects |
| `map` | | ASCII map |
| `who` | | Operators in the world, and who is here |
| `go <exit>` | `move` | Walk. A bare exit word works: `alley` |
| `say <text>` | `'` | Speech heard in this room |
| `chat <text>` | `gsay` | Speech heard everywhere |
| `status` | `st` | Your name and location |
| `inventory` | `inv`, `i` | What you carry |
| `inspect <thing>` | `examine`, `ex` | Read an object, person, or item |
| `mark <name>` | | Leave a named marker on the outskirts tile you occupy |
| `quit` | `q` | Disconnect |

`exit` is not an alias of `quit`.

In the outskirts, `n`, `s`, `e`, and `w` (or the full words) step one tile. Water and rock refuse the step. `say` is heard by people on the same tile. `chat` is still heard everywhere.

A marker name is 2–16 characters, starts with a letter, and may contain letters, digits, and hyphens. One marker per tile. The server records you as the owner. You cannot mark a city room, and you cannot mark a tile that already has one.

Spectators, if a future mode adds them, can look and read. They cannot move, inspect, speak, or mark. The server checks that. The client cannot grant it.

## A short discovery

You start with a loyalty chip. It opens nothing.

In the Abandoned Exchange, `inspect terminal` tears off a log scrap. At the Old Relay, `inspect relay` yields a cipher plate. Back at the relay, `go archive` opens the Black Archive. Before that, the door answers that it is sealed.

## Tone

Short descriptions, present tense, no encyclopedia. The log you can see is the last 40 lines that apply to you.
