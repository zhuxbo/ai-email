-- 0003_message_category.sql
-- Sprint 3 — classification + priority.
--
-- The AI classifier writes one of five buckets per message:
--   'personal' | 'work' | 'notification' | 'promotion' | 'spam'
--
-- Stored as plain TEXT (open at the schema level) so future sprints can refine the set
-- without another migration. The Rust layer validates against the closed five for now.

ALTER TABLE messages ADD COLUMN category TEXT;

-- Filter chip in the UI: "show me everything tagged work, sort by priority". Index speeds
-- up large inboxes; small ones (≤a few thousand rows) don't need it but the cost is tiny.
CREATE INDEX idx_messages_category_priority
    ON messages (mailbox_id, category, priority);
