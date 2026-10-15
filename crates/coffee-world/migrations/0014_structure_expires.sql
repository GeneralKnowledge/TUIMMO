-- Ephemeral player traces (chalk). NULL = permanent (sign, trail, cabin, …).
ALTER TABLE world_structures ADD COLUMN expires_at INTEGER;
