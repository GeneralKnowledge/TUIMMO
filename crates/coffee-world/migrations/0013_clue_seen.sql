CREATE TABLE clue_seen (
    operator_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
    clue_id TEXT NOT NULL,
    first_seen_at INTEGER NOT NULL,
    PRIMARY KEY (operator_id, clue_id)
);

CREATE INDEX idx_clue_seen_clue ON clue_seen (clue_id);
