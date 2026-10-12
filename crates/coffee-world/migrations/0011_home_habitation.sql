-- Home habitation history for narrative evolution.
-- meaningful_returns counts arrivals after a genuine absence.
-- Departure/return timestamps support that rule without a background worker.
-- public_name remains the optional public title (S1 column).

ALTER TABLE personal_homes ADD COLUMN meaningful_returns INTEGER NOT NULL DEFAULT 0;
ALTER TABLE personal_homes ADD COLUMN last_departed_at INTEGER;
ALTER TABLE personal_homes ADD COLUMN last_returned_at INTEGER;
