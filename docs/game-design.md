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

## Arriving

A new operator who is still in the Coffee Shop, carrying only the loyalty chip and with no notes of their own, gets a short arrival page. It says they are in the Undernet, that the world stays when they leave, and that other operators share it. It offers `look`, `go alley`, `inspect counter`, and `help`. It does not assign a task.

`look` clears that page. `welcome` shows it again. The same account does not get the page on every reconnect. One line in that operator's own feed says help is available. Other operators do not see the line.

`seen-welcome` and `hint-inspect` remember those two notes. They grant nothing: no travel, no item, and no journal row. The first successful `inspect` adds one private line about the noticed panel, and does not repeat it.

`help` with no arguments lists topics: `around`, `people`, `explore`, `commands`, and `stuck`. `help <command>` still explains that command, including why someone might use it. `help chat` is the chat command. `help move` is `go`, because `move` is already an alias.

From the Roof Array, `outskirts` steps through a break in the fencing onto the gate. The gate is packed earth. West is open ground. Water blocks north, east, and south, and the description says so. `map` draws the 16 by 16 region you are standing in. `city`, from the gate tile, returns to the roof. The ten rooms are unchanged. The land beyond them is described in [world architecture](world-architecture.md).

## Commands

| Command | Aliases | Effect |
| --- | --- | --- |
| `help` | `?` | Topics `around`, `people`, `explore`, `commands`, `stuck`, or one command |
| `welcome` | | Show the arrival note again |
| `look` | `l` | Current room, exits, people, objects |
| `map` | | ASCII map |
| `who` | | Operators in the world, and who is here |
| `go <exit>` | `move` | Walk. A bare exit word works: `alley` |
| `say <text>` | `'` | Speech heard in this location |
| `chat <text>` | `gsay` | Speech heard everywhere |
| `region <text>` | `rsay` | Speech heard across this physical region |
| `whisper <player> <text>` | `tell` | Private message to one online operator. Not stored |
| `mail [list\|read\|send\|delete]` | | Private letter, kept for an offline operator |
| `emote <action>` | `me`, `em` | Action text heard in this location |
| `profile [player\|bio …\|status …]` | | View or edit a short public profile |
| `friend <add\|accept\|decline\|remove> <player>` | | Mutual friendship requests |
| `friends` | | List friends and pending requests |
| `ignore <player>` / `unignore` / `ignores` | `block` | Filter say, chat, region, emote, whisper, mail, and friend/settle invites |
| `status` | `st` | Your name and location |
| `inventory` | `inv`, `i` | What you carry |
| `inspect <thing>` | `examine`, `ex` | Read an object, person, cabin, landmark, or item |
| `journal [recent\|read <id>\|record]` | | Private field journal of authored discoveries |
| `theory [list\|read <id>\|post <observation\|theory> <text>\|remove <id>]` | | Shared observations and theories about the delisted floor |
| `waypoint [add\|list\|show\|note\|remove]` | | Private name and note for where you are standing |
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

