-- Personal field notes reuse region_discoveries.
-- Visit rows stay kind 'entered'. Notes use kind 'field' and a stable content_key.
-- region_id is null when the note was taken in a static Undernet room.
-- Existing visit rows, structures, and accounts are copied through unchanged.

CREATE TABLE region_discoveries_v2 (
    id TEXT PRIMARY KEY NOT NULL,
    region_id TEXT REFERENCES world_regions (id),
    local_x INTEGER NOT NULL,
    local_y INTEGER NOT NULL,
    kind TEXT NOT NULL,
    note TEXT NOT NULL,
    actor_id TEXT,
    created_at INTEGER NOT NULL,
    content_key TEXT,
    title TEXT,
    place_key TEXT
);

INSERT INTO region_discoveries_v2 (
    id, region_id, local_x, local_y, kind, note, actor_id, created_at,
    content_key, title, place_key
)
SELECT
    id, region_id, local_x, local_y, kind, note, actor_id, created_at,
    NULL, NULL, NULL
FROM region_discoveries;

DROP TABLE region_discoveries;
ALTER TABLE region_discoveries_v2 RENAME TO region_discoveries;

CREATE INDEX idx_region_discoveries_region ON region_discoveries (region_id);
CREATE INDEX idx_region_discoveries_actor ON region_discoveries (actor_id, kind);

-- One personal note per player per authored discovery. Visit rows are excluded.
CREATE UNIQUE INDEX idx_region_discoveries_field
    ON region_discoveries (actor_id, content_key)
    WHERE kind = 'field' AND actor_id IS NOT NULL AND content_key IS NOT NULL;
