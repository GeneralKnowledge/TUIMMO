-- Settlements: named membership groups anchored to an outskirts tile.
-- Structures may optionally link to a settlement; disband never deletes cabins.

CREATE TABLE settlements (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    name_key TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL DEFAULT 'public',
    anchor_place TEXT NOT NULL,
    owner_id TEXT NOT NULL REFERENCES operators (id),
    created_at INTEGER NOT NULL,
    CHECK (visibility IN ('public', 'private'))
);

CREATE INDEX idx_settlements_owner ON settlements (owner_id);
CREATE INDEX idx_settlements_anchor ON settlements (anchor_place);

CREATE TABLE settlement_members (
    settlement_id TEXT NOT NULL REFERENCES settlements (id) ON DELETE CASCADE,
    operator_id TEXT NOT NULL REFERENCES operators (id),
    role TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (settlement_id, operator_id),
    CHECK (role IN ('owner', 'member'))
);

-- One membership per player.
CREATE UNIQUE INDEX idx_settlement_members_operator ON settlement_members (operator_id);
CREATE INDEX idx_settlement_members_settlement ON settlement_members (settlement_id);

CREATE TABLE settlement_invites (
    id TEXT PRIMARY KEY NOT NULL,
    settlement_id TEXT NOT NULL REFERENCES settlements (id) ON DELETE CASCADE,
    inviter_id TEXT NOT NULL REFERENCES operators (id),
    invitee_id TEXT NOT NULL REFERENCES operators (id),
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    CHECK (status IN ('pending', 'accepted', 'declined', 'expired')),
    CHECK (inviter_id != invitee_id)
);

CREATE UNIQUE INDEX idx_settlement_invites_pending
    ON settlement_invites (settlement_id, invitee_id)
    WHERE status = 'pending';

CREATE INDEX idx_settlement_invites_invitee ON settlement_invites (invitee_id);
CREATE INDEX idx_settlement_invites_settlement ON settlement_invites (settlement_id);

ALTER TABLE world_structures ADD COLUMN settlement_id TEXT REFERENCES settlements (id) ON DELETE SET NULL;

CREATE INDEX idx_world_structures_settlement ON world_structures (settlement_id);
