-- Personal waypoints. One operator's names for places they have stood.
-- The place key is the stable destination. The tile grid is not copied.
-- Notes are private to operator_id and are not journal entries.

CREATE TABLE waypoints (
    id TEXT PRIMARY KEY NOT NULL,
    operator_id TEXT NOT NULL REFERENCES operators (id),
    name_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    place_key TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (operator_id, name_key),
    CHECK (length(name_key) BETWEEN 2 AND 24),
    CHECK (length(display_name) BETWEEN 2 AND 24),
    CHECK (place_key != '')
);

CREATE INDEX idx_waypoints_operator ON waypoints (operator_id);
