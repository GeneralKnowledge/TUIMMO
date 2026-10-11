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
| `say <text>` | `'` | Speech heard in this location |
| `chat <text>` | `gsay` | Speech heard everywhere |
| `whisper <player> <text>` | `tell` | Private message to one online operator |
| `emote <action>` | `me`, `em` | Action text heard in this location |
| `profile [player\|bio …\|status …]` | | View or edit a short public profile |
| `friend <add\|accept\|decline\|remove> <player>` | | Mutual friendship requests |
| `friends` | | List friends and pending requests |
| `ignore <player>` / `unignore` / `ignores` | `block` | Filter say, chat, emote, whisper, and friend requests |
| `status` | `st` | Your name and location |
| `inventory` | `inv`, `i` | What you carry |
| `inspect <thing>` | `examine`, `ex` | Read an object, person, cabin, or item |
| `mark <name>` | | Leave a named marker on the outskirts tile |
| `build cabin <name>` | | Raise a cabin on walkable outskirts ground |
| `enter <name>` / `leave` | | Enter or leave a cabin |
| `home <access\|allow\|deny\|rename\|demolish> …` | | Owner cabin controls |
| `sign <text>` / `sign read` / `sign remove` | | Persistent roadside notes |
| `trail <name>` | | Leave a trail post |
| `board [post <text>\|remove <id>]` | `notices` | Coffee Shop noticeboard |
| `quit` | `q` | Disconnect |

`exit` is not an alias of `quit`.

In the outskirts, `n`, `s`, `e`, and `w` (or the full words) step one tile. Water and rock refuse the step. `say` is heard by people on the same tile (or cabin interior). `chat` is still heard everywhere. Whispers are never written to room or global feeds; offline recipients are refused (messages are not stored for later delivery).

### Social rules

- Friend requests need explicit acceptance. Identity is the operator id, not the display name.
- `ignore` is directional. Ignored operators cannot whisper you or send friend requests, and their local/global/emote lines are hidden from your feed.
- Profiles expose only display name, optional bio/status, and online/offline to friends. Email, passwords, and recovery material are never shown.
- Friend lists show online/offline only. They do not reveal exact location.

### Homesteads and traces

- Cabins need walkable outskirts ground that is not the city gate. One cabin per tile; markers, signs, and trail posts may share that tile.
- Cabins have an interior place (`i:<id>`) linked by validated `world_connections`. Access defaults to `public`; owners may set `private` or `friends`, grant named visitors, rename, or demolish with `home demolish confirm`.
- Demolition removes connections and moves anyone inside to the exterior door tile. Reconnecting inside a demolished cabin relocates you safely.
- Signs are short (≤80 chars). Noticeboard posts are ≤160 chars, rate-limited, capped per board, and expire after seven days. Authors may remove their own posts.

Spectators, if a future mode adds them, can look and read. They cannot move, inspect, speak, build, or mark. The server checks that. The client cannot grant it.

## A short discovery

You start with a loyalty chip. It opens nothing.

In the Abandoned Exchange, `inspect terminal` tears off a log scrap. At the Old Relay, `inspect relay` yields a cipher plate. Back at the relay, `go archive` opens the Black Archive. Before that, the door answers that it is sealed.

## Tone

Short descriptions, present tense, no encyclopedia. The log you can see is the last 40 lines that apply to you.
