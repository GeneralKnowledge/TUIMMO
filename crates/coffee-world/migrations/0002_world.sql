-- Persistent procedural geography.
-- These tables are never wiped by static Undernet reseeding.
-- The generated tile grid is not stored; region identity is enough to rebuild it.

CREATE TABLE world_regions (
    id TEXT PRIMARY KEY NOT NULL,
    world_id TEXT NOT NULL,
    epoch_id INTEGER NOT NULL,
    origin_x INTEGER NOT NULL,
    origin_y INTEGER NOT NULL,
    generation_version INTEGER NOT NULL,
    region_seed TEXT NOT NULL,
    placement_x INTEGER NOT NULL,
    placement_y INTEGER NOT NULL,
    discovered_at INTEGER NOT NULL,
    UNIQUE (world_id, epoch_id, origin_x, origin_y)
);

CREATE INDEX idx_world_regions_placement
    ON world_regions (world_id, epoch_id, placement_x, placement_y);

CREATE TABLE world_structures (
    id TEXT PRIMARY KEY NOT NULL,
    region_id TEXT NOT NULL REFERENCES world_regions (id),
    local_x INTEGER NOT NULL,
    local_y INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    owner_id TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (region_id, local_x, local_y)
);

CREATE INDEX idx_world_structures_owner ON world_structures (owner_id);

CREATE TABLE terrain_modifications (
    region_id TEXT NOT NULL REFERENCES world_regions (id),
    local_x INTEGER NOT NULL,
    local_y INTEGER NOT NULL,
    terrain TEXT NOT NULL,
    reason TEXT NOT NULL,
    actor_id TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (region_id, local_x, local_y)
);

CREATE TABLE region_discoveries (
    id TEXT PRIMARY KEY NOT NULL,
    region_id TEXT NOT NULL REFERENCES world_regions (id),
    local_x INTEGER NOT NULL,
    local_y INTEGER NOT NULL,
    kind TEXT NOT NULL,
    note TEXT NOT NULL,
    actor_id TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_region_discoveries_region ON region_discoveries (region_id);

CREATE TABLE world_connections (
    id TEXT PRIMARY KEY NOT NULL,
    from_place TEXT NOT NULL,
    keyword TEXT NOT NULL,
    to_place TEXT NOT NULL,
    label TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (from_place, keyword)
);

CREATE INDEX idx_world_connections_from ON world_connections (from_place);
