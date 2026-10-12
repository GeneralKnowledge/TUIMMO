# World architecture

This document records the geography that existed before this milestone, then the model that now sits beside it. The generator creates the baseline. Players create the history. Terminus is only the way they inhabit it.

## Assessment of the starting architecture

Coffee.shop was one process. `WorldState` implemented `terminus_core::Game`. `Hub` owned that state and applied one login, command, or disconnect at a time. A successful command returned a `PersistOp` list. The hub committed that list in one SQLite transaction and, on failure, restored the in-memory operator and message log.

Locations were ten hand-authored rooms in `crates/coffee-world/src/undernet.rs`. Each room had a stable string id (`coffee-shop`, `roof-array`, and so on), a name, a description, and graph exits. The archive door required the `cipher-plate` flag. Live movement read `undernet::all_exits()` in process. The `exits` table was a seed copy, not the authority.

On every startup `Store::seed` upserted location names and descriptions, then deleted and reinserted every exit. Operator accounts, inventory, positions (`operators.location_id`), flags, and the bounded message log survived that reseed. There was no procedural geography, no region identity, and no place for a player-built structure that the reseed would be required to leave alone.

The map and location views were built from those static definitions. `terminus-core` knew commands, sessions, and `GameView`. It did not know rooms, biomes, or SQLite. `examples/minimal-game` implemented `Game` without linking `coffee-world`.

That boundary is preserved. Procedural geography lives in `coffee-world`.

## Three kinds of information

**Generated baseline.** Terrain, landmarks, and the natural layout of a region are a pure function of world, epoch, birth coordinates, and generation version. Unexplored regions have no database row. The tile grid is never stored.

**Persistent modifications.** Anything an authoritative action creates is a row: a materialized region record, a structure, a terrain override, a discovery, or an explicit connection. These survive restart, a static Undernet reseed, and a future generator. They are not rebuilt by calling the generator again.

**Runtime state.** Sessions, who is standing on a tile, in-progress commands, and the region cache exist only while the process runs. The cache is a FIFO of 64 generated baselines. Evicting it cannot change what a later generation returns.

## Stable identity

A region is not "the square currently drawn at these coordinates."

```text
RegionIdentity
  world              WorldId::Undernet  ("undernet", mixer tag 1)
  epoch_id           u32
  origin_x, origin_y i32 birth coordinates
  generation_version u16
```

`RegionCoordKey` is `(world, epoch, origin_x, origin_y)`. One epoch owns at most one region per birth cell. The version is stored on the region and is part of `RegionIdentity` and of every place key, but it is not part of the unique database key. A second algorithm cannot occupy the same birth cell by choosing a new version number.

`placement_x` and `placement_y` are an atlas label. They start equal to the birth coordinates. `relocate_region` can move the label. Birth coordinates, the derived seed, and place keys stay. Movement walks the birth grid, not the atlas. An enormous coordinate offset is not the migration mechanism.

A wilderness place key, stored in `operators.location_id` and in connections, is:

```text
w:<world>:<epoch>:<origin_x>:<origin_y>:<version>:<local_x>:<local_y>
```

Example: `w:undernet:1:0:0:1:8:8`. Static Undernet ids contain no colon, so they cannot parse as wilderness keys. Negative origins stay with the number (`w:undernet:1:-2:4:1:15:0`).

`Place` is either `Static(id)` or `Wild(WildernessPos)`. A room, a tile, a landmark, and a marker do not share one table. They share movement, inspection, and presentation through this key.

## Coordinates

Regions are 16 by 16 tiles. North is `+Y`. East is `+X`. Local coordinates are `0..16`.

Region `(0, 0)` covers absolute tiles `0..16` on both axes. Region `(-1, 0)` covers absolute `x` of `-16..0`. Crossing a border uses `div_euclid` and `rem_euclid`, so the west edge of `(0, 0)` steps onto local `x = 15` of region `(-1, 0)`. Stepping east from local `15` of region `i32::MAX` returns `RegionOutOfRange` and the move is refused.

## Epochs and generation versions

An epoch is one generation regime: a name, a world seed, and the generation version new regions of that epoch are born with.

Epoch 1, "First Wilderness", is the only epoch. Its world seed is `0x00C0_FFEE_5EED_0001` and its generation version is `1`. `frontier_epoch()` returns that epoch. A newly visited birth cell in epoch 1 is born as version 1. A cell that already has a row keeps the version on that row.

