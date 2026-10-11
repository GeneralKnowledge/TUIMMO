-- Social relationships, profiles, structure access, interiors, noticeboard.
-- Player-created rows stay independent of static Undernet reseeding.

ALTER TABLE operators ADD COLUMN bio TEXT NOT NULL DEFAULT '';
ALTER TABLE operators ADD COLUMN status TEXT NOT NULL DEFAULT '';

CREATE TABLE friendships (
    requester_id TEXT NOT NULL REFERENCES operators (id),
    recipient_id TEXT NOT NULL REFERENCES operators (id),
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (requester_id, recipient_id),
    CHECK (requester_id != recipient_id),
    CHECK (status IN ('pending', 'accepted'))
);

CREATE INDEX idx_friendships_recipient ON friendships (recipient_id);
CREATE INDEX idx_friendships_status ON friendships (status);

CREATE TABLE ignores (
    operator_id TEXT NOT NULL REFERENCES operators (id),
    ignored_id TEXT NOT NULL REFERENCES operators (id),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (operator_id, ignored_id),
    CHECK (operator_id != ignored_id)
);

CREATE INDEX idx_ignores_ignored ON ignores (ignored_id);

-- Allow multiple structure kinds on one tile (marker + cabin + sign + trail).
CREATE TABLE world_structures_new (
    id TEXT PRIMARY KEY NOT NULL,
    region_id TEXT NOT NULL REFERENCES world_regions (id),
    local_x INTEGER NOT NULL,
    local_y INTEGER NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    owner_id TEXT,
    access TEXT NOT NULL DEFAULT 'public',
    interior_place TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (region_id, local_x, local_y, kind),
    CHECK (access IN ('public', 'private', 'friends'))
);

INSERT INTO world_structures_new (
    id, region_id, local_x, local_y, kind, name, owner_id, access, interior_place, created_at
)
SELECT id, region_id, local_x, local_y, kind, name, owner_id, 'public', NULL, created_at
FROM world_structures;

DROP TABLE world_structures;
ALTER TABLE world_structures_new RENAME TO world_structures;

CREATE INDEX idx_world_structures_owner ON world_structures (owner_id);
CREATE INDEX idx_world_structures_interior ON world_structures (interior_place);

CREATE TABLE structure_grants (
    structure_id TEXT NOT NULL REFERENCES world_structures (id) ON DELETE CASCADE,
    grantee_id TEXT NOT NULL REFERENCES operators (id),
    created_at INTEGER NOT NULL,
    PRIMARY KEY (structure_id, grantee_id)
);

CREATE INDEX idx_structure_grants_grantee ON structure_grants (grantee_id);

CREATE TABLE noticeboard_posts (
    id TEXT PRIMARY KEY NOT NULL,
    place_id TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES operators (id),
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX idx_noticeboard_place_created ON noticeboard_posts (place_id, created_at DESC);
CREATE INDEX idx_noticeboard_author ON noticeboard_posts (author_id);
