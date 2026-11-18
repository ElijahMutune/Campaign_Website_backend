
-- YOUTH TALENT PORTAL
-- Keep applicant information separate from public showcase content.

ALTER TABLE talent_applications
ADD COLUMN is_public BOOLEAN NOT NULL DEFAULT FALSE,
ADD COLUMN public_display_name VARCHAR(150),
ADD COLUMN public_summary TEXT,
ADD COLUMN public_media_id UUID REFERENCES media(id) ON DELETE SET NULL,
ADD COLUMN published_at TIMESTAMPTZ;

ALTER TABLE talent_applications
ADD CONSTRAINT talent_applications_publication_fields_check
CHECK (
    NOT is_public
    OR (
        NULLIF(BTRIM(public_display_name), '') IS NOT NULL
        AND NULLIF(BTRIM(public_summary), '') IS NOT NULL
    )
);

CREATE INDEX idx_talent_applications_public
ON talent_applications (published_at DESC)
WHERE is_public = TRUE AND published_at IS NOT NULL;


-- NAVIGATION MANAGEMENT
-- Support parent and child navigation items.

ALTER TABLE navigation_items
ADD COLUMN parent_id UUID REFERENCES navigation_items(id) ON DELETE SET NULL;

CREATE INDEX idx_navigation_visible_order
ON navigation_items (parent_id, display_order)
WHERE is_visible = TRUE;


-- DOCUMENTS MANAGEMENT
-- Add categories, ordering and download tracking.

ALTER TABLE documents
ADD COLUMN category VARCHAR(150),
ADD COLUMN display_order INTEGER NOT NULL DEFAULT 0,
ADD COLUMN published_at TIMESTAMPTZ,
ADD COLUMN download_count BIGINT NOT NULL DEFAULT 0;

ALTER TABLE documents
ALTER COLUMN is_published SET DEFAULT FALSE;

CREATE INDEX idx_documents_public_order
ON documents (display_order, created_at)
WHERE is_published = TRUE AND published_at IS NOT NULL;


-- CONTACT MANAGEMENT
-- Add private administrator notes and update tracking.

ALTER TABLE contact_messages
ADD COLUMN admin_notes TEXT,
ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

CREATE INDEX idx_contact_messages_status_date
ON contact_messages (status, created_at DESC);
