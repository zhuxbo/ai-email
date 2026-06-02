//! Repository for the `ai_role_defaults` table — which model serves which role.
//!
//! Roles are open-set at the SQL level (TEXT PRIMARY KEY) but the Rust layer only knows
//! the four MVP roles: `summary` / `classify` / `translate` / `draft`.

use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::db::ai_models::AiModel;
use crate::db::Pool;
use crate::error::AppResult;

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RoleDefault {
    pub role: String,
    pub model_id: Uuid,
}

/// Returns every (role, model) mapping currently configured. Used by the settings UI to
/// render the role → model picker.
pub async fn list(pool: &Pool) -> AppResult<Vec<RoleDefault>> {
    let rows = sqlx::query_as::<_, RoleDefault>(
        r#"SELECT role, model_id FROM ai_role_defaults ORDER BY role"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// UPSERT — there's at most one row per role.
pub async fn set(pool: &Pool, role: &str, model_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO ai_role_defaults (role, model_id)
        VALUES (?1, ?2)
        ON CONFLICT (role) DO UPDATE SET model_id = excluded.model_id
        "#,
    )
    .bind(role)
    .bind(model_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear(pool: &Pool, role: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM ai_role_defaults WHERE role = ?1")
        .bind(role)
        .execute(pool)
        .await?;
    Ok(())
}

/// Fast lookup used by `ai_summarize` / `ai_classify` / etc.: JOIN through `ai_role_defaults`
/// to fetch the model in one query. Returns None when the role has no default configured —
/// the command layer turns that into a user-facing "请先在设置中配置模型" error.
pub async fn resolve_model(pool: &Pool, role: &str) -> AppResult<Option<AiModel>> {
    let row = sqlx::query_as::<_, AiModel>(
        r#"
        SELECT m.id, m.display_name, m.provider, m.model_id, m.base_url, m.created_at
        FROM ai_role_defaults d
        JOIN ai_models m ON m.id = d.model_id
        WHERE d.role = ?1
        "#,
    )
    .bind(role)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
