# Coffee.shop — the Undernet

Ten rooms, one shared world. The Coffee Shop is the safe room where new operators appear. The authoritative graph is the exit list in `crates/coffee-world/src/undernet.rs`, mirrored in `assets/world/implemented.graph`.

```text
Roof Array
    |
The Coffee Shop -- Neon Alley -- Cooling Tunnels -- The Old Relay
    |                 |                               |    |
    |                 |                               |    +-- Black Archive (sealed)
    |            The Exchange -- Stack Market         |
    |                 |         \                     |
    |                 |          Service Corridor     |
    |                 |                 |             |
    |                 |         Abandoned Exchange ---+
```

`map` marks where you are and shows the archive as `(sealed)` until you carry a cipher plate. That schematic omits two real walks: Stack Market to the Service Corridor, and the Abandoned Exchange to the Old Relay. Visual direction, floorplans, and the full connection list live in `assets/world/`.

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
| `quit` | `q` | Disconnect |

`exit` is not an alias of `quit`.

Spectators, if a future mode adds them, can look and read. They cannot move, inspect, or speak. The server checks that. The client cannot grant it.

## A short discovery

You start with a loyalty chip. It opens nothing.

In the Abandoned Exchange, `inspect terminal` tears off a log scrap. At the Old Relay, `inspect relay` yields a cipher plate. `go archive` then opens the Black Archive. Before that, the exit is hidden and guessing it says the way is sealed. `go floor` in the relay is a different, always-open passage to the Abandoned Exchange.

## Tone

Short descriptions, present tense, no encyclopedia. The log you can see is the last 40 lines that apply to you.
