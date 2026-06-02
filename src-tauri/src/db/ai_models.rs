//! Repository for the `ai_models` table — configurable AI provider entries.
//!
//! API keys NEVER appear here. They live in the OS keychain under service
//! "com.zhuxbo.aiemail.ai", keyed by `ai_models.id`.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: Uuid,
    pub display_name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelInput {
    pub display_name: String,
    pub provider: String,
    pub model_id: String,
    pub base_url: Option<String>,
}

pub async fn insert(pool: &Pool, input: &AiModelInput) -> AppResult<AiModel> {
    let row = sqlx::query_as::<_, AiModel>(
        r#"
        INSERT INTO ai_models (id, display_name, provider, model_id, base_url)
        VALUES (?1, ?2, ?3, ?4, ?5)
        RETURNING id, display_name, provider, model_id, base_url, created_at
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&input.display_name)
    .bind(&input.provider)
    .bind(&input.model_id)
    .bind(&input.base_url)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn get(pool: &Pool, id: Uuid) -> AppResult<Option<AiModel>> {
    let row = sqlx::query_as::<_, AiModel>(
        r#"
        SELECT id, display_name, provider, model_id, base_url, created_at
        FROM ai_models
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn list(pool: &Pool) -> AppResult<Vec<AiModel>> {
    let rows = sqlx::query_as::<_, AiModel>(
        r#"
        SELECT id, display_name, provider, model_id, base_url, created_at
        FROM ai_models
        ORDER BY created_at
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete the row. The `ai_role_defaults` FK has ON DELETE RESTRICT — SQLite errors out
/// (and so do we) if any role still points here. The caller (UI) is expected to reassign
/// first; we surface the FK error verbatim so the user sees "this model is still the
/// default for: summary" rather than a generic failure.
pub async fn delete(pool: &Pool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM ai_models WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
