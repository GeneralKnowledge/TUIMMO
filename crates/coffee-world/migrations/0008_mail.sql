-- Private mail. One row is the recipient's copy.
-- The id is assigned by the world and never reused. mail_seq holds the next id
-- after deletes, so an old number cannot come to mean a different letter.
-- Bodies are not written into the shared message log.

CREATE TABLE mail (
    id INTEGER PRIMARY KEY NOT NULL,
    sender_id TEXT NOT NULL REFERENCES operators (id),
    recipient_id TEXT NOT NULL REFERENCES operators (id),
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    read_at INTEGER,
    CHECK (id >= 1),
    CHECK (sender_id != recipient_id),
    CHECK (body != '')
);

CREATE INDEX idx_mail_recipient ON mail (recipient_id, created_at, id);

CREATE TABLE mail_seq (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    next_id INTEGER NOT NULL CHECK (next_id >= 1)
);

INSERT INTO mail_seq (singleton, next_id) VALUES (1, 1);
