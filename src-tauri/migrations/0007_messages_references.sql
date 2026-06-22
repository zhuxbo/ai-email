-- Add references_header column to messages for RFC 5322 thread chain continuity.
-- `references` is a SQL reserved word so we use `references_header` instead.
-- Existing rows get NULL (no prior chain), which is the correct fallback.
ALTER TABLE messages ADD COLUMN references_header TEXT;
