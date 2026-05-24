//! Repository for the `ai_results` table.
//!
//! Acts as the dedupe layer in front of the Anthropic API: every call computes a
//! `prompt_hash` over `system + user_input` and looks up the table before hitting the
//! network. Identical inputs hit the cache for free (no token cost). The Anthropic
//! prompt-cache breakpoint on the system block is orthogonal — when WE miss the DB cache,
//! the API call still benefits from its own 5-minute cache.

use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AiResult {
    pub id: Uuid,
    pub message_id: Uuid,
    pub kind: String,
    pub model: String,
    pub prompt_hash: String,
    pub output: serde_json::Value,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub cache_read_tokens: Option<i32>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct AiResultInsert {
    pub message_id: Uuid,
    pub kind: String,
    pub model: String,
    pub prompt_hash: String,
    pub output: serde_json::Value,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub cache_read_tokens: Option<i32>,
}

pub async fn get(
    pool: &Pool,
    message_id: Uuid,
    kind: &str,
    prompt_hash: &str,
) -> AppResult<Option<AiResult>> {
    let row = sqlx::query_as::<_, AiResult>(
        r#"
        SELECT id, message_id, kind, model, prompt_hash, output,
               input_tokens, output_tokens, cache_read_tokens, created_at
        FROM ai_results
        WHERE message_id = $1 AND kind = $2 AND prompt_hash = $3
        "#,
    )
    .bind(message_id)
    .bind(kind)
    .bind(prompt_hash)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// INSERT ... ON CONFLICT DO NOTHING. Returns the resulting row regardless of whether we
/// inserted or another caller raced us — useful so callers always have a stable AiResult to
/// hand back to the UI.
pub async fn insert(pool: &Pool, r: &AiResultInsert) -> AppResult<AiResult> {
    let row = sqlx::query_as::<_, AiResult>(
        r#"
        INSERT INTO ai_results (
            message_id, kind, model, prompt_hash, output,
            input_tokens, output_tokens, cache_read_tokens
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (message_id, kind, prompt_hash) DO UPDATE
        SET output = EXCLUDED.output
        RETURNING id, message_id, kind, model, prompt_hash, output,
                  input_tokens, output_tokens, cache_read_tokens, created_at
        "#,
    )
    .bind(r.message_id)
    .bind(&r.kind)
    .bind(&r.model)
    .bind(&r.prompt_hash)
    .bind(&r.output)
    .bind(r.input_tokens)
    .bind(r.output_tokens)
    .bind(r.cache_read_tokens)
    .fetch_one(pool)
    .await?;
    Ok(row)
}
