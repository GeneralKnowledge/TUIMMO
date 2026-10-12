-- Shared theories. One row is a player-authored observation or interpretation.
-- These rows do not expire. They are not noticeboard posts and not journal notes.
-- The id is assigned by the world and never reused. shared_theory_seq holds the
-- next id after deletes, so an old number cannot come to mean a different entry.
-- topic names an authored mystery. This slice only writes delisted-floor.
-- Bodies are not written into the shared message log.

CREATE TABLE shared_theories (
    id INTEGER PRIMARY KEY NOT NULL,
    topic TEXT NOT NULL,
    author_id TEXT NOT NULL REFERENCES operators (id),
    kind TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    CHECK (id >= 1),
    CHECK (topic != ''),
    CHECK (kind IN ('observation', 'theory')),
    CHECK (body != '')
);

CREATE INDEX idx_shared_theories_topic ON shared_theories (topic, id);

CREATE TABLE shared_theory_seq (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    next_id INTEGER NOT NULL CHECK (next_id >= 1)
);

INSERT INTO shared_theory_seq (singleton, next_id) VALUES (1, 1);
