-- 0010_filter_rules.sql (SQLite) — Plan B AI 过滤规则
-- 会话感知增量提取的"例外/定制层"：默认三类（签名/引用/重复）全剥，本表是叠加规则。
-- 全局 scope 三级 + 优先级解析（email > domain > global），解析逻辑在 db::filter_rules::resolve_for。

CREATE TABLE filter_rules (
    id          BLOB PRIMARY KEY,
    scope       TEXT NOT NULL,        -- 'global' | 'domain' | 'email'
    scope_value TEXT NOT NULL,        -- 域名/邮箱；global 存空串 '' (让 UNIQUE 生效，避 SQLite 多-NULL 不约束)
    target      TEXT NOT NULL,        -- 'signature' | 'quote' | 'repeat'
    action      TEXT NOT NULL,        -- 'keep' | 'strip'
    pattern     TEXT,                 -- 可选 Rust regex；NULL/'' = 无条件
    enabled     INTEGER NOT NULL DEFAULT 1,
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);
CREATE UNIQUE INDEX idx_filter_rules_unique ON filter_rules(scope, scope_value, target);
CREATE INDEX idx_filter_rules_lookup ON filter_rules(scope, scope_value);

-- per-message override：某封禁用过滤（AI 收完整原文）。不影响他封剥离。
ALTER TABLE messages ADD COLUMN filter_disabled INTEGER NOT NULL DEFAULT 0;
