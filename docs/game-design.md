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
| `region <text>` | `rsay` | Speech heard across this physical region |
| `whisper <player> <text>` | `tell` | Private message to one online operator |
| `emote <action>` | `me`, `em` | Action text heard in this location |
| `profile [player\|bio …\|status …]` | | View or edit a short public profile |
| `friend <add\|accept\|decline\|remove> <player>` | | Mutual friendship requests |
| `friends` | | List friends and pending requests |
| `ignore <player>` / `unignore` / `ignores` | `block` | Filter say, chat, region, emote, whisper, and friend/settle invites |
| `status` | `st` | Your name and location |
| `inventory` | `inv`, `i` | What you carry |
| `inspect <thing>` | `examine`, `ex` | Read an object, person, cabin, landmark, or item |
| `journal [recent\|read <id>\|record]` | | Private field journal of authored discoveries |
| `mark <name>` | | Leave a named marker on the outskirts tile |
| `build <cabin\|workshop> <name>` | | Raise a cabin or workshop on walkable outskirts ground |
| `enter <name>` / `leave` | | Enter or leave a cabin or workshop |
| `home <inspect\|access\|share\|allow\|deny\|rename\|demolish> …` | | Owner cabin/workshop controls |
| `workshop <inspect\|board …>` | | Inspect a workshop or use its project board |
| `sign <text>` / `sign read` / `sign remove` | | Persistent roadside notes |
| `trail <name>` | | Leave a trail post |
| `board [post <text>\|remove <id>]` | `notices` | Coffee Shop noticeboard |
| `settle <create\|inspect\|invite\|invites\|…>` | | Found, join, or manage a settlement |
| `settlements` | | List settlements visible from here |
| `quit` | `q` | Disconnect |

`exit` is not an alias of `quit`.

