-- Structure sharing policies and workshop kind support.
-- Legacy rows default to private sharing (settlement membership does not grant access).

ALTER TABLE world_structures ADD COLUMN share TEXT NOT NULL DEFAULT 'private';

-- Rebuild with CHECK so legacy DBs and fresh installs agree.
CREATE TABLE world_structures_share (
    id TEXT PRIMARY KEY NOT NULL,
    region_id TEXT NOT NULL REFERENCES world_regions (id),
    local_x INTEGER NOT NULL,
    local_y INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    owner_id TEXT,
    access TEXT NOT NULL DEFAULT 'public',
    interior_place TEXT,
    settlement_id TEXT REFERENCES settlements (id) ON DELETE SET NULL,
    share TEXT NOT NULL DEFAULT 'private',
    created_at INTEGER NOT NULL,
    UNIQUE (region_id, local_x, local_y, kind),
    CHECK (access IN ('public', 'private', 'friends')),
    CHECK (share IN ('private', 'shared', 'communal'))
);

INSERT INTO world_structures_share (
    id, region_id, local_x, local_y, kind, name, owner_id, access,
    interior_place, settlement_id, share, created_at
)
SELECT
    id, region_id, local_x, local_y, kind, name, owner_id, access,
    interior_place, settlement_id,
    CASE WHEN share IN ('private', 'shared', 'communal') THEN share ELSE 'private' END,
    created_at
FROM world_structures;

DROP TABLE world_structures;
ALTER TABLE world_structures_share RENAME TO world_structures;

CREATE INDEX idx_world_structures_owner ON world_structures (owner_id);
CREATE INDEX idx_world_structures_interior ON world_structures (interior_place);
CREATE INDEX idx_world_structures_settlement ON world_structures (settlement_id);
CREATE INDEX idx_world_structures_share ON world_structures (share);
