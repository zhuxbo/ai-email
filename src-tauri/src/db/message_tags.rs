//! Repository for the `message_tags` table.
//!
//! Two sources of tags: `'ai'` (auto-classification) and `'user'` (manual). They coexist
//! in the table but use distinct semantics — `replace_ai_tags` only clears AI-sourced
//! rows, preserving user labels across reclassification.

use crate::db::Pool;
use crate::error::AppResult;
use uuid::Uuid;

/// Replace this message's AI-sourced tags with the given set. User-sourced tags are kept.
/// Runs inside a transaction so a partial update can never leave conflicting state.
pub async fn replace_ai_tags(pool: &Pool, message_id: Uuid, tags: &[String]) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM message_tags WHERE message_id = $1 AND source = 'ai'")
        .bind(message_id)
        .execute(&mut *tx)
        .await?;
    for tag in tags {
        if tag.trim().is_empty() {
            continue;
        }
        // ON CONFLICT DO NOTHING — if a user already added the same tag, the user row stays.
        sqlx::query(
            r#"
            INSERT INTO message_tags (message_id, tag, source)
            VALUES ($1, $2, 'ai')
            ON CONFLICT (message_id, tag) DO NOTHING
            "#,
        )
        .bind(message_id)
        .bind(tag.trim())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