A later epoch would be a new `EpochSpec` and new frontier regions. Existing rows would keep epoch 1 and version 1. The two regimes would meet through explicit connections (expeditions, built routes, anomalous passages), not by regenerating the old grid.

Do not edit what version 1 means after regions exist. A new algorithm is a new version number. Loading a row whose version this binary cannot generate does not delete the row and does not block login. The place stays addressable. The view says the ground is unreadable. Movement that needs the generator fails closed.

Changing terrain *semantics* (what "forest" or "water" means to building, travel, or resources) is not a silent migration. It needs an explicit conversion. This milestone does not provide one, and it should not be treated as trivial.

## Deterministic generation

`crates/coffee-world/src/geography/generate.rs` is the version 1 generator. It does not read `GeographyState`, the database, the clock, or a process-wide random number generator.

The mixer is the SplitMix64 finalizer, used as a pure function (`mix`). It is not `std` hashing. Channel hashes mix the epoch world seed, the generation version, the absolute tile, and a channel id:

| Channel | Use |
| --- | --- |
| 1 | Elevation lattice corners |
| 2 | Moisture at the absolute tile |
| 3 | Landmark chance |
| 4 | Landmark kind |

Elevation is bilinear interpolation of a lattice spaced every 4 absolute tiles. Moisture and landmarks are per tile. Thresholds, out of 65536:

| Field | Rule |
| --- | --- |
| elevation `< 14418` | water |
| elevation `>= 56361` | rock |
| otherwise, moisture `< 18350` | scrub |
| otherwise, moisture `>= 44564` | forest |
| otherwise | grass |
| feature `>= 63570` on walkable ground | spring, standing stone, fallen mast, or milepost |

Grass, forest, and scrub are traversable. Water and rock are not.

`derive_region_seed` mixes the epoch world seed, the world tag, the epoch id, the packed birth coordinates, and the version. The result is stored as 16 hex digits on `world_regions.region_seed` for audit. Version 1 terrain does not read that column back. Overwriting it does not change the grid.

The fingerprint of region `(0, 0)` version 1 is `0x8812_F1DB_241C_39A1`. That constant is a regression lock in the generator tests.

Regions are generated when a player enters them or when a server API first changes them. Startup does not walk the universe.

## Boundary continuity

Version 1 samples an absolute coordinate field. The east edge of region `(x, y)` and the west edge of `(x + 1, y)` are neighbouring absolute tiles of the same function, including across negative origins. The same holds north and south. Rivers and ridges are not traced as connected features. Water and rock are speckles and short runs of the elevation field. That is the continuity guarantee for version 1: shared samples, not a river network.

A future version beside version 1 is not continuous with it. The seam is the epoch and version boundary, to be joined later by an explicit connection.

The city gate is not part of the baseline. Tile `(8, 8)` of region `(0, 0)` epoch 1 version 1 is an authored seam. Version 1 generates water there, and water on the north, east, and south neighbours. The seam presents that one tile as packed earth and keeps it walkable, including when a terrain modification would otherwise block it. The generated grid is not rewritten, so the origin fingerprint stays `0x8812_F1DB_241C_39A1`. Look text is built from the same neighbour check movement uses: west is open, and water blocks the other three sides. `city` returns to the roof. A terrain modification on the gate is shown as recorded; it still cannot seal the city.

A repeatable walk of version 1, including that seam, is locked in `geography::explore`. From the gate, 221 tiles are one walkable component and all 6 origin landmarks are reachable. The nearest is a standing stone 8 steps away. Across five birth cells, no sampled chart left a landmark on ground the start could not reach. Median steps to a first landmark stay between 4 and 10. Region `(1, 0)` holds only one landmark and `(2, -1)` holds two; the others hold six or eight. That spread is the existing feature threshold (about 3% of walkable tiles), not a reason to retune it. Water at the gate is a local seam, now described in the look text. The region map already marks every landmark with `*`.

## Persistence and migrations

`0001_init.sql` is unchanged. `0002_world.sql` adds:

| Table | What it stores |
| --- | --- |
| `world_regions` | Identity, version, audit seed, atlas placement, discovery time. Unique on `(world_id, epoch_id, origin_x, origin_y)`. |
| `world_structures` | Structures per tile and kind (`marker`, `cabin`, `workshop`, `sign`, `trail`). Unique on `(region_id, local_x, local_y, kind)`. Enterables carry `access`, `interior_place`, and `share` (`private` / `shared` / `communal`). `owner_id` is attribution, not a foreign key, so history can outlive an account. |
| `terrain_modifications` | One override per tile. The baseline is not copied. |
| `region_discoveries` | Visits and personal field notes. Entering a region records an `entered` discovery. A player's journal is additional `field` rows in the same table. |
| `waypoints` | One operator's names for places they have stood. The destination is a place key, not a stored map. |
| `mail` | One recipient's private letters. `mail_seq` is the next id, kept ahead of deletes. |
| `world_connections` | Explicit links. Unique on `(from_place, keyword)`. |