In the outskirts, `n`, `s`, `e`, and `w` (or the full words) step one tile. Water and rock refuse the step. `say` is heard by people on the same tile (or cabin interior). `region`/`rsay` is heard by anyone in the same birth region (cabin interiors share their exterior region's key) or the same Undernet room. `chat` is still heard everywhere. Whispers are never written to room or global feeds; offline recipients are refused (messages are not stored for later delivery).

### Social rules

- Friend requests need explicit acceptance. Identity is the operator id, not the display name.
- `ignore` is directional. Ignored operators cannot whisper you or send friend/settlement invites, and their local/global/region/emote lines are hidden from your feed.
- Profiles expose only display name, optional bio/status, and online/offline to friends. Email, passwords, and recovery material are never shown.
- Friend lists show online/offline only. They do not reveal exact location.

### Homesteads and traces

- Cabins and workshops need walkable outskirts ground that is not the city gate. One of each kind per tile; markers, signs, and trail posts may share that tile.
- Enterable structures have an interior place (`i:<id>`) linked by validated `world_connections`. Cabin visitor access defaults to `public`; workshops default to `private`. Owners may set `public` / `private` / `friends`, grant named visitors, rename, or demolish with `home demolish confirm`.
- Settlement sharing is separate from visitor access. `home share private|shared|communal` controls whether linked settlement members may enter. Joining a settlement never opens a private structure by itself.
- Demolition removes connections and moves anyone inside to the exterior door tile. Reconnecting inside a demolished cabin relocates you safely.
- Signs are short (≤80 chars). Noticeboard posts are ≤160 chars, rate-limited, capped per board, and expire after seven days. Authors may remove their own posts.

### Settlements

- One membership per player. `settle create <name>` founds a public settlement on walkable outskirts ground (or the door tile when inside a cabin). The founder is owner; owned cabins/workshops on that tile are auto-linked.
- `settle invites` lists your pending invitations (settlement, inviter, expiry, and exact accept/decline commands). Invites remain available while offline. They expire after seven days and cannot be accepted once expired.
- Ignore/block stops invites both ways. Accept requires no existing membership.
- Owner may `kick`, `transfer`, `describe` (≤160), `access public|private`, or `disband confirm`. Owner cannot leave while others remain; sole-member leave disbands.
- `claim` / `unclaim` links owned structures on the current tile. Disband and unclaim clear links and revert settlement share to `private`; cabins, workshops, and signs remain.
- `settlements` lists public settlements in the current region footprint (or linked here). Private settlements are visible only to members. `settle inspect` shows name, visibility, description, member count, and an anchor title (not raw coords for outsiders of private settles).
- `settle board` posts use `place_id = settle:<uuid>`. Readers must be in the anchor region or at a linked structure/cabin; writers are members; delete is author or owner.

### Sharing and workshops

- Visitor access (`home access`) and settlement share (`home share`) are orthogonal. Precedence for entry: owner → explicit grant → settlement share/communal membership → visitor access mode → deny.
- `shared` and `communal` require a live settlement link. Membership is checked at entry time; leaving removes membership-derived access immediately.
- `build workshop <name>` raises a persistent workshop. Designate settlement use with `home share communal` (or share a cabin the same way). `workshop inspect` / `home inspect` show ownership and policies. `workshop board` holds short project notes (`place_id = workshop:<uuid>`), using the same retention rules as other boards.

Spectators, if a future mode adds them, can look and read. They cannot move, inspect, speak, build, or mark. The server checks that. The client cannot grant it.

## Field journal

`journal` is a private record of authored discoveries. It is not a quest log and not a shared noticeboard.

- `journal` lists your entries (id and title). An empty journal says so.
- `journal recent` shows the latest entries with a place line and a short finding.
- `journal read <id>` shows one entry: title, finding, a way back, and the stable place key.
- `journal record` keeps the notable discovery at your current place. Recording the same discovery again does not add a second copy.

Ids such as `survey-mark` or `lm:undernet:1:stone:0:0:5:13` name the discovery, not the database row. Two operators can record the same id. `journal read` only opens the caller's own entry. A missing id, including another operator's, answers `No journal entry matches`.

Wilderness `inspect` names the landmark and, on some tiles, an unusual detail worth recording. Ordinary ground says there is nothing unusual. Look text for landmarks stays the short generated line; inspection is the longer note. Neither changes the generator.

A journal entry is not a sign, a trail post, or a region visit. `region_discoveries` rows with kind `entered` are shared history of who first opened a birth cell. Kind `field` is the personal note. Signs and structures stay world-visible. Journal text is not copied into chat, settlement boards, or another operator's journal.

## A short discovery

You start with a loyalty chip. It opens nothing.

In the Abandoned Exchange, `inspect terminal` tears off a log scrap. At the Old Relay, `inspect relay` yields a cipher plate. Back at the relay, `go archive` opens the Black Archive. Before that, the door answers that it is sealed. The cipher plate is still required. Field notes do not open the archive.

## The unfiled survey

This chain is optional and solo. It uses the Roof Array, the city gate, and the Old Relay dish. It does not grant items and does not change the archive door.

Player-facing text reveals the path one step at a time. The ending is not in `help`.

1. The Roof Array mast has fresh scratches. `inspect antenna` reads a bearing: one step west of the gate. `journal record` there keeps `survey-scratch`.
2. That tile (`w:undernet:1:0:0:1:7:8`) is open grass with no generated landmark. Look shows a flat stone cut twice. `inspect mark` (also `stone` or `cairn`) finds relay insulation and does not name the dish. Inspecting or recording it sets the `survey-mark` flag. `journal record` keeps `survey-mark`.
3. At the Old Relay, `inspect dish` mentions a scuff. Before the flag, that is all, and `journal record` refuses. After the flag, the dish matches the fiber: the relay crew swung the dish toward the gate, closed the survey, and never filed it because the archive was already sealed. `journal record` keeps `survey-closure`.

The stone is an authored overlay in presentation. It is not a terrain edit and not a new landmark in `generate_region`. Version 1's fingerprint stays `0x8812_F1DB_241C_39A1`.

## Tone

Short descriptions, present tense, no encyclopedia. The log you can see is the last 40 lines that apply to you.
