-- 0001_initial.sql (SQLite)
-- Initial schema for AI Email. See docs/SPEC.md § 3 for the canonical data model.
--
-- SQLite conventions (vs the original PostgreSQL schema):
--   • UUIDs are BLOB (16 bytes) — generated app-side via Uuid::new_v4() (no gen_random_uuid()).
--   • Timestamps are TEXT in RFC3339/UTC; DB default via strftime, Rust reads/writes UTC.
--   • Array columns (to_addrs / cc_addrs / flags) are JSON TEXT, decoded with #[sqlx(json)].
--   • Credentials NEVER live here — they live in the OS keychain keyed by accounts.id.
--   • FKs cascade; foreign_keys pragma is enabled on connect (db::connect).

-- Email accounts. Only non-secret metadata; the auth code lives in the OS keychain.
CREATE TABLE accounts (
    id              BLOB    PRIMARY KEY,
    email           TEXT    NOT NULL UNIQUE,
    display_name    TEXT,
    provider        TEXT    NOT NULL,    -- 'qq' | '163' | 'gmail' | 'outlook' | 'imap'
    imap_host       TEXT    NOT NULL,
    imap_port       INTEGER NOT NULL DEFAULT 993,
    smtp_host       TEXT    NOT NULL,
    smtp_port       INTEGER NOT NULL DEFAULT 465,
    created_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    last_synced_at  TEXT
);

-- IMAP folders, mirrored per account.
CREATE TABLE mailboxes (
    id              BLOB    PRIMARY KEY,
    account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,    -- 'INBOX' | '已发送' | '收件箱' …
    delimiter       TEXT,
    uid_validity    INTEGER,
    uid_next        INTEGER,
    last_synced_at  TEXT,
    UNIQUE (account_id, name)
);

-- Header-only rows, keyed by IMAP UID. Bodies live in message_bodies.
CREATE TABLE messages (
    id              BLOB    PRIMARY KEY,
    account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    mailbox_id      BLOB    NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    imap_uid        INTEGER NOT NULL,
    rfc_message_id  TEXT,
    thread_id       TEXT,
    subject         TEXT,
    from_addr       TEXT,
    to_addrs        TEXT    NOT NULL DEFAULT '[]',   -- JSON array of strings
    cc_addrs        TEXT    NOT NULL DEFAULT '[]',   -- JSON array of strings
    sent_at         TEXT,
    internal_date   TEXT,
    flags           TEXT    NOT NULL DEFAULT '[]',   -- JSON array, e.g. ["\\Seen","\\Flagged"]
    size_bytes      INTEGER,
    has_attachment  INTEGER NOT NULL DEFAULT 0,      -- 0/1 boolean
    snippet         TEXT,
    priority        INTEGER,                         -- 1 = top, 2 = med, 3 = low; NULL = unset
    body_fetched_at TEXT,
    UNIQUE (account_id, mailbox_id, imap_uid)
);

CREATE INDEX idx_messages_account_sent ON messages (account_id, sent_at DESC);
CREATE INDEX idx_messages_thread       ON messages (thread_id);

-- Bodies are big; lazy-fetch on detail view and cache.
CREATE TABLE message_bodies (
    message_id  BLOB PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    text_plain  TEXT,
    html        TEXT,
    fetched_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

-- AI outputs. prompt_hash dedupes identical inputs (free cache hit for the model too).
CREATE TABLE ai_results (
    id                BLOB    PRIMARY KEY,
    message_id        BLOB    NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    kind              TEXT    NOT NULL,   -- 'summary' | 'translate' | 'classify' | 'draft'
    model             TEXT    NOT NULL,   -- 'claude-haiku-4-5' | 'claude-sonnet-4-6' …
    prompt_hash       TEXT    NOT NULL,   -- sha256(system + user_input)
    output            TEXT    NOT NULL,   -- JSON value, decoded with #[sqlx(json)]
    input_tokens      INTEGER,
    output_tokens     INTEGER,
    cache_read_tokens INTEGER,
    created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    UNIQUE (message_id, kind, prompt_hash)
);

CREATE INDEX idx_ai_results_message ON ai_results (message_id);

-- Tags (AI- or user-applied).
CREATE TABLE message_tags (
    message_id  BLOB NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    tag         TEXT NOT NULL,
    source      TEXT NOT NULL,   -- 'ai' | 'user'
    confidence  REAL,            -- 0..1 for AI, NULL for user
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    PRIMARY KEY (message_id, tag)
);

-- Audit log of every outbound SMTP send. Mandatory at MVP — never delete rows here.
CREATE TABLE send_log (
    id              BLOB    PRIMARY KEY,
    account_id      BLOB    NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    in_reply_to     BLOB    REFERENCES messages(id),
    to_addrs        TEXT    NOT NULL DEFAULT '[]',   -- JSON array of strings
    subject         TEXT    NOT NULL,
    ai_assisted     INTEGER NOT NULL,                -- 0/1 boolean
    sent_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now')),
    smtp_response   TEXT
);