Indexes cover atlas placement, structure owner, discoveries by region, and connections by origin place.

`Store::seed` still upserts static locations and replaces static exits. It does not touch the world tables, waypoints, or mail. A close-and-reopen cycle reloads regions, structures, modifications, discoveries, connections, waypoints, mail, accounts, and operator positions.

Materializing a region is ordered before the structure, modification, or discovery that references it, inside the same transaction. A failed commit restores the operator, the log, and the geography snapshot. A command that returns `Err` does not mutate first.

## The Undernet and the wilderness

The ten rooms and their exits are unchanged, including the sealed Black Archive. The Roof Array description mentions a break in the fencing.

From the Roof Array, `outskirts` (or `go outskirts`) enters the gate tile. From that tile, `city` returns to the Roof Array. That seam is code. It is not a row in `exits`, because those rows have foreign keys to static location ids, and a wilderness key must not be forced into that table.

Cardinal movement in the wilderness is `n`/`north`, `s`/`south`, `e`/`east`, `w`/`west`. Impassable directions are omitted from the exit list. A blocked step says which terrain stopped it. `say` is heard by operators on the same place key. `region`/`rsay` is heard across the physical region (birth cell or Undernet room). `chat` is still global. `map` in the wilderness draws the current region:

```text
@ you  + city gate  * landmark  M marker
. grass  t trees  , scrub  ~ water  ^ rock
```

The header shows epoch, birth coordinates, atlas placement, and generator version. The Undernet map is unchanged inside the city.

`mark <name>` leaves one marker on the current tile. The name is 2–16 characters, starts with a letter, and is letters, digits, or hyphens. Reserved words (`north`, `city`, `water`, landmark keywords, and the rest listed in `is_reserved_word`) are rejected. The owner is the session player. A second marker on the same tile is rejected. Spectators cannot mark.

`apply_terrain_mod`, `relocate_region`, and `link_places` are server APIs, not player commands. They are the seam the next builder milestone will call. `link_places` rejects reserved keywords, unknown places, self-links, impassable wilderness destinations, and duplicate keywords. Clients cannot set seeds, versions, or another player's owner id.

## Scale, without a second service

Generation is lazy. Lookup of a materialized region is a hash of the coordinate key, backed by the unique index. Modifications and structures are scanned per region in memory; the tables are indexed for the queries that will matter when a region holds many of them. The cache cap is fixed.

One Rust process and one SQLite file remain the deployment. If player activity outgrows that process, the split that fits this model is: keep generation pure and stateless, keep the persistent tables as the only history, and let one authoritative writer own mutations. A second process can regenerate baselines from identity alone. It must not invent a second writer for the same SQLite file.

## Security

Every persistent change is validated in `WorldState` and committed by the hub. A client command cannot:

- set an arbitrary place key (`go` only follows exits, cardinals, the city seam, or a stored connection)
- choose a generation version or seed
- write another player's `owner_id`
- read another player's field journal (`journal read` only returns the caller's rows)
- read or edit another operator's waypoint (`waypoint` commands are scoped to the caller)
- read or delete another operator's mail (`mail read` and `mail delete` require the recipient id)
- overwrite a generated grid except through `apply_terrain_mod`, which records an override
- open an operating-system shell (the SSH gateway still refuses `exec`)

`0003_social_and_places.sql` adds operator bio/status, friendships, ignores, structure access/interior columns (relaxing the one-structure-per-tile unique key to per-kind), structure grants, and the Coffee Shop noticeboard. Player social rows and player-built places are never wiped by static Undernet reseeding.

`0004_settlements.sql` adds settlements, membership (one per player), invites, and a nullable `world_structures.settlement_id` link. Disband clears membership, invites, settle notices, and structure links; it never deletes cabins, signs, trails, or discoveries. Hub rollback snapshots settlements alongside social/geography/log.

`0005_sharing_and_workshops.sql` adds `world_structures.share` (default `private`) with a CHECK constraint, rebuilds the structures table so fresh and upgraded databases agree, and indexes share. Legacy rows become private. Workshop kind values use the existing structures table; workshop project boards reuse `noticeboard_posts` with `place_id = workshop:<uuid>`.

