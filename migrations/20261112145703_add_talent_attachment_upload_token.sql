ALTER TABLE talent_applications
ADD COLUMN attachment_upload_token UUID NOT NULL DEFAULT gen_random_uuid();