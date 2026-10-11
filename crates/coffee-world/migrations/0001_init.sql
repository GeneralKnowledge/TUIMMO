CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE operators (
    id TEXT PRIMARY KEY NOT NULL,
    account_id TEXT NOT NULL UNIQUE REFERENCES accounts (id),
    name TEXT NOT NULL,
    location_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen INTEGER NOT NULL
);

CREATE TABLE locations (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL
);

CREATE TABLE exits (
    from_id TEXT NOT NULL REFERENCES locations (id),
    keyword TEXT NOT NULL,
    to_id TEXT NOT NULL REFERENCES locations (id),
    label TEXT NOT NULL,
    requires_flag TEXT,
    PRIMARY KEY (from_id, keyword)
);

CREATE TABLE operator_flags (
    operator_id TEXT NOT NULL REFERENCES operators (id),
    flag TEXT NOT NULL,
    PRIMARY KEY (operator_id, flag)
);

CREATE TABLE operator_items (
    operator_id TEXT NOT NULL REFERENCES operators (id),
    item_key TEXT NOT NULL,
    name TEXT NOT NULL,
    detail TEXT NOT NULL,
    PRIMARY KEY (operator_id, item_key)
);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    at_ms INTEGER NOT NULL,
    channel TEXT NOT NULL,
    audience TEXT NOT NULL,
    audience_key TEXT NOT NULL,
    speaker TEXT,
    body TEXT NOT NULL
);

CREATE INDEX idx_messages_id ON messages (id DESC);
CREATE INDEX idx_operators_location ON operators (location_id);
