-- 0011_category_lock.sql
ALTER TABLE messages ADD COLUMN category_locked INTEGER NOT NULL DEFAULT 0;
CREATE TABLE app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
