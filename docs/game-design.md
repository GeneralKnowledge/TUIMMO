# Coffee.shop — the Undernet

Ten rooms, one shared world. The Coffee Shop is the safe room where new operators appear.

```text
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

`map` prints this with your room marked. The archive door is hidden until you are carrying a cipher plate.

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

In the Abandoned Exchange, `inspect terminal` tears off a log scrap. At the Old Relay, `inspect relay` yields a cipher plate. Back at the relay, `go archive` opens the Black Archive. Before that, the door answers that it is sealed.

## Tone

Short descriptions, present tense, no encyclopedia. The log you can see is the last 40 lines that apply to you.
