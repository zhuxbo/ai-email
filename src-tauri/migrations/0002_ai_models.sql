-- 0002_ai_models.sql (SQLite)
-- Configurable AI providers (Sprint 2.5 — see docs/SPEC.md § 4.3).
--
-- Two providers supported at MVP:
--   • anthropic — native /v1/messages API (prompt-cache aware)
--   • openai    — OpenAI /v1/chat/completions, covers DeepSeek / 智谱 GLM /
--                 Moonshot Kimi / 通义 Qwen / 字节豆包 via base_url override
--
-- API keys NEVER live here. They live in the OS keychain under service
-- "com.zhuxbo.aiemail.ai", username = ai_models.id (UUID).

CREATE TABLE ai_models (
    id            BLOB PRIMARY KEY,
    display_name  TEXT NOT NULL,    -- user-chosen label, e.g. "Sonnet 4.6" / "DeepSeek V3"
    provider      TEXT NOT NULL,    -- 'anthropic' | 'openai'
    model_id      TEXT NOT NULL,    -- API model id, e.g. 'claude-sonnet-4-6' / 'deepseek-chat'
    base_url      TEXT,             -- override; NULL = provider default URL
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00', 'now'))
);

-- Per-role dispatch table. ai_summarize / ai_classify / ai_translate / ai_draft_reply each
-- look up their model here on every call. ON DELETE RESTRICT prevents orphaning a role —
-- the UI must reassign before allowing the user to delete a model.
CREATE TABLE ai_role_defaults (
    role      TEXT NOT NULL PRIMARY KEY,   -- 'summary' | 'classify' | 'translate' | 'draft'
    model_id  BLOB NOT NULL REFERENCES ai_models(id) ON DELETE RESTRICT
);
