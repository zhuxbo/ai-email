-- 0005_auto_reply.sql (SQLite) — P3c 自动回复中心
-- 两张表：用户定义规则 + 物化的建议回复队列。AI 草稿待审，绝不自动发送。

CREATE TABLE auto_reply_rules (
    id                     BLOB PRIMARY KEY,
    account_id             BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    name                   TEXT NOT NULL,
    enabled                INTEGER NOT NULL DEFAULT 1,   -- 0/1
    match_domain           TEXT,                         -- 发件地址小写子串(contains)匹配, NULL=不限
    match_category         TEXT,                         -- 'personal'|'work'|'notification'|'promotion'|'spam', NULL=不限
    match_priority_ceiling INTEGER,                      -- 命中 message.priority <= 该值(1..3, 1=最重要), NULL=不限
    draft_intent           TEXT NOT NULL,                -- 中文意图, 喂 ai_draft_reply
    created_at             TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);
CREATE INDEX idx_auto_reply_rules_account ON auto_reply_rules(account_id, enabled);

-- 物化队列：每封邮件至多一条（message_id UNIQUE）。
-- rule 删 → SET NULL（建议靠 *_snapshot 仍可展示/起草）；message 删 → CASCADE。
CREATE TABLE suggested_replies (
    id                 BLOB PRIMARY KEY,
    message_id         BLOB NOT NULL UNIQUE REFERENCES messages(id) ON DELETE CASCADE,
    rule_id            BLOB REFERENCES auto_reply_rules(id) ON DELETE SET NULL,
    intent_snapshot    TEXT NOT NULL,
    rule_name_snapshot TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending',  -- 'pending'|'dismissed'
    created_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);
