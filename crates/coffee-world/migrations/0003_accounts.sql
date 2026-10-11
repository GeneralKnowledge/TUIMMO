-- Account lifecycle. Existing rows stay in place and become permanent accounts.
-- password_hash remains required. Guests store a throwaway verifier, not a blank secret.

ALTER TABLE accounts ADD COLUMN kind TEXT NOT NULL DEFAULT 'permanent';
ALTER TABLE accounts ADD COLUMN email TEXT;
ALTER TABLE accounts ADD COLUMN recovery_hash TEXT;
ALTER TABLE accounts ADD COLUMN recovery_rotated_at INTEGER;
ALTER TABLE accounts ADD COLUMN password_changed_at INTEGER;

CREATE UNIQUE INDEX idx_accounts_email ON accounts (email) WHERE email IS NOT NULL;
