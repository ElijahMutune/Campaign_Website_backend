-- Candidate profile media reference.
-- Images are managed separately by the central media library.

ALTER TABLE candidate_profile
ADD COLUMN profile_media_id UUID
REFERENCES media(id)
ON DELETE SET NULL;

-- Control whether the candidate profile is public.
ALTER TABLE candidate_profile
ADD COLUMN is_published BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE candidate_profile
ADD COLUMN published_at TIMESTAMPTZ;

-- This website manages one candidate profile.
CREATE UNIQUE INDEX candidate_profile_singleton_idx
ON candidate_profile ((TRUE));