In the outskirts, `n`, `s`, `e`, and `w` (or the full words) step one tile. Water and rock refuse the step. `say` is heard by people on the same tile (or cabin interior). `region`/`rsay` is heard by anyone in the same birth region (cabin interiors share their exterior region's key) or the same Undernet room. `chat` is still heard everywhere. Whispers are never written to room or global feeds; an offline recipient is refused and the whisper is not stored. `mail` is a separate letter that waits in the recipient's mailbox.

### Social rules

- Friend requests need explicit acceptance. Identity is the operator id, not the display name.
- `ignore` is directional for room, region, and global lines. Whisper and mail use the same either-direction check: if either operator ignores the other, delivery is refused, and the refusal does not say which side set it. Mail already in the mailbox is left there.
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

Ids such as `survey-mark`, `delisted-floor`, or `lm:undernet:1:stone:0:0:5:13` name the discovery, not the database row. Two operators can record the same id. `journal read` only opens the caller's own entry. A missing id, including another operator's, answers `No journal entry matches`.

Wilderness `inspect` names the landmark and, on some tiles, an unusual detail worth recording. Ordinary ground says there is nothing unusual. Look text for landmarks stays the short generated line; inspection is the longer note. Neither changes the generator. The place description stays put. The same inspection is kept in a `noticed` panel (and still copied to your private feed) until you inspect something else, so a later `say` or step does not hide it. `journal` opens in a notes panel beside that place. `look` closes the notes and returns to the place alone. Help, the map, and `who` use the same return.

A journal entry is not a sign, a trail post, or a region visit. `region_discoveries` rows with kind `entered` are shared history of who first opened a birth cell. Kind `field` is the personal note. Signs and structures stay world-visible. Journal text is not copied into chat, settlement boards, or another operator's journal.

## Mail

`mail` leaves a private letter for one existing operator. It is not a whisper, a noticeboard, or a journal entry.

```text
mail
mail list
mail read 42
mail send bob I found the old relay. The dish still faces the gate.
mail delete 42
```

`mail` with no arguments shows the command help and `Mailbox: N unread of M.` `mail list` is newest first. `#42` and `42` are the same id. Ids are not reused after delete.

A letter is at most 400 characters and is refused rather than trimmed. A mailbox holds 32 letters. When it is full, new mail is refused and nothing already stored is deleted. A sender can write one letter every 2 seconds. That gap is not kept across a server restart. The 32-letter cap is.

The recipient is resolved the same way as whisper: username or display name, case-insensitive, to an operator id. Sending does not create an account. You cannot mail yourself. The recipient may be offline. The letter is stored before the command reports success.

Only the recipient can list, read, or delete a letter. Reading marks it read for that operator and keeps the mark across restart. There is no sent folder and no read receipt. The letter body is not written into room, region, or global chat. A login status line shows `mail: N unread` when N is at least one.

Ignore in either direction refuses a new letter with the same wording whisper uses. Deleting a letter removes that recipient's row only.

## Waypoints

A waypoint answers where a place was. A journal entry answers what it meant. Either can exist without the other, and removing one leaves the other.

- `waypoint` explains the commands.
- `waypoint add <name>` saves the server's current place under a name you choose. The client does not send coordinates.
- `waypoint list` shows your names. `waypoint show <name>` adds a return hint, a private note, and the journal ids recorded at that same place.
- `waypoint note <name> <text>` replaces the note only. Notes are at most 160 characters.
- `waypoint remove <name>` deletes your mark. It does not delete the room, the landmark, or a journal entry.

Names are 2–24 characters: a letter, then letters, digits, or hyphens. Matching ignores case and keeps the typed spelling. A second `waypoint add` of the same name does not move the mark; remove it first. Two operators may use the same name. Neither can read the other's note.

Hints walk open tiles inside the current birth cell. Water and rock are not steps. Another birth cell is named by its offset and called out as not a route. A place that no longer matches its epoch or generator version stays in the list as unavailable and is not pointed at a different tile. `look` returns to the place. Nothing in these commands moves you.

## A short discovery

You start with a loyalty chip. It opens nothing.

At the Old Relay, `inspect relay` yields a cipher plate. Back at the relay, `go archive` opens the Black Archive. Before that, the door answers that it is sealed. The cipher plate is still required. Field notes do not open the archive. The torn page in the Abandoned Exchange is a separate optional note; see The delisted floor.

## The unfiled survey

This chain is optional and solo. It uses the Roof Array, the city gate, and the Old Relay dish. It does not grant items and does not change the archive door.

Player-facing text reveals the path one step at a time. The ending is not in `help`.

1. The Roof Array mast has fresh scratches. `inspect antenna` reads a bearing: one step west of the gate. `journal record` there keeps `survey-scratch`.
2. That tile (`w:undernet:1:0:0:1:7:8`) is open grass with no generated landmark. Look shows a flat stone cut twice. `inspect mark` (also `stone` or `cairn`) finds relay insulation and does not name the dish. Inspecting or recording it sets the `survey-mark` flag. `journal record` keeps `survey-mark`.
3. At the Old Relay, `inspect dish` mentions a scuff. Before the flag, that is all, and `journal record` refuses. After the flag, the dish matches the fiber: the relay crew swung the dish toward the gate, closed the survey, and never filed it because the archive was already sealed. `journal record` keeps `survey-closure`.

The stone is an authored overlay in presentation. It is not a terrain edit and not a new landmark in `generate_region`. Version 1's fingerprint stays `0x8812_F1DB_241C_39A1`.

## The delisted floor

This chain is optional and solo. It uses the Service Corridor panel, the Abandoned Exchange terminal, and, only as corroboration, the Black Archive stacks. It does not grant the cipher plate and does not change the archive door. A player can ignore it and keep using the city.

Player-facing text reveals the connection one step at a time. The ending is not in `help`.

The panel and the terminal are shared objects. The code on the panel is part of that shared description. Reading the terminal sets one operator's `read-terminal` flag and gives that operator a torn log. The terminal stays mounted. Another operator can take a separate page. Neither flag is announced.

1. In the Service Corridor, `inspect panel` shows the lowest floor crossed out. The stroke misses a code at the end: B-4. The room description does not repeat it. `journal record` waits, because the strike is not yet explained.
2. One floor down, `inspect terminal` (also `screen` or `computer`) tears off a single page. The noticed text leads with the page: last line B-4, the code on the service panel; the room was struck from the public list; the lock was left with the relay. A second inspect does not grant another page. It says there is nothing new and points at the scrap. The carried name is `torn log`; `inspect log`, `inspect torn`, and `inspect torn-log` all read it. The scrap does not include the journal hint.
3. Back at the panel, inspection leads with the match: B-4 is the torn page, and the marker struck that room from the public list. `journal record` keeps one note, `delisted-floor`. The same key can be recorded at the terminal, the panel, or (once the flag is set) in the Black Archive. Recording it again does not add a row. The note is short enough to read in the notes panel beside a noticed inspection.
4. An operator who has also opened the archive can `inspect stacks`. With the flag, the empty shelf is tagged B-4. Without the flag, the stacks text is unchanged. The shelf does not contain the missing file.

The exchange ticker still reads `ARCHIVE LOCKED`. This note does not say how to open the door. What was filed under B-4, and why the floor was delisted, stays unresolved.

Personal knowledge is the existing flag, the `torn-log` item, and the field note. Sharing a thought about the floor is a separate `theory` post, below. `generate_region` is unchanged.

## Shared theories

`theory` is a community record of what operators believe may be happening. It is not a quest log, not a public journal, and not the Coffee Shop noticeboard.

The private journal records a personal discovery. A shared theory records a player-authored statement. Neither confers discovery progress, items, flags, or a way into the Black Archive. Reading one does not copy the author's journal, mail, waypoint notes, or location.

The only topic is `delisted-floor`. Players cannot open another topic. A later authored mystery could add a topic id beside this one; the table already stores that id. This slice does not.

```
theory
theory list
theory read 3
theory post observation The panel still shows B-4.
theory post theory The lock may still be with the relay.
theory remove 3
```

`theory` with no arguments explains the commands. `theory list` is newest first and shows the type, the author's public name, and a short preview. `theory read <id>` shows one entry. `#3` and `3` are the same id. An observation is something the author believes they saw. A theory is an interpretation and may be wrong. The label is the author's. The game does not check it, mark it correct, or turn agreement into a flag.

Publishing is deliberate. `journal record` does not copy a note here. The post does not have to contain a keyword or prove the author found the panel, the torn log, or the archive.

An entry is at most 120 characters and is refused rather than trimmed. Control characters are stripped before it is stored. `delisted-floor` holds 32 entries. When it is full, the new post is refused and nothing already stored is deleted. One operator can post every 2 seconds. That gap is not kept across a server restart. The 32-entry cap is. Entries do not expire after seven days. That limit belongs to the noticeboards and is unchanged.

Ids come from a sequence that stays ahead of deletes, so a removed entry's number is not given to a later post. You can remove only your own entry. The author is the operator id, and the name on screen is that operator's current public name. There is no moderator command. A full topic stays full until an author removes their own entry.

Another operator can list and read while the author is offline. The body is not written into room, region, or global chat. `ignore` does not hide an entry. The record is public, not a directed message.

## Tone

Short descriptions, present tense, no encyclopedia. The log you can see is the last 40 lines that apply to you.
