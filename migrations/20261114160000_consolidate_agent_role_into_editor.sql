BEGIN;

UPDATE users
SET
    role = 'editor',
    updated_at = NOW()
WHERE role = 'agent';

ALTER TABLE users
DROP CONSTRAINT IF EXISTS users_role_check;

ALTER TABLE users
ADD CONSTRAINT users_role_check
CHECK (role IN ('admin', 'mp', 'editor'));

COMMIT;