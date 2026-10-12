-- Single-use account invite codes. Distinct from settlement_invites.
-- created_by NULL means an admin/seed code (for example a Reddit batch).
-- Players receive starter codes when they redeem one; invite list shows unused rows they own.

CREATE TABLE account_invites (
    id TEXT PRIMARY KEY NOT NULL,
    code_key TEXT NOT NULL COLLATE NOCASE,
    display_code TEXT NOT NULL,
    created_by TEXT REFERENCES operators (id),
    created_at INTEGER NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    redeemed_by TEXT REFERENCES operators (id),
    redeemed_at INTEGER,
    CHECK (code_key != ''),
    CHECK (display_code != ''),
    CHECK (
        (redeemed_by IS NULL AND redeemed_at IS NULL)
        OR (redeemed_by IS NOT NULL AND redeemed_at IS NOT NULL)
    )
);

CREATE UNIQUE INDEX idx_account_invites_code_key ON account_invites (code_key);

CREATE INDEX idx_account_invites_owner_open ON account_invites (created_by)
WHERE
    redeemed_by IS NULL;
