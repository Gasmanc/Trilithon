-- Review remediation F011/F012: associate tokens with their owning user so that
-- the auth middleware can enforce disabled_at and must_change_pw for bearer token
-- authentication, matching the enforcement already present for session authentication.
--
-- The column is nullable for backward compatibility: tokens created before this
-- migration (or in test fixtures) have user_id = NULL and skip the user-level
-- enforcement. New token-creation paths must populate user_id.
ALTER TABLE tokens ADD COLUMN user_id TEXT REFERENCES users(id) ON DELETE CASCADE;
