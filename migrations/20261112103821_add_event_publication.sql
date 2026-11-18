ALTER TABLE events
ADD COLUMN is_published BOOLEAN NOT NULL DEFAULT FALSE,
ADD COLUMN published_at TIMESTAMPTZ;

CREATE INDEX idx_events_public_listing
ON events (event_date)
WHERE is_published = TRUE;
