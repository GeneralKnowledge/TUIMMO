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

The city gate is not part of the baseline. Tile `(8, 8)` of region `(0, 0)` epoch 1 version 1 is an authored overlay. It stays walkable even if the baseline is water or a terrain modification would block it. `generate_region` itself is unchanged.

## Persistence and migrations

`0001_init.sql` is unchanged. `0002_world.sql` adds:

| Table | What it stores |
| --- | --- |
| `world_regions` | Identity, version, audit seed, atlas placement, discovery time. Unique on `(world_id, epoch_id, origin_x, origin_y)`. |
| `world_structures` | One structure per tile. `kind` is structured (`marker` today). `name` is the player's text. `owner_id` is attribution, not a foreign key, so history can outlive an account. |
| `terrain_modifications` | One override per tile. The baseline is not copied. |
| `region_discoveries` | Visits and notes. Entering a region records an `entered` discovery. |
| `world_connections` | Explicit links. Unique on `(from_place, keyword)`. |

Indexes cover atlas placement, structure owner, discoveries by region, and connections by origin place.

`Store::seed` still upserts static locations and replaces static exits. It does not touch the five world tables. A close-and-reopen cycle reloads regions, structures, modifications, discoveries, connections, accounts, and operator positions.

Materializing a region is ordered before the structure, modification, or discovery that references it, inside the same transaction. A failed commit restores the operator, the log, and the geography snapshot. A command that returns `Err` does not mutate first.

## The Undernet and the wilderness

The ten rooms and their exits are unchanged, including the sealed Black Archive. The Roof Array description mentions a break in the fencing.

From the Roof Array, `outskirts` (or `go outskirts`) enters the gate tile. From that tile, `city` returns to the Roof Array. That seam is code. It is not a row in `exits`, because those rows have foreign keys to static location ids, and a wilderness key must not be forced into that table.

Cardinal movement in the wilderness is `n`/`north`, `s`/`south`, `e`/`east`, `w`/`west`. Impassable directions are omitted from the exit list. A blocked step says which terrain stopped it. `say` is heard by operators on the same place key. `chat` is still global. `map` in the wilderness draws the current region:

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
- overwrite a generated grid except through `apply_terrain_mod`, which records an override
- open an operating-system shell (the SSH gateway still refuses `exec`)

## Current limitations

- One epoch and one generation version.
- No connected rivers or mountain ranges. Continuity is the absolute field.
- Version boundaries are not blended.
- The only player-facing structure is a marker. Cabins, homesteads, and settlements are not built yet.
- Terrain modification, relocation, and player links are tested server APIs, not commands.
- Wilderness chat is per tile, not per region.
- `world_structures` allows one row per tile, which is right for a marker and too tight for a later tile that holds both a trail post and a cabin. That unique key should be revisited when structure kinds multiply.
- Static exit reseeding still deletes and reinserts the hand-authored exit graph. Player links live in `world_connections` and are left alone.
- Unknown generator versions stay in the database and fail closed for movement.

## Next milestone

Player-built structures that other people can walk into.

1. A `cabin` (and then a workshop) as a `world_structures` kind with an owner, a structured footprint, and an interior place key that is not a terrain tile.
2. `world_connections` from the door tile to that interior and back, created only by the owning player or someone they grant.
3. Trails as `terrain_modifications` along a validated path, not as free coordinate claims.
4. A settlement as a named grouping of structures in one or more regions, with membership separate from free-form description text.
5. Region-scoped chat distinct from tile-local `say` and global `chat`.
6. Permissions checked on the server for build, name, and demolish. Names and descriptions stay text fields. They never become the simulation.
