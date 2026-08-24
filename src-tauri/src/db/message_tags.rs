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
    sqlx::query("DELETE FROM message_tags WHERE message_id = ?1 AND source = 'ai'")
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
            VALUES (?1, ?2, 'ai')
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

/// 用户添加标签（source='user'）。同名 AI 行被升级为 user（重新分类不再覆写/清除它）。
/// 返回受影响行数（0 = 目标消息已删的竞态，调用方按惯例 warn）。
pub async fn set_user_tag(pool: &Pool, message_id: Uuid, tag: &str) -> AppResult<u64> {
    let res = sqlx::query(
        r#"
        INSERT INTO message_tags (message_id, tag, source)
        VALUES (?1, ?2, 'user')
        ON CONFLICT (message_id, tag) DO UPDATE
        SET source = 'user', confidence = NULL
        "#,
    )
    .bind(message_id)
    .bind(tag)
    .execute(pool)
    .await?;
    Ok(res.rows_affected())
}

/// 删除一个标签行（无论来源）。删 AI 标签后，下次分类可能重新加上——这是已知语义。
/// 返回受影响行数（0 = 行本就不存在，幂等）。
pub async fn remove_tag(pool: &Pool, message_id: Uuid, tag: &str) -> AppResult<u64> {
    let res = sqlx::query("DELETE FROM message_tags WHERE message_id = ?1 AND tag = ?2")
        .bind(message_id)
        .bind(tag)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// 当前标签总数（命令层的每消息上限检查用）。
pub async fn count_tags(pool: &Pool, message_id: Uuid) -> AppResult<i64> {
    let n: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM message_tags WHERE message_id = ?1")
        .bind(message_id)
        .fetch_one(pool)
        .await?;
    Ok(n.0)
}

#[cfg(test)]
mod user_tag_tests {
    use super::*;
    use crate::db;

    async fn seed(pool: &Pool) -> Uuid {
        let acc = crate::db::test_seed::seed_account(pool).await;
        let mb = crate::db::test_seed::seed_mailbox(pool, acc, "INBOX", None).await;
        crate::db::test_seed::seed_msg(pool, acc, mb, 1, "a@x.com", None, None, false, "[]").await
    }

    #[tokio::test]
    async fn set_user_tag_upgrades_ai_row_and_survives_reclassify() {
        let pool = db::test_pool().await;
        let id = seed(&pool).await;

        replace_ai_tags(&pool, id, &["work".into()]).await.unwrap();
        assert_eq!(set_user_tag(&pool, id, "work").await.unwrap(), 1);

        // 再跑一次 AI 分类：不得清除用户升级过的 work，只更新 AI 侧。
        replace_ai_tags(&pool, id, &["urgent".into()])
            .await
            .unwrap();
        let tags: Vec<(String, String)> =
            sqlx::query_as("SELECT tag, source FROM message_tags WHERE message_id=?1 ORDER BY tag")
                .bind(id)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            tags,
            vec![
                ("urgent".into(), "ai".into()),
                ("work".into(), "user".into())
            ],
            "用户标签须在重新分类后存活"
        );
    }

    #[tokio::test]
    async fn remove_tag_deletes_any_source_and_is_idempotent() {
        let pool = db::test_pool().await;
        let id = seed(&pool).await;
        replace_ai_tags(&pool, id, &["ai-tag".into()])
            .await
            .unwrap();
        set_user_tag(&pool, id, "my-tag").await.unwrap();

        assert_eq!(remove_tag(&pool, id, "ai-tag").await.unwrap(), 1);
        assert_eq!(
            remove_tag(&pool, id, "ai-tag").await.unwrap(),
            0,
            "重复删除幂等"
        );
        assert_eq!(count_tags(&pool, id).await.unwrap(), 1);
    }
}
