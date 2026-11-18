ALTER TABLE campaign_posters
ADD COLUMN display_order INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_campaign_posters_public_order
ON campaign_posters (display_order, created_at)
WHERE is_published = TRUE;