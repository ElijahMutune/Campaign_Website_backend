ALTER TABLE campaign_schedule
ADD COLUMN is_published BOOLEAN NOT NULL DEFAULT FALSE,
ADD COLUMN published_at TIMESTAMPTZ;

CREATE INDEX idx_campaign_schedule_public_listing
ON campaign_schedule (scheduled_date, display_order)
WHERE is_published = TRUE;