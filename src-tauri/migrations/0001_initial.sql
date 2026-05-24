-- 0001_initial.sql
-- Initial schema for AI Email. See docs/SPEC.md § 3 for the canonical data model.
--
-- Conventions:
--   • PG 18 ships gen_random_uuid() in core (no pgcrypto extension required).
--   • Timestamps are TIMESTAMPTZ everywhere; rows persist UTC and clients render local.
--   • Credentials NEVER live in this database — they live in the OS keychain keyed by accounts.id.
--   • All FKs cascade so deleting an account purges every derived row.

-- Email accounts. Only non-secret metadata; the auth code lives in the OS keychain.
CREATE TABLE accounts (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    email           TEXT        NOT NULL UNIQUE,
    display_name    TEXT,
    provider        TEXT        NOT NULL,    -- 'qq' | '163' | 'gmail' | 'outlook' | 'imap'
    imap_host       TEXT        NOT NULL,
    imap_port       INT         NOT NULL DEFAULT 993,
    smtp_host       TEXT        NOT NULL,
    smtp_port       INT         NOT NULL DEFAULT 465,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_synced_at  TIMESTAMPTZ
);

-- IMAP folders, mirrored per account.
CREATE TABLE mailboxes (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID        NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name            TEXT        NOT NULL,    -- 'INBOX' | '已发送' | '收件箱' …
    delimiter       TEXT,
    uid_validity    BIGINT,
    uid_next        BIGINT,
    last_synced_at  TIMESTAMPTZ,
    UNIQUE (account_id, name)
);

-- Header-only rows, keyed by IMAP UID. Bodies live in message_bodies.
CREATE TABLE messages (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID        NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    mailbox_id      UUID        NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    imap_uid        BIGINT      NOT NULL,
    rfc_message_id  TEXT,                                   -- RFC 822 Message-ID
    thread_id       TEXT,                                   -- derived from References / In-Reply-To
    subject         TEXT,
    from_addr       TEXT,
    to_addrs        TEXT[],
    cc_addrs        TEXT[],
    sent_at         TIMESTAMPTZ,
    internal_date   TIMESTAMPTZ,
    flags           TEXT[]      NOT NULL DEFAULT '{}',      -- '\Seen' | '\Flagged' …
    size_bytes      INT,
    has_attachment  BOOL        NOT NULL DEFAULT FALSE,
    snippet         TEXT,                                   -- first ~200 chars for list view
    priority        INT,                                    -- 1 = top, 2 = med, 3 = low; NULL = unset
    body_fetched_at TIMESTAMPTZ,
    UNIQUE (account_id, mailbox_id, imap_uid)
);

CREATE INDEX idx_messages_account_sent ON messages (account_id, sent_at DESC);
CREATE INDEX idx_messages_thread       ON messages (thread_id);

-- Bodies are big; lazy-fetch on detail view and cache.
CREATE TABLE message_bodies (
    message_id      UUID        PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    text_plain      TEXT,
    html            TEXT,
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- AI outputs. prompt_hash dedupes identical inputs (free cache hit for the model too).
CREATE TABLE ai_results (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id        UUID        NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    kind              TEXT        NOT NULL,   -- 'summary' | 'translate' | 'classify' | 'draft'
    model             TEXT        NOT NULL,   -- 'claude-haiku-4-5' | 'claude-sonnet-4-6' …
    prompt_hash       TEXT        NOT NULL,   -- sha256(system + user_input)
    output            JSONB       NOT NULL,
    input_tokens      INT,
    output_tokens     INT,
    cache_read_tokens INT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (message_id, kind, prompt_hash)
);

CREATE INDEX idx_ai_results_message ON ai_results (message_id);

-- Tags (AI- or user-applied).
CREATE TABLE message_tags (
    message_id  UUID        NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    tag         TEXT        NOT NULL,
    source      TEXT        NOT NULL,   -- 'ai' | 'user'
    confidence  REAL,                   -- 0..1 for AI, NULL for user
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, tag)
);

-- Audit log of every outbound SMTP send. Mandatory at MVP — never delete rows here.
CREATE TABLE send_log (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      UUID        NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    in_reply_to     UUID        REFERENCES messages(id),
    to_addrs        TEXT[]      NOT NULL,
    subject         TEXT        NOT NULL,
    ai_assisted     BOOL        NOT NULL,
    sent_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    smtp_response   TEXT
);
