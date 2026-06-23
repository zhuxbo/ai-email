-- Add special_use column to mailboxes for IMAP special-use folder identification.
-- Values: 'inbox' | 'sent' | 'drafts' | 'trash' | 'junk' | NULL (regular folder).
-- Populated by the sync engine via SPECIAL-USE attributes or heuristic name matching.
-- Existing rows get NULL until the next sync runs list_mailboxes.
ALTER TABLE mailboxes ADD COLUMN special_use TEXT;
