//! 系统级命令——不依赖业务逻辑，可在 DB 未就绪时调用。

use crate::db::Pool;
use crate::error::AppResult;
use crate::{AppState, DbStatusPayload};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearReport {
    pub message_bodies_deleted: u64,
    pub ai_results_deleted: u64,
}

/// 查询 DB 初始化状态——不依赖连接池，pool 未就绪时也可正常工作。
#[tauri::command]
pub async fn db_status(state: tauri::State<'_, AppState>) -> AppResult<DbStatusPayload> {
    let status = state.db_init_status.lock().await.clone();
    Ok(crate::db_init_status_to_payload(status))
}

async fn clear_local_cache(pool: &Pool) -> AppResult<CacheClearReport> {
    let message_bodies_deleted = sqlx::query("DELETE FROM message_bodies")
        .execute(pool)
        .await?
        .rows_affected();
    sqlx::query("UPDATE messages SET body_fetched_at = NULL WHERE body_fetched_at IS NOT NULL")
        .execute(pool)
        .await?;
    let ai_results_deleted = sqlx::query("DELETE FROM ai_results")
        .execute(pool)
        .await?
        .rows_affected();

    Ok(CacheClearReport {
        message_bodies_deleted,
        ai_results_deleted,
    })
}

#[tauri::command]
pub async fn cache_clear(state: tauri::State<'_, AppState>) -> AppResult<CacheClearReport> {
    clear_local_cache(state.pool().await?).await
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    async fn test_pool() -> crate::db::Pool {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        crate::db::MIGRATOR.run(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn clear_local_cache_removes_message_bodies_and_ai_results() {
        let pool = test_pool().await;
        let account_id = Uuid::new_v4();
        let mailbox_id = Uuid::new_v4();
        let message_id = Uuid::new_v4();
        let ai_result_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO accounts (
                id, email, provider, imap_host, imap_port, smtp_host, smtp_port
            )
            VALUES (?1, 'alice@example.com', 'imap', 'imap.example.com', 993, 'smtp.example.com', 465)
            "#,
        )
        .bind(account_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO mailboxes (id, account_id, name)
            VALUES (?1, ?2, 'INBOX')
            "#,
        )
        .bind(mailbox_id)
        .bind(account_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO messages (
                id, account_id, mailbox_id, imap_uid, to_addrs, cc_addrs, flags,
                body_fetched_at
            )
            VALUES (?1, ?2, ?3, 1, '[]', '[]', '[]', '2026-01-01T00:00:00.000+00:00')
            "#,
        )
        .bind(message_id)
        .bind(account_id)
        .bind(mailbox_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO message_bodies (message_id, text_plain, html)
            VALUES (?1, 'plain', '<p>html</p>')
            "#,
        )
        .bind(message_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO ai_results (id, message_id, kind, model, prompt_hash, output)
            VALUES (?1, ?2, 'summary', 'model', 'hash', '{"summary":"cached"}')
            "#,
        )
        .bind(ai_result_id)
        .bind(message_id)
        .execute(&pool)
        .await
        .unwrap();

        let report = super::clear_local_cache(&pool).await.unwrap();

        assert_eq!(report.message_bodies_deleted, 1);
        assert_eq!(report.ai_results_deleted, 1);

        let body_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM message_bodies")
            .fetch_one(&pool)
            .await
            .unwrap();
        let ai_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_results")
            .fetch_one(&pool)
            .await
            .unwrap();
        let body_fetched_at: Option<String> =
            sqlx::query_scalar("SELECT body_fetched_at FROM messages WHERE id = ?1")
                .bind(message_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(body_count, 0);
        assert_eq!(ai_count, 0);
        assert!(body_fetched_at.is_none());
    }
}