`0006_field_notes.sql` rebuilds `region_discoveries` forward-only. Existing visit rows are copied with null `content_key`, `title`, and `place_key`. New columns:

| Column | Use |
| --- | --- |
| `content_key` | Stable discovery id, the same for every player who records that feature. |
| `title` | Short journal title. |
| `place_key` | Wilderness place key or static location id, so the note can point back. |
| `region_id` | Now nullable. Static-room notes (the roof, the Old Relay) have no region row. |

A unique index on `(actor_id, content_key)` for `kind = 'field'` makes recording idempotent. Visit rows are not part of that index. Field notes do not write terrain, landmarks, or another player's rows. A failed commit rolls the note back with the rest of the transaction.

`0007_waypoints.sql` adds `waypoints`, unique on `(operator_id, name_key)` and referencing `operators`. A row stores the display name, the normalized key, the place key, and an optional private note. It does not store a region grid. The same coordinates in another epoch or generation version are a different place key. If that key no longer resolves, the row is kept and is not rewritten onto a new tile. Showing a waypoint does not materialize a region. Hub rollback snapshots waypoints with the rest of the world.

`0008_mail.sql` adds `mail` and a one-row `mail_seq`. A letter stores the sender id, the recipient id, the body, the time, and an optional read time. The world assigns the id and advances `mail_seq` in the same transaction as the insert. Deletes do not return that id to the sequence. The body is not written into `messages`. Hub rollback snapshots the mailbox and the session overlays with the rest of the world.

The unfiled-survey stone at birth `(0, 0)` tile `(7, 8)` is presentation layered on the version 1 tile. `generate_region` is unchanged, and the origin fingerprint stays `0x8812_F1DB_241C_39A1`. Landmark inspection variants are chosen by a fixed salt of absolute coordinates in `geography/field.rs`. That salt is not a generator input.

The delisted floor uses those same rows. `inspect terminal` in the Abandoned Exchange sets the existing `read-terminal` flag and inserts one `torn-log` item for that operator. The Service Corridor panel always shows the code B-4; the sentence that matches it to a room struck from the public list is appended only when that operator holds the flag. `journal record` stores one `field` row with content key `delisted-floor`. A second operator can read the same terminal and keep a separate row. The object stays in the room. None of this writes a location, an exit, a generator input, or another player's journal. The Black Archive door still requires `cipher-plate`. A failed commit restores the operator and the geography snapshot, so a half-applied flag, item, or note is not kept.

### Region chat keys

Regional speech uses physical proximity only. The server derives the key from the speaker's place; clients cannot subscribe to arbitrary regions.

```text
Wild tile       → r:<world>:<epoch>:<origin_x>:<origin_y>
Cabin/workshop interior → same key as the structure's MaterializedRegion identity
Static room     → u:<location_id>   (borders = room exits)
```

Settlement boards reuse `noticeboard_posts` with `place_id = settle:<uuid>`. Workshop boards use `workshop:<uuid>`. Discovery lists public settlements whose anchor region matches the viewer's region key, or that have a linked structure on the current tile; private settlements are listed only to members. There is no omniscient global directory.

### Structure sharing precedence

Entry checks are centralised in `ensure_structure_access_for_move`:

1. Structure is enterable and present.
2. Owner.
3. Explicit visitor grant.
4. Settlement `shared` / `communal` while the accessor is a current member of the linked settlement (settlement must still exist).
5. Visitor access mode (`public` / `friends` / `private`).
6. Deny.

Disband and unclaim clear `settlement_id` and set `share` back to `private`. Structures are never deleted by settlement changes.

## Current limitations

- One epoch and one generation version.
- No connected rivers or mountain ranges. Continuity is the absolute field.
- Version boundaries are not blended.
- Settlements are membership groups with optional structure links, not map rectangles or guild ranks.
- No settlement-wide remote chat (region chat is physical only).
- Workshops are shared places with inspect/board/enter — not a crafting economy.
- Terrain modification and region relocation remain server APIs, not general player commands (building uses them only where needed).
- Private messages are online-only; offline delivery is not stored.
- Static exit reseeding still deletes and reinserts the hand-authored exit graph. Player links live in `world_connections` and are left alone.
- Unknown generator versions stay in the database and fail closed for movement.

## Next milestone

1. Optional trail segments as validated `terrain_modifications` along a path, still without a full terraforming sim.
2. Workshop activities that consume resources (crafting), if the world needs them.
3. Public-key authentication alongside passwords.
