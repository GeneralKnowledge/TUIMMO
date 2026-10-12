-- Personal homes. A player designates one existing Squalor place as home.
-- The place key is the stable destination. No cabin or interior is created.
-- Abandoned rows stay for lifecycle presentation; they are not deleted on move.
-- Partial unique indexes enforce one active home per operator and per place.

CREATE TABLE personal_homes (
    id TEXT PRIMARY KEY NOT NULL,
    operator_id TEXT NOT NULL REFERENCES operators (id),
    place_key TEXT NOT NULL,
    claimed_at INTEGER NOT NULL,
    abandoned_at INTEGER,
    public_name TEXT NOT NULL DEFAULT '',
    public_description TEXT NOT NULL DEFAULT '',
    CHECK (place_key != ''),
    CHECK (abandoned_at IS NULL OR abandoned_at >= claimed_at)
);

CREATE UNIQUE INDEX idx_homes_one_active_per_operator
ON personal_homes (operator_id)
WHERE abandoned_at IS NULL;

CREATE UNIQUE INDEX idx_homes_one_active_per_place
ON personal_homes (place_key)
WHERE abandoned_at IS NULL;

CREATE INDEX idx_homes_place ON personal_homes (place_key);
CREATE INDEX idx_homes_operator ON personal_homes (operator_id);
